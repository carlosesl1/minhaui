use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use shell_config::{AtomicPaths, AtomicWriter, ConfigStore, PersistenceError, ShellConfigV1};

struct InterruptingWriter;

impl AtomicWriter for InterruptingWriter {
    fn replace(&self, paths: &AtomicPaths, contents: &[u8]) -> Result<(), PersistenceError> {
        fs::write(paths.temporary(), contents).map_err(PersistenceError::Io)?;
        Err(PersistenceError::Interrupted)
    }
}

fn qa_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("shell-config-{name}-{}", std::process::id()))
}

fn clean(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[test]
fn interrupted_atomic_write_preserves_previous_valid_config()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a store containing a valid config.
    let dir = qa_dir("interrupt");
    clean(&dir)?;
    fs::create_dir_all(&dir)?;
    let store = ConfigStore::new(dir.join("settings.json"));
    let original = ShellConfigV1::default();
    store.persist(&original)?;
    let original_bytes = fs::read(store.path())?;

    // When: replacement is interrupted after the same-directory temp write.
    let result = store.persist_with(&original.with_autohide(true), &InterruptingWriter);

    // Then: the original file is byte-for-byte preserved and still loads.
    assert_eq!(result, Err(PersistenceError::Interrupted));
    let after = fs::read(store.path())?;
    assert_eq!(after, original_bytes);
    assert_eq!(store.load().config(), &original);
    clean(&dir)?;
    Ok(())
}

#[test]
fn corrupt_primary_recovers_from_preserved_backup() -> Result<(), Box<dyn std::error::Error>> {
    // Given: two valid writes, followed by corruption of the primary file.
    let dir = qa_dir("backup");
    clean(&dir)?;
    fs::create_dir_all(&dir)?;
    let store = ConfigStore::new(dir.join("settings.json"));
    let original = ShellConfigV1::default();
    store.persist(&original)?;
    store.persist(&original.with_autohide(true))?;
    fs::write(store.path(), b"truncated {")?;

    // When: the store loads.
    let loaded = store.load();

    // Then: the previous valid backup is recovered without being destroyed.
    assert_eq!(loaded.config(), &original);
    assert!(store.backup_path().exists());
    clean(&dir)?;
    Ok(())
}
