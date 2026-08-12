#![forbid(unsafe_code)]

mod calendar;
mod events;
mod ids;
mod model;
mod quick_settings;
mod reducer;
mod reducer_dock;
mod state;
mod watchdog_protocol;

pub use calendar::{CalendarDate, CalendarMonth, days_in_month};
pub use events::{Effect, NoOpReason, ShellEvent, Transition, TransitionError, TransitionOutcome};
pub use ids::{AppId, AppIdError, DockItemId, DockSeparatorId, MonitorId, WindowId};
pub use model::{
    Accessibility, AutohideState, DockItem, DockLayoutEntry, DockVisibility, Monitor,
    PerformancePreset, PinState, Popover, RunningState, TaskbarPolicy, TopbarIntent, TopbarModule,
    TopbarModuleKind,
};
pub use quick_settings::{QuickControlKind, QuickControlPlacement};
pub use reducer::reduce;
pub use state::{ShellState, StateError};
pub use watchdog_protocol::{
    ArmDeniedReason, ArmRequestId, FixedHexError, MAX_TASKBAR_ARMING_SNAPSHOTS,
    MAX_TASKBAR_DEVICE_ID_BYTES, MAX_WATCHDOG_CONTROL_FRAME_BYTES, RecoveryTransactionId,
    TaskbarArmingFingerprintError, TaskbarFingerprintBounds, TaskbarFingerprintSnapshot,
    TaskbarStateFingerprint, TaskbarWindowClass, WatchdogControlFrame, WatchdogFrameDirection,
    WatchdogProtocolError, taskbar_arming_fingerprint_v1,
};

/// Returns the stable crate identity used by workspace smoke tests.
#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-core"
}
