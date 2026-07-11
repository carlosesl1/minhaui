use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use shell_config::{
    AtomicPaths, AtomicWriter, ConfigLoad, ConfigSource, ConfigStore, PersistenceError,
    RecoveryKind, ShellConfigV1, ThemeError, ThemePayload, decode_config, export_theme,
    import_theme,
};

struct InterruptAfterTemp;

impl AtomicWriter for InterruptAfterTemp {
    fn replace(&self, paths: &AtomicPaths, contents: &[u8]) -> Result<(), PersistenceError> {
        fs::write(paths.temporary(), contents).map_err(PersistenceError::Io)?;
        Err(PersistenceError::Interrupted)
    }
}

fn main() -> ExitCode {
    let directory = qa_directory();
    let prepared = clean(&directory)
        .and_then(|()| fs::create_dir_all(&directory))
        .is_ok();
    let results = if prepared {
        run_matrix(&directory)
    } else {
        failed_matrix()
    };
    let cleaned = clean(&directory).is_ok();
    let mut passed = cleaned;
    for (name, result) in results {
        println!("{name}|{}", if result { "PASS" } else { "FAIL" });
        passed &= result;
    }
    println!("CLEANUP|{}", if cleaned { "PASS" } else { "FAIL" });
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run_matrix(directory: &Path) -> [(&'static str, bool); 7] {
    let missing_store = ConfigStore::new(directory.join("missing.json"));
    let default_pass = missing_store.load().source() == ConfigSource::Defaults;

    let store = ConfigStore::new(directory.join("settings.json"));
    let original = ShellConfigV1::default();
    let valid_pass = store.persist(&original).is_ok() && store.load().config() == &original;

    let migrated_pass = matches!(
        decode_config(include_bytes!("../tests/fixtures/v0.json")),
        ConfigLoad::Migrated { from: 0, .. }
    );

    let preserved_before = fs::read(store.path()).ok();
    let interrupted = store.persist_with(&original.with_autohide(true), &InterruptAfterTemp);
    let preservation_pass = interrupted == Err(PersistenceError::Interrupted)
        && fs::read(store.path()).ok() == preserved_before;

    let second_write = store.persist(&original.with_autohide(true)).is_ok();
    let corrupt_written = fs::write(store.path(), b"truncated {").is_ok();
    let recovered = store.load();
    let corrupt_pass = second_write
        && corrupt_written
        && recovered.source() == ConfigSource::Backup
        && recovered.config() == &original
        && store.backup_path().exists();

    let future_pass = matches!(
        decode_config(br#"{"schema_version":99}"#),
        ConfigLoad::Recovered { report, .. }
            if report.kind() == RecoveryKind::FutureVersion
                && report.detected_version() == Some(99)
    );

    let tampered_theme_pass = export_theme(&ThemePayload::default())
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|text| text.replace("#11151BEF", "#21151BEF"))
        .map(|text| import_theme(text.as_bytes()))
        == Some(Err(ThemeError::ChecksumMismatch));

    [
        ("DEFAULT", default_pass),
        ("VALID", valid_pass),
        ("MIGRATED", migrated_pass),
        ("PRESERVATION", preservation_pass),
        ("CORRUPT", corrupt_pass),
        ("FUTURE", future_pass),
        ("TAMPERED_THEME", tampered_theme_pass),
    ]
}

const fn failed_matrix() -> [(&'static str, bool); 7] {
    [
        ("DEFAULT", false),
        ("VALID", false),
        ("MIGRATED", false),
        ("PRESERVATION", false),
        ("CORRUPT", false),
        ("FUTURE", false),
        ("TAMPERED_THEME", false),
    ]
}

fn qa_directory() -> PathBuf {
    std::env::temp_dir().join(format!("shell-config-matrix-{}", std::process::id()))
}

fn clean(path: &Path) -> std::io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
