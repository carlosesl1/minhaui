use shell_core::RecoveryTransactionId;
use shell_watchdog::{
    MAX_RECOVERY_JOURNAL_BYTES, RECOVERY_JOURNAL_FILE_NAME, RecoveryCoordinator,
    RecoveryCoordinatorError, RecoveryJournalError, RecoveryJournalPhase, RecoveryJournalV1,
    TaskbarBounds, TaskbarSnapshot, cancel_prepared_recovery_journal,
    create_prepared_recovery_journal, load_recovery_journal, mark_recovery_journal_applied,
    recovery_journal_path, remove_recovery_journal, remove_recovery_journal_for_transaction,
    save_recovery_journal,
};
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

enum CreateAttempt {
    Created(RecoveryJournalV1),
    AlreadyExists,
    Failed(String),
}

enum TransitionAttempt {
    Applied,
    AlreadyApplied,
    Failed(String),
}

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

fn transaction_id(seed: u8) -> Result<RecoveryTransactionId, Box<dyn Error>> {
    Ok(RecoveryTransactionId::try_from_bytes([seed; 16])?)
}

fn sample_snapshots() -> Vec<TaskbarSnapshot> {
    vec![TaskbarSnapshot::new(
        r"\\.\DISPLAY1",
        "Shell_TrayWnd",
        true,
        TaskbarBounds::new(0, 1040, 1920, 1080),
    )]
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
    assert!(entries.iter().any(|entry| entry.path() == path));
    assert!(
        !entries
            .iter()
            .any(|entry| { entry.file_name().to_string_lossy().ends_with(".tmp") })
    );

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

#[test]
fn exclusive_creation_never_overwrites_existing_or_corrupt_journal() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("exclusive")?;
    let path = directory.journal_path();
    let original_transaction = transaction_id(1)?;
    let original =
        create_prepared_recovery_journal(&path, original_transaction, 1, sample_snapshots())?;

    let error = create_prepared_recovery_journal(&path, transaction_id(2)?, 0, sample_snapshots())
        .err()
        .ok_or_else(|| expected_error("existing journal was unexpectedly replaced"))?;
    assert!(matches!(error, RecoveryJournalError::AlreadyExists { .. }));
    assert_eq!(load_recovery_journal(&path)?, Some(original));

    remove_recovery_journal(&path)?;
    let corrupt = b"{ pending-but-corrupt";
    fs::write(&path, corrupt)?;
    let error = create_prepared_recovery_journal(&path, transaction_id(3)?, 1, sample_snapshots())
        .err()
        .ok_or_else(|| expected_error("corrupt journal was unexpectedly replaced"))?;
    assert!(matches!(error, RecoveryJournalError::AlreadyExists { .. }));
    assert_eq!(fs::read(&path)?, corrupt);
    Ok(())
}

#[test]
fn concurrent_creation_has_exactly_one_winner() -> Result<(), Box<dyn Error>> {
    const CONTENDERS: u8 = 8;
    let directory = TestDirectory::create("concurrent-create")?;
    let path = Arc::new(directory.journal_path());
    let barrier = Arc::new(Barrier::new(usize::from(CONTENDERS)));
    let mut handles = Vec::new();

    for seed in 1..=CONTENDERS {
        let worker_path = Arc::clone(&path);
        let worker_barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            worker_barrier.wait();
            let transaction_id = match RecoveryTransactionId::try_from_bytes([seed; 16]) {
                Ok(transaction_id) => transaction_id,
                Err(error) => return CreateAttempt::Failed(error.to_string()),
            };
            match create_prepared_recovery_journal(
                worker_path.as_ref(),
                transaction_id,
                1,
                sample_snapshots(),
            ) {
                Ok(journal) => CreateAttempt::Created(journal),
                Err(RecoveryJournalError::AlreadyExists { .. }) => CreateAttempt::AlreadyExists,
                Err(error) => CreateAttempt::Failed(error.to_string()),
            }
        }));
    }

    let mut winners = Vec::new();
    let mut already_exists = 0;
    for handle in handles {
        let outcome = handle
            .join()
            .map_err(|_| expected_error("journal creation worker panicked"))?;
        match outcome {
            CreateAttempt::Created(journal) => winners.push(journal),
            CreateAttempt::AlreadyExists => already_exists += 1,
            CreateAttempt::Failed(message) => return Err(Box::new(io::Error::other(message))),
        }
    }

    assert_eq!(winners.len(), 1);
    assert_eq!(already_exists, usize::from(CONTENDERS - 1));
    assert_eq!(load_recovery_journal(path.as_ref())?, winners.pop());
    Ok(())
}

