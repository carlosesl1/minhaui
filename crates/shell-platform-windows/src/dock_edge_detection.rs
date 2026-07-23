#![deny(unsafe_code)]

use shell_renderer::PhysicalRect;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DockEdgeProbeMode {
    Disabled,
    Far,
    Near,
    Animating,
}

impl DockEdgeProbeMode {
    pub(crate) const fn interval_ms(self) -> Option<u32> {
        match self {
            Self::Disabled => None,
            Self::Far => Some(125),
            Self::Near => Some(25),
            Self::Animating => Some(16),
        }
    }
}

pub(crate) const fn dock_edge_probe_mode(
    hidden: bool,
    edge_reveal_active: bool,
    cursor_near: bool,
    visibility_animating: bool,
) -> DockEdgeProbeMode {
    if visibility_animating {
        DockEdgeProbeMode::Animating
    } else if edge_reveal_active || cursor_near {
        DockEdgeProbeMode::Near
    } else if hidden {
        DockEdgeProbeMode::Far
    } else {
        DockEdgeProbeMode::Disabled
    }
}

pub(crate) const fn cursor_hits_physical_bottom(
    monitor: PhysicalRect,
    cursor_x: i32,
    cursor_y: i32,
) -> bool {
    if monitor.width <= 0 || monitor.height <= 0 {
        return false;
    }
    let left = monitor.x as i64;
    let right = left + monitor.width as i64;
    let bottom = monitor.y as i64 + monitor.height as i64 - 1;
    let cursor_x = cursor_x as i64;
    cursor_x >= left && cursor_x < right && cursor_y as i64 == bottom
}

pub(crate) const fn cursor_near_physical_bottom(
    monitor: PhysicalRect,
    cursor_x: i32,
    cursor_y: i32,
    threshold_pixels: i32,
) -> bool {
    if monitor.width <= 0 || monitor.height <= 0 || threshold_pixels <= 0 {
        return false;
    }
    let left = monitor.x as i64;
    let right = left + monitor.width as i64;
    let bottom = monitor.y as i64 + monitor.height as i64 - 1;
    let candidate_top = bottom - threshold_pixels as i64 + 1;
    let monitor_top = monitor.y as i64;
    let top = if candidate_top > monitor_top {
        candidate_top
    } else {
        monitor_top
    };
    let x = cursor_x as i64;
    let y = cursor_y as i64;
    x >= left && x < right && y >= top && y <= bottom
}

pub(crate) const fn cursor_inside_rect(rect: PhysicalRect, cursor_x: i32, cursor_y: i32) -> bool {
    if rect.width <= 0 || rect.height <= 0 {
        return false;
    }
    let x = cursor_x as i64;
    let y = cursor_y as i64;
    let left = rect.x as i64;
    let top = rect.y as i64;
    x >= left && x < left + rect.width as i64 && y >= top && y < top + rect.height as i64
}

pub(crate) const fn cursor_inside_approach_corridor(
    monitor: PhysicalRect,
    dock: PhysicalRect,
    cursor_x: i32,
    cursor_y: i32,
) -> bool {
    if monitor.height <= 0 || dock.width <= 0 || dock.height <= 0 {
        return false;
    }
    let x = cursor_x as i64;
    let y = cursor_y as i64;
    let dock_left = dock.x as i64;
    let dock_right = dock_left + dock.width as i64;
    let dock_bottom = dock.y as i64 + dock.height as i64;
    let monitor_bottom = monitor.y as i64 + monitor.height as i64 - 1;
    x >= dock_left && x < dock_right && y >= dock_bottom && y <= monitor_bottom
}

#[cfg(test)]
mod tests {
    use shell_renderer::PhysicalRect;

    use super::{
        DockEdgeProbeMode, cursor_hits_physical_bottom, cursor_inside_approach_corridor,
        cursor_inside_rect, cursor_near_physical_bottom, dock_edge_probe_mode,
    };

    #[test]
    fn exact_last_pixel_of_monitor_triggers_hidden_dock() {
        let monitor = PhysicalRect::new(-2560, -49, 2560, 1440);

        assert!(cursor_hits_physical_bottom(monitor, -1280, 1390));
        assert!(!cursor_hits_physical_bottom(monitor, -1280, 1389));
        assert!(!cursor_hits_physical_bottom(monitor, 0, 1390));
    }

    #[test]
    fn dock_bounds_use_half_open_physical_coordinates() {
        let dock = PhysicalRect::new(700, 980, 520, 55);

        assert!(cursor_inside_rect(dock, 700, 980));
        assert!(cursor_inside_rect(dock, 1219, 1034));
        assert!(!cursor_inside_rect(dock, 1220, 1034));
        assert!(!cursor_inside_rect(dock, 1219, 1035));
    }

    #[test]
    fn cursor_can_travel_from_monitor_bottom_to_dock_without_leaving_activation() {
        let monitor = PhysicalRect::new(0, 0, 2560, 1440);
        let dock = PhysicalRect::new(980, 1320, 600, 55);

        assert!(cursor_inside_approach_corridor(monitor, dock, 1280, 1438));
        assert!(cursor_inside_approach_corridor(monitor, dock, 1280, 1375));
        assert!(!cursor_inside_approach_corridor(monitor, dock, 900, 1400));
        assert!(!cursor_inside_approach_corridor(monitor, dock, 1280, 1319));
    }

    #[test]
    fn hidden_dock_uses_adaptive_edge_probe_cadence() {
        assert_eq!(
            dock_edge_probe_mode(false, false, false, false),
            DockEdgeProbeMode::Disabled
        );
        assert_eq!(
            dock_edge_probe_mode(true, false, false, false),
            DockEdgeProbeMode::Far
        );
        assert_eq!(
            dock_edge_probe_mode(true, false, true, false),
            DockEdgeProbeMode::Near
        );
        assert_eq!(
            dock_edge_probe_mode(true, false, false, true),
            DockEdgeProbeMode::Animating
        );
        assert_eq!(
            dock_edge_probe_mode(false, true, false, false),
            DockEdgeProbeMode::Near
        );
    }

    #[test]
    fn near_bottom_zone_respects_negative_monitor_coordinates() {
        let monitor = PhysicalRect::new(-1920, -100, 1920, 1080);

        assert!(cursor_near_physical_bottom(monitor, -960, 979, 48));
        assert!(cursor_near_physical_bottom(monitor, -960, 932, 48));
        assert!(!cursor_near_physical_bottom(monitor, -960, 931, 48));
        assert!(!cursor_near_physical_bottom(monitor, 0, 979, 48));
    }
}
