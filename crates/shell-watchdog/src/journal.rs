use serde::{Deserialize, Serialize};
use serde_json::Value;
use shell_core::{FixedHexError, RecoveryTransactionId};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const RECOVERY_JOURNAL_SCHEMA_V1: u32 = 1;
pub const RECOVERY_JOURNAL_FILE_NAME: &str = "taskbar-recovery-v1.json";
pub const MAX_RECOVERY_JOURNAL_BYTES: usize = 256 * 1024;

static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryJournalPhase {
    Prepared,
    Applied,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskbarBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl TaskbarBounds {
    #[must_use]
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskbarSnapshot {
    pub device: String,
    #[serde(rename = "class")]
    pub class_name: String,
    pub visible: bool,
    pub bounds: TaskbarBounds,
}

impl TaskbarSnapshot {
    #[must_use]
    pub fn new(
        device: impl Into<String>,
        class_name: impl Into<String>,
        visible: bool,
        bounds: TaskbarBounds,
    ) -> Self {
        Self {
            device: device.into(),
            class_name: class_name.into(),
            visible,
            bounds,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryJournalV1 {
    #[serde(rename = "schema")]
    schema_version: u32,
    pub transaction_id: String,
    pub phase: RecoveryJournalPhase,
    pub appbar_state: u32,
    pub snapshots: Vec<TaskbarSnapshot>,
}

impl RecoveryJournalV1 {
    #[must_use]
    pub fn new(
        transaction_id: impl Into<String>,
        phase: RecoveryJournalPhase,
        appbar_state: u32,
        snapshots: Vec<TaskbarSnapshot>,
    ) -> Self {
        Self {
            schema_version: RECOVERY_JOURNAL_SCHEMA_V1,
            transaction_id: transaction_id.into(),
            phase,
            appbar_state,
            snapshots,
        }
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub const fn set_phase(&mut self, phase: RecoveryJournalPhase) {
        self.phase = phase;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryJournalError {
    #[error("failed to {operation} recovery journal `{path}`: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "recovery journal `{path}` is {actual_bytes} bytes, exceeding the {max_bytes}-byte limit"
    )]
    Oversized {
        path: PathBuf,
        actual_bytes: u64,
        max_bytes: usize,
    },
    #[error("recovery journal `{path}` contains invalid JSON: {source}")]
    Corrupt {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("recovery journal `{path}` has a missing or invalid `schema` field")]
    InvalidSchema { path: PathBuf },
    #[error(
        "recovery journal `{path}` uses future schema {found}; this build supports schema {supported}"
    )]
    FutureSchema {
        path: PathBuf,
        found: u64,
        supported: u32,
    },
    #[error(
        "recovery journal `{path}` uses unsupported schema {found}; this build supports schema {supported}"
    )]
    UnsupportedSchema {
        path: PathBuf,
        found: u64,
        supported: u32,
    },
    #[error("failed to encode recovery journal `{path}`: {source}")]
    Encode {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("recovery journal path `{path}` has no file name")]
    InvalidPath { path: PathBuf },
    #[error("could not allocate a staging file next to recovery journal `{path}`")]
    StagingCollision { path: PathBuf },
    #[error("recovery journal `{path}` already exists and was preserved")]
    AlreadyExists { path: PathBuf },
    #[error("recovery journal `{path}` does not exist")]
    Missing { path: PathBuf },
    #[error("recovery journal `{path}` contains a non-canonical transaction id: {source}")]
    InvalidTransactionId {
        path: PathBuf,
        #[source]
        source: FixedHexError,
    },
    #[error(
        "recovery journal `{path}` belongs to transaction {found}, not expected transaction {expected}"
    )]
    TransactionMismatch {
        path: PathBuf,
        expected: RecoveryTransactionId,
        found: RecoveryTransactionId,
    },
    #[error("recovery journal `{path}` is in phase {found:?}; expected phase {expected:?}")]
    UnexpectedPhase {
        path: PathBuf,
        expected: RecoveryJournalPhase,
        found: RecoveryJournalPhase,
    },
}

