use serde::{Deserialize, Serialize};

use crate::{AppId, DockItemId, MonitorId, WindowId};

/// Defines how the native Windows taskbar is treated.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarPolicy {
    /// Leaves the taskbar untouched.
    #[default]
    Off,
    /// Requests the platform's normal auto-hide behavior.
    AutoHide,
    /// Requests that the taskbar be hidden while the shell is active.
    Hide,
}

/// Selects bounded rendering and polling budgets.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformancePreset {
    /// Preserves visual quality while respecting normal budgets.
    #[default]
    Balanced,
    /// Reduces animation and background refresh work.
    BatterySaver,
    /// Uses the highest supported visual quality.
    HighQuality,
}

/// Captures user and system accessibility preferences.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accessibility {
    pub(crate) reduced_motion: bool,
    pub(crate) high_contrast: bool,
    pub(crate) transparency: bool,
}

impl Accessibility {
    /// Creates validated accessibility preferences from platform/user settings.
    #[must_use]
    pub const fn new(reduced_motion: bool, high_contrast: bool, transparency: bool) -> Self {
        Self {
            reduced_motion,
            high_contrast,
            transparency,
        }
    }

    /// Returns whether nonessential motion must be removed.
    #[must_use]
    pub const fn reduced_motion(self) -> bool {
        self.reduced_motion
    }

    /// Returns whether forced high-contrast presentation is active.
    #[must_use]
    pub const fn high_contrast(self) -> bool {
        self.high_contrast
    }

    /// Returns whether transparent materials are allowed.
    #[must_use]
    pub const fn transparency(self) -> bool {
        self.transparency
    }
}

/// Describes whether an entry is retained independently of running windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinState {
    /// The entry remains in the dock after its last window closes.
    Pinned,
    /// The entry exists only while it has a running window.
    Unpinned,
}

/// Describes the running window associated with a dock entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RunningState {
    /// No window is currently associated with the entry.
    Stopped,
    /// A platform window is associated with the entry.
    Running {
        /// Stable platform window identity.
        window: WindowId,
        /// Whether the window currently owns foreground focus.
        focused: bool,
        /// Whether the window is minimized.
        minimized: bool,
    },
}

/// A typed dock entry combining application, pin, and running state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockItem {
    pub(crate) id: DockItemId,
    pub(crate) app: AppId,
    pub(crate) pin: PinState,
    pub(crate) running: RunningState,
}

impl DockItem {
    /// Creates a pinned, stopped dock entry.
    #[must_use]
    pub const fn pinned(id: DockItemId, app: AppId) -> Self {
        Self {
            id,
            app,
            pin: PinState::Pinned,
            running: RunningState::Stopped,
        }
    }

    /// Returns the dock identity.
    #[must_use]
    pub const fn id(&self) -> DockItemId {
        self.id
    }

    /// Borrows the application identity.
    #[must_use]
    pub const fn app(&self) -> &AppId {
        &self.app
    }

    /// Returns the pin state.
    #[must_use]
    pub const fn pin(&self) -> PinState {
        self.pin
    }

    /// Borrows the running state.
    #[must_use]
    pub const fn running(&self) -> &RunningState {
        &self.running
    }
}

/// One display available to the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Monitor {
    pub(crate) id: MonitorId,
    pub(crate) primary: bool,
}

impl Monitor {
    /// Creates a monitor description.
    #[must_use]
    pub const fn new(id: MonitorId, primary: bool) -> Self {
        Self { id, primary }
    }

    /// Returns its stable identity.
    #[must_use]
    pub const fn id(self) -> MonitorId {
        self.id
    }

    /// Returns whether this is the primary monitor.
    #[must_use]
    pub const fn is_primary(self) -> bool {
        self.primary
    }
}

/// Identifies a top-bar module with fixed semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopbarModuleKind {
    /// Clock and calendar entry point.
    Clock,
    /// Network status.
    Network,
    /// Audio status.
    Volume,
    /// Power status.
    Power,
    /// Notification center entry point.
    Notifications,
}

/// Stores one module's order and visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopbarModule {
    pub(crate) kind: TopbarModuleKind,
    pub(crate) visible: bool,
}

impl TopbarModule {
    /// Creates a module placement.
    #[must_use]
    pub const fn new(kind: TopbarModuleKind, visible: bool) -> Self {
        Self { kind, visible }
    }

    /// Returns the fixed module kind.
    #[must_use]
    pub const fn kind(self) -> TopbarModuleKind {
        self.kind
    }

    /// Returns whether the module is visible.
    #[must_use]
    pub const fn visible(self) -> bool {
        self.visible
    }
}

/// Identifies the single popover allowed to be active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Popover {
    /// Calendar popover.
    Calendar,
    /// Network details popover.
    Network,
    /// Volume and device popover.
    Volume,
    /// Power/session popover.
    Power,
    /// Notification center popover.
    Notifications,
}

/// Describes current dock visibility.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockVisibility {
    /// Dock surface is visible.
    #[default]
    Revealed,
    /// Dock surface is hidden behind its reveal target.
    Hidden,
}

/// Stores autohide enablement and current visibility.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutohideState {
    pub(crate) enabled: bool,
    pub(crate) visibility: DockVisibility,
}

impl AutohideState {
    /// Returns whether autohide is enabled.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Returns whether the dock is currently revealed.
    #[must_use]
    pub const fn is_revealed(self) -> bool {
        matches!(self.visibility, DockVisibility::Revealed)
    }
}
