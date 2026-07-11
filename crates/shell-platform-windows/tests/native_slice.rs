use shell_platform_windows::{
    HitRegion, LifecycleMessage, PlatformEvent, classify_hit_test, translate_lifecycle_message,
};
use shell_renderer::{DipPoint, DipRect};

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
        translate_lifecycle_message(0, LifecycleMessage::DpiChanged),
        Some(PlatformEvent::DpiChanged)
    );
    assert_eq!(
        translate_lifecycle_message(0, LifecycleMessage::DisplayChanged),
        Some(PlatformEvent::DisplayChanged)
    );
    assert_eq!(
        translate_lifecycle_message(0, LifecycleMessage::PowerBroadcast),
        Some(PlatformEvent::PowerBroadcast)
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
