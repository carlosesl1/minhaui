use shell_watchdog::{
    MAX_RECOVERY_JOURNAL_BYTES, RECOVERY_JOURNAL_FILE_NAME, RecoveryJournalError,
    RecoveryJournalPhase, RecoveryJournalV1, TaskbarBounds, TaskbarSnapshot, load_recovery_journal,
    recovery_journal_path, remove_recovery_journal, save_recovery_journal,
};
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(name: &str) -> io::Result<Self> {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "minha-ui-watchdog-journal-{name}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn journal_path(&self) -> PathBuf {
        recovery_journal_path(&self.0)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ignored = fs::remove_dir_all(&self.0);
    }
}

fn sample_journal(phase: RecoveryJournalPhase) -> RecoveryJournalV1 {
    RecoveryJournalV1::new(
        "transaction-42",
        phase,
        3,
        vec![TaskbarSnapshot::new(
            r"\\.\DISPLAY1",
            "Shell_TrayWnd",
            true,
            TaskbarBounds::new(0, 1040, 1920, 1080),
        )],
    )
}

fn expected_error(message: &'static str) -> io::Error {
    io::Error::other(message)
}

#[test]
fn journal_round_trips_and_can_be_atomically_replaced() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("roundtrip")?;
    let path = directory.journal_path();
    assert_eq!(
        path.file_name(),
        Some(OsStr::new(RECOVERY_JOURNAL_FILE_NAME))
    );

    let mut expected = sample_journal(RecoveryJournalPhase::Prepared);
    save_recovery_journal(&path, &expected)?;
    assert_eq!(load_recovery_journal(&path)?, Some(expected.clone()));

    expected.set_phase(RecoveryJournalPhase::Applied);
    save_recovery_journal(&path, &expected)?;
    assert_eq!(load_recovery_journal(&path)?, Some(expected));
    let entries = fs::read_dir(&directory.0)?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path(), path);

    remove_recovery_journal(&path)?;
    remove_recovery_journal(&path)?;
    assert_eq!(load_recovery_journal(&path)?, None);
    Ok(())
}

#[test]
fn corrupt_json_has_an_explicit_error() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("corrupt")?;
    let path = directory.journal_path();
    fs::write(&path, b"{ definitely-not-json")?;

    let error = load_recovery_journal(&path)
        .err()
        .ok_or_else(|| expected_error("corrupt journal unexpectedly loaded"))?;
    assert!(matches!(error, RecoveryJournalError::Corrupt { .. }));
    Ok(())
}

#[test]
fn future_schema_is_not_misread_as_v1() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("future")?;
    let path = directory.journal_path();
    fs::write(&path, br#"{"schema": 2}"#)?;

    let error = load_recovery_journal(&path)
        .err()
        .ok_or_else(|| expected_error("future journal unexpectedly loaded"))?;
    assert!(matches!(
        error,
        RecoveryJournalError::FutureSchema {
            found: 2,
            supported: 1,
            ..
        }
    ));
    Ok(())
}

#[test]
fn oversized_journal_is_rejected_before_json_decode() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("oversize")?;
    let path = directory.journal_path();
    fs::write(&path, vec![b' '; MAX_RECOVERY_JOURNAL_BYTES + 1])?;

    let error = load_recovery_journal(&path)
        .err()
        .ok_or_else(|| expected_error("oversized journal unexpectedly loaded"))?;
    assert!(matches!(
        error,
        RecoveryJournalError::Oversized {
            actual_bytes,
            max_bytes: MAX_RECOVERY_JOURNAL_BYTES,
            ..
        } if actual_bytes == (MAX_RECOVERY_JOURNAL_BYTES + 1) as u64
    ));
    Ok(())
}
