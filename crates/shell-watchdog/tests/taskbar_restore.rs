#[cfg(not(windows))]
use shell_watchdog::save_recovery_journal;
use shell_watchdog::{
    RecoveryJournalError, RecoveryJournalPhase, RecoveryJournalV1, TaskbarBounds,
    TaskbarObservation, TaskbarObservationId, TaskbarRestoreError, TaskbarRestoreOutcome,
    TaskbarRestorePlanError, TaskbarSnapshot, plan_taskbar_restore, recovery_journal_path,
    restore_recovery_journal,
};
use std::error::Error;
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
            "minha-ui-restore-contract-{name}-{}-{sequence}",
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

fn journal() -> RecoveryJournalV1 {
    RecoveryJournalV1::new(
        "restore-contract-1",
        RecoveryJournalPhase::Applied,
        1,
        vec![TaskbarSnapshot::new(
            r"\\.\DISPLAY1",
            "Shell_TrayWnd",
            true,
            TaskbarBounds::new(0, 1040, 1920, 1080),
        )],
    )
}

fn observation() -> TaskbarObservation {
    TaskbarObservation::new(
        TaskbarObservationId::new(7),
        r"\\.\DISPLAY1",
        "Shell_TrayWnd",
        false,
        TaskbarBounds::new(0, 1040, 1920, 1080),
    )
}

#[test]
fn missing_journal_is_an_idempotent_no_op() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("missing")?;
    let path = directory.journal_path();

    assert_eq!(
        restore_recovery_journal(&path, false)?,
        TaskbarRestoreOutcome::NoJournal
    );
    assert_eq!(
        restore_recovery_journal(&path, true)?,
        TaskbarRestoreOutcome::NoJournal
    );
    Ok(())
}

#[test]
fn corrupt_and_future_journals_fail_before_native_recovery() -> Result<(), Box<dyn Error>> {
    let corrupt_directory = TestDirectory::create("corrupt")?;
    let corrupt_path = corrupt_directory.journal_path();
    fs::write(&corrupt_path, b"{not-json")?;
    assert!(matches!(
        restore_recovery_journal(&corrupt_path, false),
        Err(TaskbarRestoreError::Journal(
            RecoveryJournalError::Corrupt { .. }
        ))
    ));
    assert!(corrupt_path.exists());

    let future_directory = TestDirectory::create("future")?;
    let future_path = future_directory.journal_path();
    fs::write(&future_path, br#"{"schema":2}"#)?;
    assert!(matches!(
        restore_recovery_journal(&future_path, false),
        Err(TaskbarRestoreError::Journal(
            RecoveryJournalError::FutureSchema { found: 2, .. }
        ))
    ));
    assert!(future_path.exists());
    Ok(())
}

#[test]
fn planner_rejects_topology_mismatch_instead_of_guessing() {
    let mut mismatched = observation();
    mismatched.device = r"\\.\DISPLAY2".to_owned();

    assert_eq!(
        plan_taskbar_restore(&journal(), &[mismatched]),
        Err(TaskbarRestorePlanError::SnapshotMismatch { snapshot: 0 })
    );
}

#[cfg(not(windows))]
#[test]
fn non_windows_recovery_preserves_a_valid_pending_journal() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("unsupported")?;
    let path = directory.journal_path();
    save_recovery_journal(&path, &journal())?;

    assert!(matches!(
        restore_recovery_journal(&path, false),
        Err(TaskbarRestoreError::UnsupportedPlatform)
    ));
    assert!(path.exists());
    Ok(())
}
