#![deny(unsafe_code)]

mod diagnostics;
mod instance;
mod journal;
mod process_supervisor;
mod recovery;
mod recovery_coordinator;
mod supervisor;
mod taskbar_restore;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 Explorer taskbar recovery FFI is isolated by ADR-0011"
)]
mod taskbar_restore_win32;

pub use diagnostics::{
    DiagnosticEvent, DiagnosticField, export_diagnostics, retained_log_segments,
};
pub use instance::{InstanceDecision, InstanceProbe, classify_instance};
pub use journal::{
    MAX_RECOVERY_JOURNAL_BYTES, RECOVERY_JOURNAL_FILE_NAME, RECOVERY_JOURNAL_SCHEMA_V1,
    RecoveryJournalError, RecoveryJournalPhase, RecoveryJournalV1, TaskbarBounds, TaskbarSnapshot,
    create_prepared_recovery_journal, load_recovery_journal, mark_recovery_journal_applied,
    recovery_journal_path, remove_recovery_journal, remove_recovery_journal_for_transaction,
    save_recovery_journal,
};
pub use process_supervisor::{ProcessSupervisorConfig, supervise_process};
pub use recovery::{
    Consent, ExplorerTaskbarState, RecoveryHook, RecoveryShortcut, SafeModeProfile, TaskbarAction,
    TaskbarMutation, TaskbarRequest, TaskbarTransaction, begin_taskbar_transaction,
};
pub use recovery_coordinator::{RecoveryCoordinator, RecoveryCoordinatorError};
pub use shell_diagnostics::RetentionPolicy as RotationPolicy;
pub use supervisor::{
    ChildCleanup, ChildEvent, RestartDelay, SupervisorAction, SupervisorConfig, SupervisorState,
};
pub use taskbar_restore::{
    TaskbarObservation, TaskbarObservationId, TaskbarRestoreError, TaskbarRestoreOutcome,
    TaskbarRestorePlan, TaskbarRestorePlanError, TaskbarRestoreTarget, plan_taskbar_restore,
    restore_recovery_journal,
};

#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-watchdog"
}
