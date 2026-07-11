use crate::{
    AppId, DockItem, DockItemId, Monitor, MonitorId, PerformancePreset, Popover, ShellState,
    StateError, TaskbarPolicy, TopbarModuleKind, WindowId,
};
use thiserror::Error;

/// An external action requested by a pure state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Launches an application through the platform adapter.
    Launch(AppId),
    /// Focuses a platform window.
    FocusWindow(WindowId),
    /// Minimizes a platform window.
    MinimizeWindow(WindowId),
    /// Persists user-controlled configuration.
    PersistConfiguration,
    /// Rebuilds per-monitor native surfaces.
    RebuildSurfaces,
    /// Applies a taskbar policy through the platform adapter.
    ApplyTaskbarPolicy(TaskbarPolicy),
}

/// A typed input to the deterministic shell reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    /// Activates a dock entry using launch/focus/minimize semantics.
    ActivateDockItem(DockItemId),
    /// Records a newly observed running window.
    WindowOpened { item: DockItemId, window: WindowId },
    /// Records that an item's running window closed.
    WindowClosed(DockItemId),
    /// Adds or pins a dock entry.
    Pin(DockItem),
    /// Removes persistent pinning from an entry.
    Unpin(DockItemId),
    /// Moves an entry before another entry or to the end.
    ReorderDockItem {
        item: DockItemId,
        before: Option<DockItemId>,
    },
    /// Opens the exclusive popover slot.
    OpenPopover(Popover),
    /// Closes the exclusive popover slot.
    DismissPopover,
    /// Enables or disables autohide.
    EnableAutohide(bool),
    /// Hides an enabled autohide dock.
    HideDock,
    /// Reveals the dock.
    RevealDock,
    /// Replaces monitor topology after a display change.
    DisplaysChanged(Vec<Monitor>),
    /// Selects a taskbar policy.
    SetTaskbarPolicy(TaskbarPolicy),
    /// Changes one top-bar module's visibility.
    SetTopbarVisibility {
        module: TopbarModuleKind,
        visible: bool,
    },
    /// Selects a performance budget.
    SetPerformance(PerformancePreset),
    /// Enters conservative recovery presentation.
    EnterSafeMode,
}

/// Names why a valid event intentionally changed nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoOpReason {
    /// Dock was already hidden.
    AlreadyHidden,
    /// Dock was already revealed.
    AlreadyRevealed,
    /// Popover slot was already empty.
    NoActivePopover,
    /// Requested value already matched state.
    AlreadyConfigured,
    /// Unpin targeted an entry that was not present.
    ItemNotPresent,
}

/// Reports whether a transition applied a state change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionOutcome {
    /// State and/or effects changed.
    Applied,
    /// The event was valid but redundant.
    NoOp(NoOpReason),
}

/// The next immutable state plus platform intents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    /// Next validated shell state.
    pub state: ShellState,
    /// Typed effects for outer I/O adapters.
    pub effects: Vec<Effect>,
    /// Whether the event applied or was redundant.
    pub outcome: TransitionOutcome,
}

pub(crate) fn applied(effects: Vec<Effect>) -> (Vec<Effect>, TransitionOutcome) {
    (effects, TransitionOutcome::Applied)
}

pub(crate) fn no_op(reason: NoOpReason) -> (Vec<Effect>, TransitionOutcome) {
    (Vec::new(), TransitionOutcome::NoOp(reason))
}

/// Describes an invalid reducer input or source state.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransitionError {
    /// Source or generated state violated a domain invariant.
    #[error("invalid shell state: {0}")]
    InvalidState(StateError),
    /// A dock identity was not present.
    #[error("unknown dock item {0:?}")]
    UnknownDockItem(DockItemId),
    /// A new dock entry reused an existing identity.
    #[error("duplicate dock item {0:?}")]
    DuplicateDockItem(DockItemId),
    /// A top-bar module was not configured.
    #[error("unknown topbar module {0:?}")]
    UnknownTopbarModule(TopbarModuleKind),
    /// Display topology was empty.
    #[error("display topology must contain a monitor")]
    NoMonitors,
    /// Display topology did not contain exactly one primary monitor.
    #[error("display topology must contain exactly one primary monitor")]
    InvalidMonitorTopology,
    /// An active monitor identity was not available after topology change.
    #[error("active monitor {0:?} is unavailable")]
    UnknownMonitor(MonitorId),
}
