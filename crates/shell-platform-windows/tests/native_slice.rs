use shell_platform_windows::{
    DockRenderAction, DockRenderChange, HitRegion, LifecycleMessage, PlatformEvent, RuntimeAction,
    RuntimeOrchestrator, classify_dock_render_action, classify_hit_test,
    translate_lifecycle_message,
};
use shell_renderer::{DipPoint, DipRect, PhysicalRect};

#[test]
fn translates_taskbarcreated_to_typed_lifecycle_event() {
    let taskbar_created = LifecycleMessage::TaskbarCreated(0xC123);
    let event = translate_lifecycle_message(0xC123, taskbar_created);
    assert_eq!(event, Some(PlatformEvent::TaskbarCreated));
}

#[test]
fn leaves_unknown_messages_at_win32_boundary() {
    let taskbar_created = LifecycleMessage::TaskbarCreated(0xC123);
    let event = translate_lifecycle_message(0xC124, taskbar_created);
    assert_eq!(event, None);
}

#[test]
fn maps_native_lifecycle_messages_to_typed_events() {
    assert_eq!(
        translate_lifecycle_message(
            0,
            LifecycleMessage::DpiChanged(PhysicalRect::new(10, 20, 300, 80))
        ),
        Some(PlatformEvent::DpiChanged(PhysicalRect::new(
            10, 20, 300, 80
        )))
    );
    assert_eq!(
        translate_lifecycle_message(0, LifecycleMessage::DisplayChanged),
        Some(PlatformEvent::DisplayChanged)
    );
    assert_eq!(
        translate_lifecycle_message(0, LifecycleMessage::PowerResumed),
        Some(PlatformEvent::PowerResumed)
    );
    assert_eq!(
        translate_lifecycle_message(0, LifecycleMessage::Timer),
        Some(PlatformEvent::QaExitRequested)
    );
    assert_eq!(
        translate_lifecycle_message(0, LifecycleMessage::Close),
        Some(PlatformEvent::CloseRequested)
    );
    assert_eq!(
        translate_lifecycle_message(0, LifecycleMessage::Destroy),
        Some(PlatformEvent::Destroyed)
    );
}

#[test]
fn runtime_orchestrator_increments_generation_for_real_rebuild_triggers() {
    let mut runtime = RuntimeOrchestrator::new();
    assert_eq!(runtime.generation(), 1);
    assert_eq!(
        runtime.handle(PlatformEvent::DeviceLost),
        RuntimeAction::Rebuild
    );
    assert_eq!(runtime.generation(), 2);
    assert_eq!(
        runtime.handle(PlatformEvent::DisplayChanged),
        RuntimeAction::RepositionAndRebuild
    );
    assert_eq!(runtime.generation(), 3);
    assert_eq!(
        runtime.handle(PlatformEvent::PowerResumed),
        RuntimeAction::Rebuild
    );
    assert_eq!(runtime.generation(), 4);
    assert_eq!(
        runtime.handle(PlatformEvent::TaskbarCreated),
        RuntimeAction::RepositionAndRebuild
    );
    assert_eq!(runtime.generation(), 5);
    assert_eq!(
        runtime.handle(PlatformEvent::DpiChanged(PhysicalRect::new(1, 2, 3, 4))),
        RuntimeAction::ResizeAndRebuild(PhysicalRect::new(1, 2, 3, 4))
    );
    assert_eq!(runtime.generation(), 6);
}

#[test]
fn dock_hover_visual_change_requests_redraw_without_resource_generation() {
    // Given: a runtime generation that only tracks native resource rebuilds.
    let runtime = RuntimeOrchestrator::new();
    let generation = runtime.generation();

    // When: dock state is unchanged but hover/magnification changes visuals.
    let action = classify_dock_render_action(DockRenderChange {
        state_changed: false,
        visual_changed: true,
        dock_visibility_changed: false,
        rebuild_requested: false,
    });

    // Then: the dock needs only an in-place redraw, not surface recreation.
    assert_eq!(action, DockRenderAction::RedrawDock);
    assert_eq!(runtime.generation(), generation);
}

#[test]
fn dock_visibility_change_still_requests_rebuild_for_resized_strip() {
    // Given: autohide changes the dock HWND size.
    // When: the dock visibility flag changed while handling a pointer event.
    let action = classify_dock_render_action(DockRenderChange {
        state_changed: true,
        visual_changed: false,
        dock_visibility_changed: true,
        rebuild_requested: false,
    });

    // Then: the swap-chain-sized surface must still be rebuilt.
    assert_eq!(action, DockRenderAction::RebuildSurfaces);
}

#[test]
fn classifies_transparent_rounded_corners_as_click_through() {
    let shell_bounds = DipRect::new(0.0, 0.0, 300.0, 72.0);
    let hit_region = classify_hit_test(shell_bounds, 18.0, DipPoint::new(3.0, 3.0));
    assert_eq!(hit_region, HitRegion::Transparent);
}

#[test]
fn classifies_visible_shell_content_as_interactive() {
    let shell_bounds = DipRect::new(0.0, 0.0, 300.0, 72.0);
    let hit_region = classify_hit_test(shell_bounds, 18.0, DipPoint::new(150.0, 36.0));
    assert_eq!(hit_region, HitRegion::Interactive);
}