#[must_use]
pub fn recovery_journal_path(state_directory: &Path) -> PathBuf {
    state_directory.join(RECOVERY_JOURNAL_FILE_NAME)
}

pub fn save_recovery_journal(
    path: &Path,
    journal: &RecoveryJournalV1,
) -> Result<(), RecoveryJournalError> {
    let parent = journal_parent(path);
    prepare_parent(path, parent)?;
    let _lock = acquire_journal_lock(path, parent)?;
    save_recovery_journal_locked(path, parent, journal)
}

/// Creates the initial `Prepared` journal without replacing any existing file.
///
/// Publishing uses a same-directory staging file followed by an atomic hard
/// link. The link operation fails if `path` already exists, so a pending,
/// corrupt, or future journal is never overwritten by a new transaction.
pub fn create_prepared_recovery_journal(
    path: &Path,
    transaction_id: RecoveryTransactionId,
    appbar_state: u32,
    snapshots: Vec<TaskbarSnapshot>,
) -> Result<RecoveryJournalV1, RecoveryJournalError> {
    let journal = RecoveryJournalV1::new(
        transaction_id.to_string(),
        RecoveryJournalPhase::Prepared,
        appbar_state,
        snapshots,
    );
    let encoded = encode_journal(path, &journal)?;
    let parent = journal_parent(path);
    prepare_parent(path, parent)?;
    let _lock = acquire_journal_lock(path, parent)?;

    let (staging_path, mut staging_file) = create_staging_file(path, parent)?;
    let staged = write_staging_data(path, &mut staging_file, &encoded);
    drop(staging_file);
    if let Err(error) = staged {
        let _ignored = fs::remove_file(&staging_path);
        return Err(error);
    }

    let published = publish_without_replace(path, parent, &staging_path);
    let _ignored = fs::remove_file(&staging_path);
    if published.is_ok() {
        // The authoritative link was already synced. This second sync only
        // makes best-effort cleanup of the staging link durable.
        let _ignored = sync_parent_directory(parent, path);
    }
    published.map(|()| journal)
}

/// Authenticates and durably transitions one journal from `Prepared` to
/// `Applied`.
///
/// The expected typed transaction id is checked while holding the journal's
/// cross-process lock. Invalid JSON, an unknown schema, a different transaction
/// id, or any phase other than `Prepared` fails closed without replacing the
/// existing journal.
pub fn mark_recovery_journal_applied(
    path: &Path,
    expected_transaction_id: RecoveryTransactionId,
) -> Result<RecoveryJournalV1, RecoveryJournalError> {
    let parent = journal_parent(path);
    prepare_parent(path, parent)?;
    let _lock = acquire_journal_lock(path, parent)?;
    let Some(mut journal) = load_recovery_journal(path)? else {
        return Err(RecoveryJournalError::Missing {
            path: path.to_path_buf(),
        });
    };
    let found_transaction_id = parse_transaction_id(path, &journal)?;
    authenticate_transaction(path, expected_transaction_id, found_transaction_id)?;
    if journal.phase != RecoveryJournalPhase::Prepared {
        return Err(RecoveryJournalError::UnexpectedPhase {
            path: path.to_path_buf(),
            expected: RecoveryJournalPhase::Prepared,
            found: journal.phase,
        });
    }

    journal.set_phase(RecoveryJournalPhase::Applied);
    save_recovery_journal_locked(path, parent, &journal)?;
    Ok(journal)
}

fn save_recovery_journal_locked(
    path: &Path,
    parent: &Path,
    journal: &RecoveryJournalV1,
) -> Result<(), RecoveryJournalError> {
    let encoded = encode_journal(path, journal)?;

    let (staging_path, mut staging_file) = create_staging_file(path, parent)?;
    let staged = write_staging_data(path, &mut staging_file, &encoded);
    drop(staging_file);
    let result = staged.and_then(|()| commit_staging_file(path, parent, &staging_path));
    if result.is_err() {
        let _ignored = fs::remove_file(&staging_path);
    }
    result
}

