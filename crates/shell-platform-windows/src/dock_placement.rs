#![deny(unsafe_code)]

use std::ops::Deref;

use shell_core::{MonitorId, WindowId};
use shell_renderer::{Dpi, PhysicalRect, ShellMetrics, dock_showcase_rect};

use crate::DockRuntimeConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DockPhysicalPlacement {
    rect: PhysicalRect,
    hidden_strip: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskbarEdge {
    None,
    Left,
    Top,
    Right,
    Bottom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorPlacementInput {
    monitor: MonitorId,
    bounds: PhysicalRect,
    work_area: PhysicalRect,
    dpi: Dpi,
}

impl MonitorPlacementInput {
    #[must_use]
    pub const fn new(
        monitor: MonitorId,
        bounds: PhysicalRect,
        work_area: PhysicalRect,
        dpi: Dpi,
    ) -> Self {
        Self {
            monitor,
            bounds,
            work_area,
            dpi,
        }
    }

    #[must_use]
    pub const fn monitor(self) -> MonitorId {
        self.monitor
    }

    #[must_use]
    pub const fn bounds(self) -> PhysicalRect {
        self.bounds
    }

    #[must_use]
    pub const fn work_area(self) -> PhysicalRect {
        self.work_area
    }

    #[must_use]
    pub const fn dpi(self) -> Dpi {
        self.dpi
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FullscreenObservation {
    window: WindowId,
    monitor: MonitorId,
    rect: PhysicalRect,
}

impl FullscreenObservation {
    #[must_use]
    pub const fn new(window: WindowId, monitor: MonitorId, rect: PhysicalRect) -> Self {
        Self {
            window,
            monitor,
            rect,
        }
    }

    #[must_use]
    pub const fn window(self) -> WindowId {
        self.window
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullscreenPolicy {
    suppressed: Vec<MonitorId>,
}

impl FullscreenPolicy {
    #[must_use]
    pub fn hide_for_fullscreen(
        observations: &[FullscreenObservation],
        monitors: &[MonitorPlacementInput],
    ) -> Self {
        let suppressed = observations
            .iter()
            .filter(|observation| {
                monitors.iter().any(|monitor| {
                    monitor.monitor == observation.monitor && monitor.bounds == observation.rect
                })
            })
            .map(|observation| observation.monitor)
            .collect();
        Self { suppressed }
    }

    #[must_use]
    pub fn suppresses(&self, monitor: MonitorId) -> bool {
        self.suppressed.contains(&monitor)
    }
}

impl Deref for FullscreenPolicy {
    type Target = [MonitorId];

    fn deref(&self) -> &Self::Target {
        &self.suppressed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorShellPlacement {
    monitor: MonitorId,
    dock: DockPhysicalPlacement,
    taskbar_edge: TaskbarEdge,
    suppressed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotReconcileAction {
    Remove(MonitorId),
    Create(MonitorId),
    Reuse(MonitorId),
}

impl MonitorShellPlacement {
    #[must_use]
    pub const fn monitor(self) -> MonitorId {
        self.monitor
    }

    #[must_use]
    pub const fn dock(self) -> DockPhysicalPlacement {
        self.dock
    }

    #[must_use]
    pub const fn taskbar_edge(self) -> TaskbarEdge {
        self.taskbar_edge
    }

    #[must_use]
    pub const fn suppressed(self) -> bool {
        self.suppressed
    }
}

#[must_use]
pub fn plan_monitor_placements(
    monitors: &[MonitorPlacementInput],
    config: DockRuntimeConfig,
    suppressed_monitors: &[MonitorId],
) -> Vec<MonitorShellPlacement> {
    monitors
        .iter()
        .map(|monitor| {
            let normal =
                dock_showcase_rect(monitor.work_area, monitor.dpi, ShellMetrics::default());
            MonitorShellPlacement {
                monitor: monitor.monitor,
                dock: DockPhysicalPlacement::from_visibility(
                    normal,
                    config,
                    false,
                    monitor.dpi.scale(),
                ),
                taskbar_edge: taskbar_edge(monitor.bounds, monitor.work_area),
                suppressed: suppressed_monitors.contains(&monitor.monitor),
            }
        })
        .collect()
}

#[must_use]
pub fn reconcile_monitor_slots(
    current: &[MonitorId],
    monitors: &[MonitorPlacementInput],
) -> Vec<SlotReconcileAction> {
    let mut actions = current
        .iter()
        .copied()
        .filter(|monitor| !monitors.iter().any(|next| next.monitor == *monitor))
        .map(SlotReconcileAction::Remove)
        .collect::<Vec<_>>();
    actions.extend(monitors.iter().map(|monitor| {
        if current.contains(&monitor.monitor) {
            SlotReconcileAction::Reuse(monitor.monitor)
        } else {
            SlotReconcileAction::Create(monitor.monitor)
        }
    }));
    actions
}

#[must_use]
pub const fn taskbar_edge(bounds: PhysicalRect, work_area: PhysicalRect) -> TaskbarEdge {
    let left = work_area.x - bounds.x;
    let top = work_area.y - bounds.y;
    let right = bounds.x + bounds.width - (work_area.x + work_area.width);
    let bottom = bounds.y + bounds.height - (work_area.y + work_area.height);
    if left > top && left > right && left > bottom {
        TaskbarEdge::Left
    } else if top > right && top > bottom {
        TaskbarEdge::Top
    } else if right > bottom {
        TaskbarEdge::Right
    } else if bottom > 0 {
        TaskbarEdge::Bottom
    } else {
        TaskbarEdge::None
    }
}

impl DockPhysicalPlacement {
    #[must_use]
    pub fn from_visibility(
        normal: PhysicalRect,
        config: DockRuntimeConfig,
        hidden: bool,
        scale: f32,
    ) -> Self {
        if !hidden || !config.autohide() {
            return Self {
                rect: normal,
                hidden_strip: false,
            };
        }
        let reveal_height = (config.reveal_zone_height() * scale).ceil() as i32;
        let height = reveal_height.clamp(1, normal.height.max(1));
        Self {
            rect: PhysicalRect::new(
                normal.x,
                normal.y + normal.height - height,
                normal.width,
                height,
            ),
            hidden_strip: true,
        }
    }

    #[must_use]
    pub const fn rect(self) -> PhysicalRect {
        self.rect
    }

    #[must_use]
    pub const fn is_hidden_strip(self) -> bool {
        self.hidden_strip
    }
}
