use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    Accessibility, AutohideState, DockItem, DockLayoutEntry, Monitor, MonitorId, PerformancePreset,
    PinState, Popover, TaskbarPolicy, TopbarModule, TopbarModuleKind,
};

/// Complete pure shell domain state consumed by platform adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellState {
    pub(crate) monitors: Vec<Monitor>,
    pub(crate) active_monitor: MonitorId,
    pub(crate) dock_items: Vec<DockItem>,
    #[serde(default)]
    pub(crate) dock_layout: Vec<DockLayoutEntry>,
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
            dock_layout: Vec::new(),
            topbar: vec![
                TopbarModule::new(TopbarModuleKind::SystemMenu, true),
                TopbarModule::new(TopbarModuleKind::Network, true),
                TopbarModule::new(TopbarModuleKind::Volume, true),
                TopbarModule::new(TopbarModuleKind::Power, true),
                TopbarModule::new(TopbarModuleKind::Notifications, true),
                TopbarModule::new(TopbarModuleKind::Clock, true),
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
        self.dock_layout = items
            .iter()
            .filter(|item| item.pin() == PinState::Pinned)
            .map(|item| DockLayoutEntry::App(item.id()))
            .collect();
        self.dock_items = items;
        self
    }

    #[must_use]
    pub fn with_dock_layout(mut self, layout: Vec<DockLayoutEntry>) -> Self {
        self.dock_layout = layout;
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
        if !all_unique(self.dock_layout.iter().copied()) {
            return Err(StateError::DuplicateDockLayoutEntry);
        }
        if !all_unique(
            self.dock_layout
                .iter()
                .copied()
                .map(DockLayoutEntry::visual_id),
        ) {
            return Err(StateError::AmbiguousDockVisualIdentity);
        }
        if self.dock_layout.iter().any(|entry| {
            matches!(entry, DockLayoutEntry::Separator(separator) if self.dock_items.iter().any(|item| item.id().value() == separator.value()))
        }) {
            return Err(StateError::AmbiguousDockVisualIdentity);
        }
        let layout_apps = self
            .dock_layout
            .iter()
            .filter_map(|entry| match entry {
                DockLayoutEntry::App(id) => Some(*id),
                DockLayoutEntry::Separator(_) => None,
            })
            .collect::<HashSet<_>>();
        if self
            .dock_layout
            .iter()
            .any(|entry| matches!(entry, DockLayoutEntry::App(id) if !self.dock_items.iter().any(|item| item.id() == *id && item.pin() == PinState::Pinned)))
            || self
                .dock_items
                .iter()
                .filter(|item| item.pin() == PinState::Pinned)
                .any(|item| !layout_apps.contains(&item.id()))
        {
            return Err(StateError::InvalidDockLayout);
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
    #[must_use]
    pub fn dock_layout(&self) -> &[DockLayoutEntry] {
        &self.dock_layout
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
    #[error("dock layout entries must be unique")]
    DuplicateDockLayoutEntry,
    #[error("dock apps and separators must not share a visual identity")]
    AmbiguousDockVisualIdentity,
    #[error("dock layout must reference every pinned app exactly once")]
    InvalidDockLayout,
    /// Two top-bar modules shared a kind.
    #[error("topbar module kinds must be unique")]
    DuplicateTopbarModule,
}
