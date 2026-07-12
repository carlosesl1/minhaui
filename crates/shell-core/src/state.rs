use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    Accessibility, AutohideState, DockItem, Monitor, MonitorId, PerformancePreset, Popover,
    TaskbarPolicy, TopbarModule, TopbarModuleKind,
};

/// Complete pure shell domain state consumed by platform adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellState {
    pub(crate) monitors: Vec<Monitor>,
    pub(crate) active_monitor: MonitorId,
    pub(crate) dock_items: Vec<DockItem>,
    pub(crate) topbar: Vec<TopbarModule>,
    pub(crate) active_popover: Option<Popover>,
    pub(crate) dock: AutohideState,
    pub(crate) taskbar_policy: TaskbarPolicy,
    pub(crate) accessibility: Accessibility,
    pub(crate) performance: PerformancePreset,
    pub(crate) safe_mode: bool,
}

impl Default for ShellState {
    fn default() -> Self {
        let monitor = Monitor::new(MonitorId::new(1), true);
        Self {
            monitors: vec![monitor],
            active_monitor: monitor.id(),
            dock_items: Vec::new(),
            topbar: vec![
                TopbarModule::new(TopbarModuleKind::SystemMenu, true),
                TopbarModule::new(TopbarModuleKind::Clock, true),
                TopbarModule::new(TopbarModuleKind::Network, true),
                TopbarModule::new(TopbarModuleKind::Volume, true),
                TopbarModule::new(TopbarModuleKind::Power, true),
                TopbarModule::new(TopbarModuleKind::Notifications, true),
            ],
            active_popover: None,
            dock: AutohideState::default(),
            taskbar_policy: TaskbarPolicy::Off,
            accessibility: Accessibility::new(false, false, true),
            performance: PerformancePreset::Balanced,
            safe_mode: false,
        }
    }
}

impl ShellState {
    /// Replaces dock entries for composition and deterministic tests.
    #[must_use]
    pub fn with_dock_items(mut self, items: Vec<DockItem>) -> Self {
        self.dock_items = items;
        self
    }

    /// Validates cross-field uniqueness and topology invariants.
    pub fn validate(&self) -> Result<(), StateError> {
        if self.monitors.is_empty() {
            return Err(StateError::NoMonitors);
        }
        if self
            .monitors
            .iter()
            .filter(|monitor| monitor.primary)
            .count()
            != 1
        {
            return Err(StateError::PrimaryMonitorCount);
        }
        if !self
            .monitors
            .iter()
            .any(|monitor| monitor.id == self.active_monitor)
        {
            return Err(StateError::UnknownActiveMonitor);
        }
        if !all_unique(self.dock_items.iter().map(|item| item.id)) {
            return Err(StateError::DuplicateDockItem);
        }
        if !all_unique(self.topbar.iter().map(|module| module.kind)) {
            return Err(StateError::DuplicateTopbarModule);
        }
        Ok(())
    }

    /// Borrows the monitor topology.
    #[must_use]
    pub fn monitors(&self) -> &[Monitor] {
        &self.monitors
    }
    /// Returns the active monitor.
    #[must_use]
    pub const fn active_monitor(&self) -> MonitorId {
        self.active_monitor
    }
    /// Borrows dock entries in display order.
    #[must_use]
    pub fn dock_items(&self) -> &[DockItem] {
        &self.dock_items
    }
    /// Borrows top-bar modules in display order.
    #[must_use]
    pub fn topbar_modules(&self) -> &[TopbarModule] {
        &self.topbar
    }
    /// Returns the active popover, if any.
    #[must_use]
    pub const fn active_popover(&self) -> Option<Popover> {
        self.active_popover
    }
    /// Returns autohide state.
    #[must_use]
    pub const fn dock(&self) -> AutohideState {
        self.dock
    }
    /// Returns the taskbar policy.
    #[must_use]
    pub const fn taskbar_policy(&self) -> TaskbarPolicy {
        self.taskbar_policy
    }
    /// Returns accessibility preferences.
    #[must_use]
    pub const fn accessibility(&self) -> Accessibility {
        self.accessibility
    }
    /// Returns the performance preset.
    #[must_use]
    pub const fn performance(&self) -> PerformancePreset {
        self.performance
    }
    /// Returns whether conservative safe mode is active.
    #[must_use]
    pub const fn safe_mode(&self) -> bool {
        self.safe_mode
    }
}

fn all_unique<T>(values: impl Iterator<Item = T>) -> bool
where
    T: Eq + std::hash::Hash,
{
    let mut seen = HashSet::new();
    values.into_iter().all(|value| seen.insert(value))
}

/// Describes a broken cross-field state invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum StateError {
    /// No monitors were supplied.
    #[error("at least one monitor is required")]
    NoMonitors,
    /// The monitor topology did not contain exactly one primary display.
    #[error("exactly one primary monitor is required")]
    PrimaryMonitorCount,
    /// The selected monitor was not present in the topology.
    #[error("active monitor is not present")]
    UnknownActiveMonitor,
    /// Two dock entries shared an identity.
    #[error("dock item identities must be unique")]
    DuplicateDockItem,
    /// Two top-bar modules shared a kind.
    #[error("topbar module kinds must be unique")]
    DuplicateTopbarModule,
}
