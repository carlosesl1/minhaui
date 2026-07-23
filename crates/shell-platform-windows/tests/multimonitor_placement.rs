use crate::{
    DockEdgeGeometry, DockPhysicalPlacement, DockRuntimeConfig, FullscreenObservation,
    FullscreenPolicy, MonitorPlacementInput, SlotReconcileAction, TaskbarEdge,
    plan_monitor_placements, reconcile_monitor_slots,
};
use shell_core::{MonitorId, WindowId};
use shell_renderer::{Dpi, PhysicalRect, ShellMetrics, dock_showcase_rect};

#[test]
fn hidden_reveal_strip_is_flush_with_the_secondary_monitor_work_area() {
    // Given: a high-DPI secondary monitor whose visible dock has a bottom margin.
    let work = PhysicalRect::new(1920, 0, 2560, 1400);
    let dpi = Dpi::from_raw(144);
    let normal = dock_showcase_rect(work, dpi, ShellMetrics::default());
    let config = DockRuntimeConfig::default().with_autohide(true);

    // When: the dock collapses to its reveal strip at that monitor's edge.
    let hidden = DockPhysicalPlacement::from_visibility_at_edge(
        normal,
        config,
        true,
        dpi.scale(),
        work.y + work.height,
    );

    // Then: the full-height HWND starts at the reveal edge and extends off-screen.
    assert_eq!(hidden.rect().y, work.y + work.height - 12);
    assert_eq!(hidden.rect().height, normal.height);
}

#[test]
fn hidden_sensor_spans_the_physical_monitor_edge_when_the_work_area_is_inset() {
    // Given: a dock inside an inset work area on a negative-coordinate monitor.
    let monitor = PhysicalRect::new(-2560, -120, 2560, 1440);
    let normal = PhysicalRect::new(-1760, 1200, 960, 83);
    let geometry = DockEdgeGeometry::new(normal, monitor, 1.5);
    let config = DockRuntimeConfig::default().with_autohide(true);

    // When: the hidden edge sensor placement is calculated.
    let hidden = DockPhysicalPlacement::from_visibility_at_monitor_edge(geometry, config, true);

    // Then: the input surface spans the monitor and reaches its last physical row.
    assert_eq!(hidden.rect(), PhysicalRect::new(-2560, 1308, 2560, 83));
    assert!(hidden.is_hidden_strip());
}

#[test]
fn positions_each_monitor_independently_with_signed_coordinates_and_dpi() {
    // Given: two monitors with mixed DPI and a secondary display left of origin.
    let monitors = [
        MonitorPlacementInput::new(
            MonitorId::new(1),
            PhysicalRect::new(0, 0, 1920, 1080),
            PhysicalRect::new(0, 0, 1920, 1040),
            Dpi::from_raw(96),
        ),
        MonitorPlacementInput::new(
            MonitorId::new(2),
            PhysicalRect::new(-2560, -120, 2560, 1440),
            PhysicalRect::new(-2480, -80, 2480, 1400),
            Dpi::from_raw(144),
        ),
    ];

    // When: dock placements are planned for every monitor.
    let placements = plan_monitor_placements(&monitors, DockRuntimeConfig::default(), &[]);

    // Then: each dock uses its own work area and DPI without clamping signed origins.
    assert_eq!(placements.len(), 2);
    assert_eq!(placements[0].monitor(), MonitorId::new(1));
    assert_eq!(placements[0].taskbar_edge(), TaskbarEdge::Bottom);
    assert_eq!(
        placements[0].dock().rect(),
        PhysicalRect::new(580, 981, 760, 55)
    );
    assert_eq!(placements[1].monitor(), MonitorId::new(2));
    assert_eq!(placements[1].taskbar_edge(), TaskbarEdge::Left);
    assert_eq!(
        placements[1].dock().rect(),
        PhysicalRect::new(-1810, 1231, 1140, 83)
    );
}

#[test]
fn fullscreen_policy_suppresses_only_the_covered_monitor() {
    // Given: two monitors and a fullscreen window on the secondary monitor.
    let monitors = [
        MonitorPlacementInput::new(
            MonitorId::new(1),
            PhysicalRect::new(0, 0, 1920, 1080),
            PhysicalRect::new(0, 0, 1920, 1040),
            Dpi::from_raw(96),
        ),
        MonitorPlacementInput::new(
            MonitorId::new(2),
            PhysicalRect::new(1920, 0, 2560, 1440),
            PhysicalRect::new(1920, 0, 2560, 1400),
            Dpi::from_raw(144),
        ),
    ];
    let fullscreen = [FullscreenObservation::new(
        WindowId::new(900),
        MonitorId::new(2),
        PhysicalRect::new(1920, 0, 2560, 1440),
    )];

    // When: the fullscreen hide policy is applied.
    let placements = plan_monitor_placements(
        &monitors,
        DockRuntimeConfig::default(),
        &FullscreenPolicy::hide_for_fullscreen(&fullscreen, &monitors),
    );

    // Then: the secondary dock is suppressed while the primary remains visible.
    assert!(!placements[0].suppressed());
    assert!(placements[1].suppressed());
}

#[test]
fn hotplug_reconcile_creates_replacements_before_removing_old_slots() {
    // Given: two existing slots and a new topology that removes one, keeps one,
    // and adds a third monitor before the reused slot.
    let current = [MonitorId::new(1), MonitorId::new(2)];
    let monitors = [
        MonitorPlacementInput::new(
            MonitorId::new(3),
            PhysicalRect::new(-1280, 0, 1280, 720),
            PhysicalRect::new(-1280, 0, 1280, 680),
            Dpi::from_raw(96),
        ),
        MonitorPlacementInput::new(
            MonitorId::new(2),
            PhysicalRect::new(0, 0, 1920, 1080),
            PhysicalRect::new(0, 0, 1920, 1040),
            Dpi::from_raw(144),
        ),
    ];

    // When: the current slots are reconciled against the new monitor order.
    let actions = reconcile_monitor_slots(&current, &monitors);

    // Then: replacement windows exist before stale slots are destroyed, so the
    // Win32 live-window count never reaches zero during a topology transition.
    assert_eq!(
        actions,
        vec![
            SlotReconcileAction::Create(MonitorId::new(3)),
            SlotReconcileAction::Reuse(MonitorId::new(2)),
            SlotReconcileAction::Remove(MonitorId::new(1)),
        ]
    );
}