#[test]
fn concurrent_applied_transition_is_serialized_without_lost_updates() -> Result<(), Box<dyn Error>>
{
    let directory = TestDirectory::create("concurrent-transition")?;
    let path = Arc::new(directory.journal_path());
    let transaction = transaction_id(10)?;
    create_prepared_recovery_journal(path.as_ref(), transaction, 1, sample_snapshots())?;
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();

    for _worker in 0..2 {
        let worker_path = Arc::clone(&path);
        let worker_barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            worker_barrier.wait();
            match mark_recovery_journal_applied(worker_path.as_ref(), transaction) {
                Ok(_) => TransitionAttempt::Applied,
                Err(RecoveryJournalError::UnexpectedPhase {
                    found: RecoveryJournalPhase::Applied,
                    ..
                }) => TransitionAttempt::AlreadyApplied,
                Err(error) => TransitionAttempt::Failed(error.to_string()),
            }
        }));
    }

    let mut applied = 0;
    let mut already_applied = 0;
    for handle in handles {
        match handle
            .join()
            .map_err(|_| expected_error("journal transition worker panicked"))?
        {
            TransitionAttempt::Applied => applied += 1,
            TransitionAttempt::AlreadyApplied => already_applied += 1,
            TransitionAttempt::Failed(message) => {
                return Err(Box::new(io::Error::other(message)));
            }
        }
    }

    assert_eq!(applied, 1);
    assert_eq!(already_applied, 1);
    assert_eq!(
        load_recovery_journal(path.as_ref())?.map(|journal| journal.phase),
        Some(RecoveryJournalPhase::Applied)
    );
    Ok(())
}

#[test]
fn authenticated_removal_preserves_a_newer_transaction_after_aba() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("authenticated-remove-aba")?;
    let path = directory.journal_path();
    let original_transaction = transaction_id(11)?;
    let replacement_transaction = transaction_id(12)?;
    create_prepared_recovery_journal(&path, original_transaction, 1, sample_snapshots())?;

    remove_recovery_journal_for_transaction(&path, original_transaction)?;
    let replacement =
        create_prepared_recovery_journal(&path, replacement_transaction, 1, sample_snapshots())?;

    let error = remove_recovery_journal_for_transaction(&path, original_transaction)
        .err()
        .ok_or_else(|| expected_error("stale restorer unexpectedly removed replacement journal"))?;
    assert!(matches!(
        error,
        RecoveryJournalError::TransactionMismatch {
            expected,
            found,
            ..
        } if expected == original_transaction && found == replacement_transaction
    ));
    assert_eq!(load_recovery_journal(&path)?, Some(replacement));

    remove_recovery_journal_for_transaction(&path, replacement_transaction)?;
    remove_recovery_journal_for_transaction(&path, replacement_transaction)?;
    assert_eq!(load_recovery_journal(&path)?, None);
    Ok(())
}

#[test]
fn abandoned_staging_file_is_never_promoted_or_used_as_authority() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("crashed-stage")?;
    let path = directory.journal_path();
    let staging_path = directory
        .0
        .join(format!(".{RECOVERY_JOURNAL_FILE_NAME}.crashed.tmp"));
    let abandoned = b"partial write from a crashed owner";
    fs::write(&staging_path, abandoned)?;

    let expected =
        create_prepared_recovery_journal(&path, transaction_id(4)?, 1, sample_snapshots())?;

    assert_eq!(load_recovery_journal(&path)?, Some(expected));
    assert_eq!(fs::read(staging_path)?, abandoned);
    Ok(())
}

#[test]
fn applied_transition_requires_the_expected_typed_transaction_id() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("authenticated-transition")?;
    let path = directory.journal_path();
    let expected_transaction = transaction_id(5)?;
    create_prepared_recovery_journal(&path, expected_transaction, 1, sample_snapshots())?;
    let before = fs::read(&path)?;

    let error = mark_recovery_journal_applied(&path, transaction_id(6)?)
        .err()
        .ok_or_else(|| expected_error("mismatched transaction unexpectedly transitioned"))?;
    assert!(matches!(
        error,
        RecoveryJournalError::TransactionMismatch { .. }
    ));
    assert_eq!(fs::read(&path)?, before);

    let applied = mark_recovery_journal_applied(&path, expected_transaction)?;
    assert_eq!(applied.phase, RecoveryJournalPhase::Applied);
    assert_eq!(load_recovery_journal(&path)?, Some(applied.clone()));
    let after = fs::read(&path)?;

    let error = mark_recovery_journal_applied(&path, expected_transaction)
        .err()
        .ok_or_else(|| expected_error("applied journal unexpectedly transitioned twice"))?;
    assert!(matches!(
        error,
        RecoveryJournalError::UnexpectedPhase {
            expected: RecoveryJournalPhase::Prepared,
            found: RecoveryJournalPhase::Applied,
            ..
        }
    ));
    assert_eq!(fs::read(&path)?, after);
    Ok(())
}