pub fn load_recovery_journal(
    path: &Path,
) -> Result<Option<RecoveryJournalV1>, RecoveryJournalError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(io_error("inspect", path, source)),
    };
    ensure_bounded(path, metadata.len())?;

    let file = File::open(path).map_err(|source| io_error("open", path, source))?;
    let mut encoded = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_RECOVERY_JOURNAL_BYTES + 1) as u64)
        .read_to_end(&mut encoded)
        .map_err(|source| io_error("read", path, source))?;
    ensure_bounded(path, encoded.len() as u64)?;

    let value: Value =
        serde_json::from_slice(&encoded).map_err(|source| RecoveryJournalError::Corrupt {
            path: path.to_path_buf(),
            source,
        })?;
    validate_schema(path, &value)?;
    let journal =
        serde_json::from_value(value).map_err(|source| RecoveryJournalError::Corrupt {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(Some(journal))
}

pub fn remove_recovery_journal(path: &Path) -> Result<(), RecoveryJournalError> {
    let parent = journal_parent(path);
    prepare_parent(path, parent)?;
    let _lock = acquire_journal_lock(path, parent)?;
    remove_recovery_journal_locked(path, parent)
}

/// Removes a journal only when it still belongs to the expected transaction.
///
/// Absence is an idempotent success. A corrupt journal, non-canonical id, or a
/// different transaction fails closed while the journal lock is held, so a
/// stale restorer cannot delete a newer recovery transaction.
pub fn remove_recovery_journal_for_transaction(
    path: &Path,
    expected_transaction_id: RecoveryTransactionId,
) -> Result<(), RecoveryJournalError> {
    let parent = journal_parent(path);
    prepare_parent(path, parent)?;
    let _lock = acquire_journal_lock(path, parent)?;
    let Some(journal) = load_recovery_journal(path)? else {
        return Ok(());
    };
    let found_transaction_id = parse_transaction_id(path, &journal)?;
    authenticate_transaction(path, expected_transaction_id, found_transaction_id)?;
    remove_recovery_journal_locked(path, parent)
}

fn remove_recovery_journal_locked(path: &Path, parent: &Path) -> Result<(), RecoveryJournalError> {
    match fs::remove_file(path) {
        Ok(()) => {
            sync_parent_directory(parent, path)?;
            Ok(())
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error("remove", path, source)),
    }
}

fn encode_journal(
    path: &Path,
    journal: &RecoveryJournalV1,
) -> Result<Vec<u8>, RecoveryJournalError> {
    let encoded =
        serde_json::to_vec_pretty(journal).map_err(|source| RecoveryJournalError::Encode {
            path: path.to_path_buf(),
            source,
        })?;
    ensure_bounded(path, encoded.len() as u64)?;
    Ok(encoded)
}

fn parse_transaction_id(
    path: &Path,
    journal: &RecoveryJournalV1,
) -> Result<RecoveryTransactionId, RecoveryJournalError> {
    journal
        .transaction_id
        .parse()
        .map_err(|source| RecoveryJournalError::InvalidTransactionId {
            path: path.to_path_buf(),
            source,
        })
}

fn authenticate_transaction(
    path: &Path,
    expected: RecoveryTransactionId,
    found: RecoveryTransactionId,
) -> Result<(), RecoveryJournalError> {
    if found == expected {
        Ok(())
    } else {
        Err(RecoveryJournalError::TransactionMismatch {
            path: path.to_path_buf(),
            expected,
            found,
        })
    }
}

fn prepare_parent(path: &Path, parent: &Path) -> Result<(), RecoveryJournalError> {
    fs::create_dir_all(parent).map_err(|source| io_error("create its directory for", path, source))
}

fn acquire_journal_lock(path: &Path, parent: &Path) -> Result<File, RecoveryJournalError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| RecoveryJournalError::InvalidPath {
            path: path.to_path_buf(),
        })?
        .to_string_lossy();
    let lock_path = parent.join(format!(".{file_name}.lock"));
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|source| io_error("open its transaction lock for", path, source))?;
    lock.lock()
        .map_err(|source| io_error("lock its transaction state for", path, source))?;
    Ok(lock)
}

