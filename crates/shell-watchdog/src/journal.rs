use serde::{Deserialize, Serialize};
use serde_json::Value;
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
}

#[must_use]
pub fn recovery_journal_path(state_directory: &Path) -> PathBuf {
    state_directory.join(RECOVERY_JOURNAL_FILE_NAME)
}

pub fn save_recovery_journal(
    path: &Path,
    journal: &RecoveryJournalV1,
) -> Result<(), RecoveryJournalError> {
    let encoded =
        serde_json::to_vec_pretty(journal).map_err(|source| RecoveryJournalError::Encode {
            path: path.to_path_buf(),
            source,
        })?;
    ensure_bounded(path, encoded.len() as u64)?;

    let parent = journal_parent(path);
    fs::create_dir_all(parent)
        .map_err(|source| io_error("create its directory for", path, source))?;

    let (staging_path, mut staging_file) = create_staging_file(path, parent)?;
    let result = write_and_commit(path, parent, &staging_path, &mut staging_file, &encoded);
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
    match fs::remove_file(path) {
        Ok(()) => {
            sync_parent_directory(journal_parent(path), path)?;
            Ok(())
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error("remove", path, source)),
    }
}

fn write_and_commit(
    path: &Path,
    parent: &Path,
    staging_path: &Path,
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
        .map_err(|source| io_error("sync staging data for", path, source))?;
    fs::rename(staging_path, path).map_err(|source| io_error("commit", path, source))?;
    sync_parent_directory(parent, path)
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