#[test]
fn applied_transition_preserves_invalid_and_corrupt_transactions() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("invalid-transition")?;
    let path = directory.journal_path();
    let expected_transaction = transaction_id(7)?;
    let invalid = RecoveryJournalV1::new(
        "legacy-noncanonical-id",
        RecoveryJournalPhase::Prepared,
        1,
        sample_snapshots(),
    );
    save_recovery_journal(&path, &invalid)?;
    let invalid_bytes = fs::read(&path)?;
    let error = mark_recovery_journal_applied(&path, expected_transaction)
        .err()
        .ok_or_else(|| expected_error("invalid transaction unexpectedly transitioned"))?;
    assert!(matches!(
        error,
        RecoveryJournalError::InvalidTransactionId { .. }
    ));
    assert_eq!(fs::read(&path)?, invalid_bytes);

    fs::write(&path, b"{ corrupt")?;
    let corrupt_bytes = fs::read(&path)?;
    let error = mark_recovery_journal_applied(&path, expected_transaction)
        .err()
        .ok_or_else(|| expected_error("corrupt transaction unexpectedly transitioned"))?;
    assert!(matches!(error, RecoveryJournalError::Corrupt { .. }));
    assert_eq!(fs::read(&path)?, corrupt_bytes);
    Ok(())
}

#[test]
fn pure_coordinator_tracks_authenticated_durable_phases() -> Result<(), Box<dyn Error>> {
    let transaction = transaction_id(8)?;
    let other = transaction_id(9)?;
    let idle = RecoveryCoordinator::from_journal(None)?;
    assert_eq!(idle, RecoveryCoordinator::Idle);

    let prepared = idle.prepare(transaction)?;
    assert_eq!(prepared.transaction_id(), Some(transaction));
    assert!(matches!(
        prepared.mark_applied(other),
        Err(RecoveryCoordinatorError::TransactionMismatch { .. })
    ));

    let applied = prepared.mark_applied(transaction)?;
    let restoring = applied.begin_restore(transaction)?;
    assert_eq!(
        restoring,
        RecoveryCoordinator::Restoring {
            transaction_id: transaction
        }
    );
    assert_eq!(
        restoring.finish_restore(transaction)?,
        RecoveryCoordinator::Idle
    );

    let journal = RecoveryJournalV1::new(
        transaction.to_string(),
        RecoveryJournalPhase::Applied,
        1,
        sample_snapshots(),
    );
    assert_eq!(
        RecoveryCoordinator::from_journal(Some(&journal))?,
        RecoveryCoordinator::Applied {
            transaction_id: transaction
        }
    );
    Ok(())
}

#[test]
fn prepared_cancellation_is_phase_and_transaction_authenticated() -> Result<(), Box<dyn Error>> {
    let directory = TestDirectory::create("cancel-prepared")?;
    let path = directory.journal_path();
    let expected = transaction_id(10)?;
    let other = transaction_id(11)?;
    create_prepared_recovery_journal(&path, expected, 1, sample_snapshots())?;

    assert!(matches!(
        cancel_prepared_recovery_journal(&path, other),
        Err(RecoveryJournalError::TransactionMismatch { .. })
    ));
    assert!(path.exists());
    mark_recovery_journal_applied(&path, expected)?;
    assert!(matches!(
        cancel_prepared_recovery_journal(&path, expected),
        Err(RecoveryJournalError::UnexpectedPhase {
            expected: RecoveryJournalPhase::Prepared,
            found: RecoveryJournalPhase::Applied,
            ..
        })
    ));
    assert!(path.exists());

    remove_recovery_journal(&path)?;
    create_prepared_recovery_journal(&path, expected, 1, sample_snapshots())?;
    cancel_prepared_recovery_journal(&path, expected)?;
    assert!(!path.exists());
    Ok(())
}