fn publish_without_replace(
    path: &Path,
    parent: &Path,
    staging_path: &Path,
) -> Result<(), RecoveryJournalError> {
    match fs::hard_link(staging_path, path) {
        Ok(()) => {
            sync_committed_file(path)?;
            sync_parent_directory(parent, path)
        }
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
            Err(RecoveryJournalError::AlreadyExists {
                path: path.to_path_buf(),
            })
        }
        Err(source) => Err(io_error("publish without replacing", path, source)),
    }
}

fn write_staging_data(
    path: &Path,
    staging_file: &mut File,
    encoded: &[u8],
) -> Result<(), RecoveryJournalError> {
    staging_file
        .write_all(encoded)
        .map_err(|source| io_error("write staging data for", path, source))?;
    staging_file
        .flush()
        .map_err(|source| io_error("flush staging data for", path, source))?;
    staging_file
        .sync_all()
        .map_err(|source| io_error("sync staging data for", path, source))
}

fn commit_staging_file(
    path: &Path,
    parent: &Path,
    staging_path: &Path,
) -> Result<(), RecoveryJournalError> {
    replace_staging_file(staging_path, path).map_err(|source| io_error("commit", path, source))?;
    sync_committed_file(path)?;
    sync_parent_directory(parent, path)
}

#[cfg(windows)]
fn replace_staging_file(staging_path: &Path, path: &Path) -> io::Result<()> {
    crate::taskbar_restore_win32::replace_recovery_journal(staging_path, path)
}

#[cfg(not(windows))]
fn replace_staging_file(staging_path: &Path, path: &Path) -> io::Result<()> {
    fs::rename(staging_path, path)
}

fn sync_committed_file(path: &Path) -> Result<(), RecoveryJournalError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|source| io_error("sync committed data for", path, source))
}

fn create_staging_file(
    path: &Path,
    parent: &Path,
) -> Result<(PathBuf, File), RecoveryJournalError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| RecoveryJournalError::InvalidPath {
            path: path.to_path_buf(),
        })?
        .to_string_lossy();
    for _attempt in 0..16 {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let staging_path = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging_path)
        {
            Ok(file) => return Ok((staging_path, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => return Err(io_error("create staging data for", path, source)),
        }
    }
    Err(RecoveryJournalError::StagingCollision {
        path: path.to_path_buf(),
    })
}

fn validate_schema(path: &Path, value: &Value) -> Result<(), RecoveryJournalError> {
    let Some(found) = value.get("schema").and_then(Value::as_u64) else {
        return Err(RecoveryJournalError::InvalidSchema {
            path: path.to_path_buf(),
        });
    };
    if found > u64::from(RECOVERY_JOURNAL_SCHEMA_V1) {
        return Err(RecoveryJournalError::FutureSchema {
            path: path.to_path_buf(),
            found,
            supported: RECOVERY_JOURNAL_SCHEMA_V1,
        });
    }
    if found < u64::from(RECOVERY_JOURNAL_SCHEMA_V1) {
        return Err(RecoveryJournalError::UnsupportedSchema {
            path: path.to_path_buf(),
            found,
            supported: RECOVERY_JOURNAL_SCHEMA_V1,
        });
    }
    Ok(())
}

fn ensure_bounded(path: &Path, actual_bytes: u64) -> Result<(), RecoveryJournalError> {
    if actual_bytes > MAX_RECOVERY_JOURNAL_BYTES as u64 {
        return Err(RecoveryJournalError::Oversized {
            path: path.to_path_buf(),
            actual_bytes,
            max_bytes: MAX_RECOVERY_JOURNAL_BYTES,
        });
    }
    Ok(())
}

fn journal_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path, journal_path: &Path) -> Result<(), RecoveryJournalError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| io_error("sync its parent directory for", journal_path, source))
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path, _journal_path: &Path) -> Result<(), RecoveryJournalError> {
    Ok(())
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> RecoveryJournalError {
    RecoveryJournalError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}
