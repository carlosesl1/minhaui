#![forbid(unsafe_code)]

mod diagnostics;
mod instance;
mod recovery;
mod supervisor;

pub use diagnostics::{
    DiagnosticEvent, DiagnosticField, RotationPolicy, export_diagnostics, retained_log_segments,
};
pub use instance::{InstanceDecision, InstanceProbe, classify_instance};
pub use recovery::{
    Consent, ExplorerTaskbarState, RecoveryHook, RecoveryShortcut, SafeModeProfile, TaskbarAction,
    TaskbarMutation, TaskbarRequest, TaskbarTransaction, begin_taskbar_transaction,
};
pub use supervisor::{
    ChildEvent, RestartDelay, SupervisorAction, SupervisorConfig, SupervisorState,
};

#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-watchdog"
}
