use shell_core::{AppId, DockItem, DockItemId, MonitorId, ShellState};
use shell_platform_windows::{
    DockController, DockPointerPhase, DockPointerSample, DockRuntimeConfig, NativeEventTarget,
    NativeRouteDecision, NativeWindowId, NativeWindowSlot, route_native_event_to_slot,
};
use shell_renderer::{DipPoint, DipRect};

fn state(label: &str) -> Result<ShellState, Box<dyn std::error::Error>> {
    Ok(ShellState::default().with_dock_items(vec![DockItem::pinned(
        DockItemId::new(1),
        AppId::parse(label)?,
    )]))
}

#[test]
fn native_hwnd_events_mutate_only_the_target_monitor_slot() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: two dock controllers registered to distinct native HWND slots.
    let slots = [
        NativeWindowSlot::new(
            MonitorId::new(10),
            NativeWindowId::new(100),
            NativeWindowId::new(101),
        ),
        NativeWindowSlot::new(
            MonitorId::new(20),
            NativeWindowId::new(200),
            NativeWindowId::new(201),
        ),
    ];
    let mut first = DockController::new(state("first.exe")?, DockRuntimeConfig::default())?;
    let mut second = DockController::new(state("second.exe")?, DockRuntimeConfig::default())?;
    first.update_surface(DipRect::new(0.0, 0.0, 260.0, 96.0));
    second.update_surface(DipRect::new(0.0, 0.0, 260.0, 96.0));

    // When: a native hover event arrives for the first dock HWND.
    let route =
        route_native_event_to_slot(&slots, NativeEventTarget::Window(NativeWindowId::new(101)));
    if route == NativeRouteDecision::Slot(MonitorId::new(10)) {
        first.handle_pointer(DockPointerSample::new(
            DockPointerPhase::Moved,
            DipPoint::new(130.0, 48.0),
        ))?;
    }
    if route == NativeRouteDecision::Slot(MonitorId::new(20)) {
        second.handle_pointer(DockPointerSample::new(
            DockPointerPhase::Moved,
            DipPoint::new(130.0, 48.0),
        ))?;
    }

    // Then: monitor 10 changed hover state and monitor 20 remained untouched.
    assert_eq!(first.scene().hovered_item(), Some(1));
    assert_eq!(second.scene().hovered_item(), None);
    Ok(())
}

#[test]
fn native_broadcast_events_are_not_misrouted_to_a_single_slot() {
    // Given: two registered native slots.
    let slots = [
        NativeWindowSlot::new(
            MonitorId::new(10),
            NativeWindowId::new(100),
            NativeWindowId::new(101),
        ),
        NativeWindowSlot::new(
            MonitorId::new(20),
            NativeWindowId::new(200),
            NativeWindowId::new(201),
        ),
    ];

    // When: Windows sends a global display-change style event.
    let route = route_native_event_to_slot(&slots, NativeEventTarget::Broadcast);

    // Then: the shell manager handles it as a broadcast instead of slot mutation.
    assert_eq!(route, NativeRouteDecision::Broadcast);
}
