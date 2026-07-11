use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::{
    ConfigLoad, RecoveryKind, RecoveryReport, ShellConfigV1, decode_config, encode_config,
};

/// Same-directory paths participating in a replace operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicPaths {
    destination: PathBuf,
    temporary: PathBuf,
    backup: PathBuf,
    backup_temporary: PathBuf,
    rollback: PathBuf,
}

impl AtomicPaths {
    fn new(destination: PathBuf) -> Self {
        Self {
            temporary: sibling(&destination, "tmp"),
            backup: sibling(&destination, "bak"),
            backup_temporary: sibling(&destination, "bak.tmp"),
            rollback: sibling(&destination, "previous"),
            destination,
        }
    }

    /// Returns the final configuration path.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }
    /// Returns the same-directory temporary path.
    #[must_use]
    pub fn temporary(&self) -> &Path {
        &self.temporary
    }
    /// Returns the preserved previous-valid backup path.
    #[must_use]
    pub fn backup(&self) -> &Path {
        &self.backup
    }
}

/// Replace seam used to test interruption without filesystem races.
pub trait AtomicWriter {
    /// Persists bytes or leaves the previous destination usable.
    fn replace(&self, paths: &AtomicPaths, contents: &[u8]) -> Result<(), PersistenceError>;
}

#[derive(Debug, Default, Clone, Copy)]
struct StdAtomicWriter;

impl AtomicWriter for StdAtomicWriter {
    fn replace(&self, paths: &AtomicPaths, contents: &[u8]) -> Result<(), PersistenceError> {
        if let Some(parent) = paths.destination.parent() {
            fs::create_dir_all(parent)?;
        }
        remove_if_exists(&paths.temporary)?;
        write_synced(&paths.temporary, contents)?;
        if paths.destination.exists() {
            remove_if_exists(&paths.backup_temporary)?;
            fs::copy(&paths.destination, &paths.backup_temporary)?;
            sync_file(&paths.backup_temporary)?;
            remove_if_exists(&paths.backup)?;
            fs::rename(&paths.backup_temporary, &paths.backup)?;
            remove_if_exists(&paths.rollback)?;
            fs::rename(&paths.destination, &paths.rollback)?;
        }
        match fs::rename(&paths.temporary, &paths.destination) {
            Ok(()) => {
                remove_if_exists(&paths.rollback)?;
                Ok(())
            }
            Err(error) => {
                if paths.rollback.exists() && !paths.destination.exists() {
                    fs::rename(&paths.rollback, &paths.destination)?;
                }
                Err(PersistenceError::Io(error))
            }
        }
    }
}

/// Source used to obtain a usable current configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Primary file.
    Primary,
    /// Preserved previous-valid backup.
    Backup,
    /// Built-in safe defaults.
    Defaults,
}

/// Store load result including recovery provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreLoad {
    config: ShellConfigV1,
    source: ConfigSource,
    report: Option<RecoveryReport>,
}

impl StoreLoad {
    /// Borrows the usable current configuration.
    #[must_use]
    pub const fn config(&self) -> &ShellConfigV1 {
        &self.config
    }
    /// Returns the source that supplied the configuration.
    #[must_use]
    pub const fn source(&self) -> ConfigSource {
        self.source
    }
    /// Returns recovery details when fallback was required.
    #[must_use]
    pub const fn report(&self) -> Option<RecoveryReport> {
        self.report
    }
}

/// Filesystem owner for validated versioned configuration.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    paths: AtomicPaths,
}

impl ConfigStore {
    /// Creates a store for one primary file and same-directory siblings.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            paths: AtomicPaths::new(path),
        }
    }
    /// Returns the primary path.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.paths.destination()
    }
    /// Returns the preserved backup path.
    #[must_use]
    pub fn backup_path(&self) -> &Path {
        self.paths.backup()
    }

    /// Loads primary, then backup, then explicit safe defaults.
    #[must_use]
    pub fn load(&self) -> StoreLoad {
        if let Some(load) = read_valid(&self.paths.destination) {
            return StoreLoad {
                config: load.config().clone(),
                source: ConfigSource::Primary,
                report: report_of(&load),
            };
        }
        if let Some(load) = read_valid(&self.paths.backup) {
            return StoreLoad {
                config: load.config().clone(),
                source: ConfigSource::Backup,
                report: Some(RecoveryReport::new(RecoveryKind::Backup, None)),
            };
        }
        let kind = if self.paths.destination.exists() {
            RecoveryKind::Malformed
        } else {
            RecoveryKind::Missing
        };
        StoreLoad {
            config: ShellConfigV1::default(),
            source: ConfigSource::Defaults,
            report: Some(RecoveryReport::new(kind, None)),
        }
    }

    /// Persists with a same-directory, synced standard-library fallback.
    ///
    /// The previous valid bytes are retained in `.bak` before replacement. Standard Rust does
    /// not expose Windows `ReplaceFileW`, so a process crash can briefly leave only that backup;
    /// `load` treats it as an explicit recovery source. A native adapter should replace this
    /// writer when strict single-operation Windows replacement becomes available.
    pub fn persist(&self, config: &ShellConfigV1) -> Result<(), PersistenceError> {
        self.persist_with(config, &StdAtomicWriter)
    }

    /// Persists through an interruption-testable replace strategy.
    pub fn persist_with<W: AtomicWriter>(
        &self,
        config: &ShellConfigV1,
        writer: &W,
    ) -> Result<(), PersistenceError> {
        let bytes = encode_config(config).map_err(PersistenceError::InvalidConfig)?;
        writer.replace(&self.paths, &bytes)
    }
}

fn read_valid(path: &Path) -> Option<ConfigLoad> {
    let metadata = fs::metadata(path).ok()?;
    let size = usize::try_from(metadata.len()).ok()?;
    if size > crate::MAX_CONFIG_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    match decode_config(&bytes) {
        load @ (ConfigLoad::Current(_) | ConfigLoad::Migrated { .. }) => Some(load),
        ConfigLoad::Recovered { .. } => None,
    }
}

fn report_of(load: &ConfigLoad) -> Option<RecoveryReport> {
    match load {
        ConfigLoad::Current(_) => None,
        ConfigLoad::Migrated { from, .. } => {
            Some(RecoveryReport::new(RecoveryKind::Malformed, Some(*from)))
        }
        ConfigLoad::Recovered { report, .. } => Some(*report),
    }
}

fn sibling(destination: &Path, suffix: &str) -> PathBuf {
    let mut name = destination.as_os_str().to_owned();
    name.push(format!(".{suffix}"));
    PathBuf::from(name)
}

fn write_synced(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    file.write_all(contents)?;
    file.flush()?;
    file.sync_all()
}

fn sync_file(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Describes persistence validation, interruption, or filesystem failure.
#[derive(Debug, Error)]
pub enum PersistenceError {
    /// Configuration did not validate or encode.
    #[error("invalid configuration: {0}")]
    InvalidConfig(crate::ConfigError),
    /// Replace strategy was deliberately interrupted.
    #[error("atomic replacement interrupted")]
    Interrupted,
    /// Filesystem operation failed.
    #[error("configuration filesystem error: {0}")]
    Io(#[from] io::Error),
}

impl PartialEq for PersistenceError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Interrupted, Self::Interrupted) => true,
            (Self::Io(left), Self::Io(right)) => left.kind() == right.kind(),
            (Self::InvalidConfig(_), Self::InvalidConfig(_)) => true,
            _ => false,
        }
    }
}
