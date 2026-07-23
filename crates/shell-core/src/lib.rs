#![forbid(unsafe_code)]

mod calendar;
mod events;
mod ids;
mod model;
mod reducer;
mod reducer_dock;
mod state;

pub use calendar::{CalendarDate, CalendarMonth, days_in_month};
pub use events::{Effect, NoOpReason, ShellEvent, Transition, TransitionError, TransitionOutcome};
pub use ids::{AppId, AppIdError, DockItemId, DockSeparatorId, MonitorId, WindowId};
pub use model::{
    Accessibility, AutohideState, DockItem, DockLayoutEntry, DockVisibility, Monitor,
    PerformancePreset, PinState, Popover, RunningState, TaskbarPolicy, TopbarIntent, TopbarModule,
    TopbarModuleKind,
};
pub use reducer::reduce;
pub use state::{ShellState, StateError};

/// Returns the stable crate identity used by workspace smoke tests.
#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-core"
}
