use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use shell_config::{
    AtomicPaths, AtomicWriter, ConfigSource, ConfigStore, PersistenceError, ShellConfigV1,
    encode_config,
};

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

fn staging_path(destination: &Path, suffix: &str) -> PathBuf {
    let mut name = destination.as_os_str().to_owned();
    name.push(format!(".{suffix}"));
    PathBuf::from(name)
}

fn write_config(path: &Path, config: &ShellConfigV1) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, encode_config(config)?)?;
    Ok(())
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

#[test]
fn missing_primary_restores_previous_before_next_persist() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a crash after destination was moved to rollback, with staged replacement bytes.
    let dir = qa_dir("rollback-window");
    clean(&dir)?;
    fs::create_dir_all(&dir)?;
    let store = ConfigStore::new(dir.join("settings.json"));
    let previous = ShellConfigV1::default();
    let replacement = previous.with_autohide(true);
    write_config(&staging_path(store.path(), "previous"), &previous)?;
    write_config(&staging_path(store.path(), "tmp"), &replacement)?;

    // When: the next persist starts.
    store.persist(&replacement)?;

    // Then: rollback became the preserved backup and staging files were cleaned.
    assert_eq!(store.load().config(), &replacement);
    fs::write(store.path(), b"corrupt")?;
    let recovered = store.load();
    assert_eq!(recovered.source(), ConfigSource::Backup);
    assert_eq!(recovered.config(), &previous);
    assert!(!staging_path(store.path(), "tmp").exists());
    assert!(!staging_path(store.path(), "bak.tmp").exists());
    assert!(!staging_path(store.path(), "previous").exists());
    clean(&dir)?;
    Ok(())
}

#[test]
fn missing_primary_reconciles_completed_backup_candidate() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: backup copy completed, but the process stopped before publishing it.
    let dir = qa_dir("backup-temp-window");
    clean(&dir)?;
    fs::create_dir_all(&dir)?;
    let store = ConfigStore::new(dir.join("settings.json"));
    let previous = ShellConfigV1::default();
    let replacement = previous.with_autohide(true);
    write_config(&staging_path(store.path(), "bak.tmp"), &previous)?;
    write_config(&staging_path(store.path(), "tmp"), &replacement)?;

    // When: the next persist starts with no primary or published backup.
    store.persist(&replacement)?;

    // Then: the completed backup remains a recovery source after replacement.
    assert_eq!(store.load().config(), &replacement);
    fs::write(store.path(), b"corrupt")?;
    let recovered = store.load();
    assert_eq!(recovered.source(), ConfigSource::Backup);
    assert_eq!(recovered.config(), &previous);
    assert!(!staging_path(store.path(), "bak.tmp").exists());
    assert!(!staging_path(store.path(), "tmp").exists());
    clean(&dir)?;
    Ok(())
}

#[test]
fn existing_primary_wins_over_stale_rollback_and_cleans_staging()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a valid primary plus obsolete rollback and backup-temp candidates.
    let dir = qa_dir("stale-with-primary");
    clean(&dir)?;
    fs::create_dir_all(&dir)?;
    let store = ConfigStore::new(dir.join("settings.json"));
    let primary = ShellConfigV1::default().with_autohide(true);
    let stale = ShellConfigV1::default();
    let replacement = ShellConfigV1::default().with_autohide(false);
    write_config(store.path(), &primary)?;
    write_config(&staging_path(store.path(), "previous"), &stale)?;
    write_config(&staging_path(store.path(), "bak.tmp"), &stale)?;
    write_config(&staging_path(store.path(), "tmp"), &stale)?;

    // When: a new valid config is persisted.
    store.persist(&replacement)?;

    // Then: the actual primary, not stale rollback, is the preserved backup.
    fs::write(store.path(), b"corrupt")?;
    assert_eq!(store.load().config(), &primary);
    assert!(!staging_path(store.path(), "previous").exists());
    assert!(!staging_path(store.path(), "bak.tmp").exists());
    assert!(!staging_path(store.path(), "tmp").exists());
    clean(&dir)?;
    Ok(())
}

#[test]
fn staged_temp_is_promoted_when_it_is_the_only_valid_candidate()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a crash left only a fully written and synced temp config.
    let dir = qa_dir("temp-only-window");
    clean(&dir)?;
    fs::create_dir_all(&dir)?;
    let store = ConfigStore::new(dir.join("settings.json"));
    let staged = ShellConfigV1::default().with_autohide(true);
    let replacement = ShellConfigV1::default();
    write_config(&staging_path(store.path(), "tmp"), &staged)?;

    // When: the next persist starts.
    store.persist(&replacement)?;

    // Then: the staged valid config was preserved as backup before replacement.
    fs::write(store.path(), b"corrupt")?;
    assert_eq!(store.load().config(), &staged);
    assert!(!staging_path(store.path(), "tmp").exists());
    clean(&dir)?;
    Ok(())
}
