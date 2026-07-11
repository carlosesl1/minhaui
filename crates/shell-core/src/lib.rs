#![forbid(unsafe_code)]

mod events;
mod ids;
mod model;
mod reducer;
mod state;

pub use events::{Effect, NoOpReason, ShellEvent, Transition, TransitionError, TransitionOutcome};
pub use ids::{AppId, AppIdError, DockItemId, MonitorId, WindowId};
pub use model::{
    Accessibility, AutohideState, DockItem, DockVisibility, Monitor, PerformancePreset, PinState,
    Popover, RunningState, TaskbarPolicy, TopbarModule, TopbarModuleKind,
};
pub use reducer::reduce;
pub use state::{ShellState, StateError};

/// Returns the stable crate identity used by workspace smoke tests.
#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-core"
}
