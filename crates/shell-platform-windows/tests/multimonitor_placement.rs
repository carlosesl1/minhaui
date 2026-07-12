use shell_core::{MonitorId, WindowId};
use shell_platform_windows::{
    DockRuntimeConfig, FullscreenObservation, FullscreenPolicy, MonitorPlacementInput,
    SlotReconcileAction, TaskbarEdge, plan_monitor_placements, reconcile_monitor_slots,
};
use shell_renderer::{Dpi, PhysicalRect};

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
        PhysicalRect::new(440, 848, 1040, 180)
    );
    assert_eq!(placements[1].monitor(), MonitorId::new(2));
    assert_eq!(placements[1].taskbar_edge(), TaskbarEdge::Left);
    assert_eq!(
        placements[1].dock().rect(),
        PhysicalRect::new(-2020, 1032, 1560, 270)
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
fn hotplug_reconcile_adds_removes_and_reorders_monitor_slots() {
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

    // Then: removed slots are dropped and remaining slots follow current topology.
    assert_eq!(
        actions,
        vec![
            SlotReconcileAction::Remove(MonitorId::new(1)),
            SlotReconcileAction::Create(MonitorId::new(3)),
            SlotReconcileAction::Reuse(MonitorId::new(2)),
        ]
    );
}
