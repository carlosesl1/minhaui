#![deny(unsafe_code)]

use std::path::Path;

use thiserror::Error;

#[cfg(any(windows, test))]
use crate::remove_recovery_journal_for_transaction;
use crate::{
    RecoveryJournalError, RecoveryJournalV1, TaskbarBounds, TaskbarSnapshot, load_recovery_journal,
};

const PRIMARY_TASKBAR_CLASS: &str = "Shell_TrayWnd";
const SECONDARY_TASKBAR_CLASS: &str = "Shell_SecondaryTrayWnd";
// On Windows 7 and later ABM_GETSTATE no longer reports ABS_ALWAYSONTOP,
// so V1 accepts only the ABS_AUTOHIDE bit that can be read back and proven.
const SUPPORTED_APPBAR_STATE_BITS: u32 = 0x1;
const MAX_TASKBAR_SNAPSHOTS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TaskbarObservationId(u64);

impl TaskbarObservationId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskbarObservation {
    pub id: TaskbarObservationId,
    pub device: String,
    pub class_name: String,
    pub visible: bool,
    pub bounds: TaskbarBounds,
}

impl TaskbarObservation {
    #[must_use]
    pub fn new(
        id: TaskbarObservationId,
        device: impl Into<String>,
        class_name: impl Into<String>,
        visible: bool,
        bounds: TaskbarBounds,
    ) -> Self {
        Self {
            id,
            device: device.into(),
            class_name: class_name.into(),
            visible,
            bounds,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskbarRestoreTarget {
    pub id: TaskbarObservationId,
    pub snapshot: TaskbarSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskbarRestorePlan {
    appbar_state: u32,
    targets: Vec<TaskbarRestoreTarget>,
}

impl TaskbarRestorePlan {
    #[must_use]
    pub const fn appbar_state(&self) -> u32 {
        self.appbar_state
    }

    #[must_use]
    pub fn targets(&self) -> &[TaskbarRestoreTarget] {
        &self.targets
    }

    #[must_use]
    pub fn primary_target(&self) -> Option<&TaskbarRestoreTarget> {
        self.targets
            .iter()
            .find(|target| target.snapshot.class_name == PRIMARY_TASKBAR_CLASS)
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TaskbarRestorePlanError {
    #[error("recovery transaction id is empty")]
    EmptyTransactionId,
    #[error("recovery journal contains no taskbar snapshots")]
    EmptySnapshots,
    #[error(
        "recovery journal contains {actual} taskbar snapshots, exceeding the {maximum}-snapshot limit"
    )]
    TooManySnapshots { actual: usize, maximum: usize },
    #[error("recovery journal contains unsupported appbar state 0x{state:08x}")]
    UnsupportedAppbarState { state: u32 },
    #[error("taskbar snapshot {index} uses unsupported class `{class_name}`")]
    UnsupportedSnapshotClass { index: usize, class_name: String },
    #[error("taskbar snapshot {index} has an empty monitor device")]
    EmptySnapshotDevice { index: usize },
    #[error("taskbar snapshot {index} has invalid bounds")]
    InvalidSnapshotBounds { index: usize },
    #[error("taskbar snapshots {first} and {second} describe the same class and monitor")]
    DuplicateSnapshot { first: usize, second: usize },
    #[error("recovery journal must contain exactly one primary taskbar snapshot; found {actual}")]
    InvalidPrimarySnapshotCount { actual: usize },
    #[error(
        "discovered Explorer taskbar count {actual} does not match recovery snapshot count {expected}"
    )]
    ObservationCountMismatch { expected: usize, actual: usize },
    #[error("discovered taskbar {index} uses unsupported class `{class_name}`")]
    UnsupportedObservationClass { index: usize, class_name: String },
    #[error("discovered taskbar {index} has an empty monitor device")]
    EmptyObservationDevice { index: usize },
    #[error("discovered taskbar {index} has invalid bounds")]
    InvalidObservationBounds { index: usize },
    #[error("discovered taskbars {first} and {second} have the same native identity")]
    DuplicateObservationId { first: usize, second: usize },
    #[error("discovered taskbars {first} and {second} describe the same class and monitor")]
    DuplicateObservation { first: usize, second: usize },
    #[error("taskbar snapshot {snapshot} has no unique Explorer-owned class/device/bounds match")]
    SnapshotMismatch { snapshot: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskbarRestoreOutcome {
    NoJournal,
    Checked,
    Restored,
}

#[derive(Debug, Error)]
pub enum TaskbarRestoreError {
    #[error(transparent)]
    Journal(#[from] RecoveryJournalError),
    #[error(transparent)]
    Plan(#[from] TaskbarRestorePlanError),
    #[error("native taskbar recovery is unavailable on this platform; the journal was preserved")]
    UnsupportedPlatform,
    #[error(
        "native taskbar recovery failed while {operation}: {detail}; the journal was preserved"
    )]
    Native {
        operation: &'static str,
        detail: String,
    },
    #[error("native taskbar recovery could not be verified: {detail}; the journal was preserved")]
    Verification { detail: String },
}

impl TaskbarRestoreError {
    #[cfg(windows)]
    pub(crate) fn native(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::Native {
            operation,
            detail: detail.into(),
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) fn verification(detail: impl Into<String>) -> Self {
        Self::Verification {
            detail: detail.into(),
        }
    }
}

#[must_use = "a recovery plan must be applied or its error handled"]
pub fn plan_taskbar_restore(
    journal: &RecoveryJournalV1,
    observations: &[TaskbarObservation],
) -> Result<TaskbarRestorePlan, TaskbarRestorePlanError> {
    validate_journal(journal)?;
    validate_observations(observations)?;
    if observations.len() != journal.snapshots.len() {
        return Err(TaskbarRestorePlanError::ObservationCountMismatch {
            expected: journal.snapshots.len(),
            actual: observations.len(),
        });
    }

    let mut targets = Vec::with_capacity(journal.snapshots.len());
    for (snapshot_index, snapshot) in journal.snapshots.iter().enumerate() {
        let mut matches = observations.iter().filter(|observation| {
            identity_matches(snapshot, observation) && snapshot.bounds == observation.bounds
        });
        let Some(observation) = matches.next() else {
            return Err(TaskbarRestorePlanError::SnapshotMismatch {
                snapshot: snapshot_index,
            });
        };
        if matches.next().is_some() {
            return Err(TaskbarRestorePlanError::SnapshotMismatch {
                snapshot: snapshot_index,
            });
        }
        targets.push(TaskbarRestoreTarget {
            id: observation.id,
            snapshot: snapshot.clone(),
        });
    }

    Ok(TaskbarRestorePlan {
        appbar_state: journal.appbar_state,
        targets,
    })
}

pub fn restore_recovery_journal(
    path: &Path,
    check_only: bool,
) -> Result<TaskbarRestoreOutcome, TaskbarRestoreError> {
    let Some(journal) = load_recovery_journal(path)? else {
        return Ok(TaskbarRestoreOutcome::NoJournal);
    };

    #[cfg(windows)]
    {
        restore_with_backend(
            path,
            check_only,
            &journal,
            &crate::taskbar_restore_win32::Win32TaskbarRestoreBackend,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (check_only, journal);
        Err(TaskbarRestoreError::UnsupportedPlatform)
    }
}

#[cfg(any(windows, test))]
pub(crate) trait TaskbarRestoreBackend {
    fn observe(&self) -> Result<Vec<TaskbarObservation>, TaskbarRestoreError>;

    fn preflight(
        &self,
        journal: &RecoveryJournalV1,
        plan: &TaskbarRestorePlan,
    ) -> Result<(), TaskbarRestoreError>;

    fn apply_and_verify(
        &self,
        journal: &RecoveryJournalV1,
        plan: &TaskbarRestorePlan,
    ) -> Result<(), TaskbarRestoreError>;
}

#[cfg(any(windows, test))]
fn restore_with_backend<B: TaskbarRestoreBackend>(
    path: &Path,
    check_only: bool,
    journal: &RecoveryJournalV1,
    backend: &B,
) -> Result<TaskbarRestoreOutcome, TaskbarRestoreError> {
    let transaction_id = journal.transaction_id.parse().map_err(|source| {
        RecoveryJournalError::InvalidTransactionId {
            path: path.to_path_buf(),
            source,
        }
    })?;
    let observations = backend.observe()?;
    let plan = plan_taskbar_restore(journal, &observations)?;
    backend.preflight(journal, &plan)?;
    if check_only {
        return Ok(TaskbarRestoreOutcome::Checked);
    }
    backend.apply_and_verify(journal, &plan)?;
    remove_recovery_journal_for_transaction(path, transaction_id)?;
    Ok(TaskbarRestoreOutcome::Restored)
}

fn validate_journal(journal: &RecoveryJournalV1) -> Result<(), TaskbarRestorePlanError> {
    if journal.transaction_id.trim().is_empty() {
        return Err(TaskbarRestorePlanError::EmptyTransactionId);
    }
    if journal.snapshots.is_empty() {
        return Err(TaskbarRestorePlanError::EmptySnapshots);
    }
    if journal.snapshots.len() > MAX_TASKBAR_SNAPSHOTS {
        return Err(TaskbarRestorePlanError::TooManySnapshots {
            actual: journal.snapshots.len(),
            maximum: MAX_TASKBAR_SNAPSHOTS,
        });
    }
    if journal.appbar_state & !SUPPORTED_APPBAR_STATE_BITS != 0 {
        return Err(TaskbarRestorePlanError::UnsupportedAppbarState {
            state: journal.appbar_state,
        });
    }

    let mut primary_count = 0;
    for (index, snapshot) in journal.snapshots.iter().enumerate() {
        validate_snapshot(index, snapshot)?;
        primary_count += usize::from(snapshot.class_name == PRIMARY_TASKBAR_CLASS);
        for (previous_index, previous) in journal.snapshots[..index].iter().enumerate() {
            if identity_matches_snapshots(previous, snapshot) {
                return Err(TaskbarRestorePlanError::DuplicateSnapshot {
                    first: previous_index,
                    second: index,
                });
            }
        }
    }
    if primary_count != 1 {
        return Err(TaskbarRestorePlanError::InvalidPrimarySnapshotCount {
            actual: primary_count,
        });
    }
    Ok(())
}

fn validate_observations(
    observations: &[TaskbarObservation],
) -> Result<(), TaskbarRestorePlanError> {
    for (index, observation) in observations.iter().enumerate() {
        if !supported_class(&observation.class_name) {
            return Err(TaskbarRestorePlanError::UnsupportedObservationClass {
                index,
                class_name: observation.class_name.clone(),
            });
        }
        if observation.device.trim().is_empty() {
            return Err(TaskbarRestorePlanError::EmptyObservationDevice { index });
        }
        if !valid_bounds(observation.bounds) {
            return Err(TaskbarRestorePlanError::InvalidObservationBounds { index });
        }
        for (previous_index, previous) in observations[..index].iter().enumerate() {
            if previous.id == observation.id {
                return Err(TaskbarRestorePlanError::DuplicateObservationId {
                    first: previous_index,
                    second: index,
                });
            }
            if observations_share_identity(previous, observation) {
                return Err(TaskbarRestorePlanError::DuplicateObservation {
                    first: previous_index,
                    second: index,
                });
            }
        }
    }
    Ok(())
}

fn validate_snapshot(
    index: usize,
    snapshot: &TaskbarSnapshot,
) -> Result<(), TaskbarRestorePlanError> {
    if !supported_class(&snapshot.class_name) {
        return Err(TaskbarRestorePlanError::UnsupportedSnapshotClass {
            index,
            class_name: snapshot.class_name.clone(),
        });
    }
    if snapshot.device.trim().is_empty() {
        return Err(TaskbarRestorePlanError::EmptySnapshotDevice { index });
    }
    if !valid_bounds(snapshot.bounds) {
        return Err(TaskbarRestorePlanError::InvalidSnapshotBounds { index });
    }
    Ok(())
}

fn supported_class(class_name: &str) -> bool {
    matches!(class_name, PRIMARY_TASKBAR_CLASS | SECONDARY_TASKBAR_CLASS)
}

const fn valid_bounds(bounds: TaskbarBounds) -> bool {
    bounds.right > bounds.left && bounds.bottom > bounds.top
}

fn identity_matches(snapshot: &TaskbarSnapshot, observation: &TaskbarObservation) -> bool {
    snapshot.class_name == observation.class_name
        && snapshot.device.eq_ignore_ascii_case(&observation.device)
}

fn identity_matches_snapshots(left: &TaskbarSnapshot, right: &TaskbarSnapshot) -> bool {
    left.class_name == right.class_name && left.device.eq_ignore_ascii_case(&right.device)
}

fn observations_share_identity(left: &TaskbarObservation, right: &TaskbarObservation) -> bool {
    left.class_name == right.class_name && left.device.eq_ignore_ascii_case(&right.device)
}

#[cfg(test)]
mod tests {
    use super::{
        TaskbarObservation, TaskbarObservationId, TaskbarRestoreBackend, TaskbarRestoreError,
        TaskbarRestoreOutcome, TaskbarRestorePlan, TaskbarRestorePlanError, plan_taskbar_restore,
        restore_with_backend,
    };
    use crate::{
        RecoveryJournalError, RecoveryJournalPhase, RecoveryJournalV1, TaskbarBounds,
        TaskbarSnapshot, create_prepared_recovery_journal, load_recovery_journal,
        recovery_journal_path, remove_recovery_journal_for_transaction, save_recovery_journal,
    };
    use shell_core::RecoveryTransactionId;
    use std::cell::Cell;
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
                "minha-ui-taskbar-restore-{name}-{}-{sequence}",
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

    struct FakeBackend {
        observations: Vec<TaskbarObservation>,
        apply_count: Cell<u32>,
        fail_apply: bool,
    }

    struct AbaReplacingBackend {
        path: PathBuf,
        original_transaction: RecoveryTransactionId,
        replacement_transaction: RecoveryTransactionId,
    }

    impl FakeBackend {
        fn healthy() -> Self {
            Self {
                observations: sample_observations(),
                apply_count: Cell::new(0),
                fail_apply: false,
            }
        }
    }

    impl TaskbarRestoreBackend for FakeBackend {
        fn observe(&self) -> Result<Vec<TaskbarObservation>, TaskbarRestoreError> {
            Ok(self.observations.clone())
        }

        fn apply_and_verify(
            &self,
            _journal: &RecoveryJournalV1,
            _plan: &TaskbarRestorePlan,
        ) -> Result<(), TaskbarRestoreError> {
            self.apply_count.set(self.apply_count.get() + 1);
            if self.fail_apply {
                Err(TaskbarRestoreError::verification("fixture mismatch"))
            } else {
                Ok(())
            }
        }

        fn preflight(
            &self,
            _journal: &RecoveryJournalV1,
            _plan: &TaskbarRestorePlan,
        ) -> Result<(), TaskbarRestoreError> {
            Ok(())
        }
    }

    impl TaskbarRestoreBackend for AbaReplacingBackend {
        fn observe(&self) -> Result<Vec<TaskbarObservation>, TaskbarRestoreError> {
            Ok(sample_observations())
        }

        fn preflight(
            &self,
            _journal: &RecoveryJournalV1,
            _plan: &TaskbarRestorePlan,
        ) -> Result<(), TaskbarRestoreError> {
            Ok(())
        }

        fn apply_and_verify(
            &self,
            journal: &RecoveryJournalV1,
            _plan: &TaskbarRestorePlan,
        ) -> Result<(), TaskbarRestoreError> {
            remove_recovery_journal_for_transaction(&self.path, self.original_transaction)?;
            create_prepared_recovery_journal(
                &self.path,
                self.replacement_transaction,
                journal.appbar_state,
                journal.snapshots.clone(),
            )?;
            Ok(())
        }
    }

    fn transaction_id(seed: u8) -> RecoveryTransactionId {
        RecoveryTransactionId::try_from_bytes([seed; 16])
            .expect("nonzero test transaction id should be valid")
    }

    fn sample_journal() -> RecoveryJournalV1 {
        RecoveryJournalV1::new(
            transaction_id(1).to_string(),
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

    fn sample_observations() -> Vec<TaskbarObservation> {
        vec![TaskbarObservation::new(
            TaskbarObservationId::new(101),
            r"\\.\display1",
            "Shell_TrayWnd",
            false,
            TaskbarBounds::new(0, 1040, 1920, 1080),
        )]
    }

    #[test]
    fn planner_requires_exact_class_device_and_bounds() {
        let journal = sample_journal();
        let mut observations = sample_observations();
        observations[0].bounds.right = 1919;

        assert_eq!(
            plan_taskbar_restore(&journal, &observations),
            Err(TaskbarRestorePlanError::SnapshotMismatch { snapshot: 0 })
        );
    }

    #[test]
    fn planner_rejects_empty_and_ambiguous_snapshots() {
        let empty = RecoveryJournalV1::new(
            "transaction-empty",
            RecoveryJournalPhase::Prepared,
            0,
            Vec::new(),
        );
        assert_eq!(
            plan_taskbar_restore(&empty, &[]),
            Err(TaskbarRestorePlanError::EmptySnapshots)
        );

        let mut duplicate = sample_journal();
        duplicate.snapshots.push(duplicate.snapshots[0].clone());
        assert_eq!(
            plan_taskbar_restore(&duplicate, &sample_observations()),
            Err(TaskbarRestorePlanError::DuplicateSnapshot {
                first: 0,
                second: 1,
            })
        );
    }

    #[test]
    fn checked_recovery_preserves_the_journal_without_applying() -> Result<(), Box<dyn Error>> {
        let directory = TestDirectory::create("checked")?;
        let path = directory.journal_path();
        let journal = sample_journal();
        save_recovery_journal(&path, &journal)?;
        let backend = FakeBackend::healthy();

        assert_eq!(
            restore_with_backend(&path, true, &journal, &backend)?,
            TaskbarRestoreOutcome::Checked
        );
        assert_eq!(backend.apply_count.get(), 0);
        assert_eq!(load_recovery_journal(&path)?, Some(journal));
        Ok(())
    }

    #[test]
    fn failed_or_mismatched_recovery_preserves_the_journal() -> Result<(), Box<dyn Error>> {
        let directory = TestDirectory::create("preserve")?;
        let path = directory.journal_path();
        let journal = sample_journal();
        save_recovery_journal(&path, &journal)?;
        let mismatched = FakeBackend {
            observations: Vec::new(),
            apply_count: Cell::new(0),
            fail_apply: false,
        };

        assert!(restore_with_backend(&path, false, &journal, &mismatched).is_err());
        assert_eq!(load_recovery_journal(&path)?, Some(journal.clone()));

        let failed = FakeBackend {
            observations: sample_observations(),
            apply_count: Cell::new(0),
            fail_apply: true,
        };
        assert!(restore_with_backend(&path, false, &journal, &failed).is_err());
        assert_eq!(load_recovery_journal(&path)?, Some(journal));
        Ok(())
    }

    #[test]
    fn successful_recovery_removes_once_and_is_idempotent() -> Result<(), Box<dyn Error>> {
        let directory = TestDirectory::create("idempotent")?;
        let path = directory.journal_path();
        let journal = sample_journal();
        save_recovery_journal(&path, &journal)?;
        let backend = FakeBackend::healthy();

        assert_eq!(
            restore_with_backend(&path, false, &journal, &backend)?,
            TaskbarRestoreOutcome::Restored
        );
        assert_eq!(backend.apply_count.get(), 1);
        assert_eq!(load_recovery_journal(&path)?, None);
        assert_eq!(
            super::restore_recovery_journal(&path, false)?,
            TaskbarRestoreOutcome::NoJournal
        );
        assert_eq!(backend.apply_count.get(), 1);
        Ok(())
    }

    #[test]
    fn stale_restore_cannot_delete_a_replacement_journal() -> Result<(), Box<dyn Error>> {
        let directory = TestDirectory::create("restore-aba")?;
        let path = directory.journal_path();
        let original_transaction = transaction_id(2);
        let replacement_transaction = transaction_id(3);
        let journal = create_prepared_recovery_journal(
            &path,
            original_transaction,
            1,
            sample_journal().snapshots,
        )?;
        let backend = AbaReplacingBackend {
            path: path.clone(),
            original_transaction,
            replacement_transaction,
        };

        assert!(matches!(
            restore_with_backend(&path, false, &journal, &backend),
            Err(TaskbarRestoreError::Journal(
                RecoveryJournalError::TransactionMismatch {
                    expected,
                    found,
                    ..
                }
            )) if expected == original_transaction && found == replacement_transaction
        ));
        assert_eq!(
            load_recovery_journal(&path)?.map(|journal| journal.transaction_id),
            Some(replacement_transaction.to_string())
        );
        Ok(())
    }
}
