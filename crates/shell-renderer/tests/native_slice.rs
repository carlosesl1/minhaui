use shell_renderer::{
    DeviceEvent, DeviceLifecycle, DipPoint, DipRect, Dpi, PhysicalRect, Rgba8, ShellMetrics,
    apply_dpi_suggested_rect, dock_showcase_rect, physical_from_dip, premultiply_srgb,
    rounded_content_hit, topbar_rect,
};

#[test]
fn converts_dip_to_physical_when_monitor_uses_fractional_scale() {
    let monitor_scale = Dpi::from_raw(144);
    let converted_pixels = physical_from_dip(52.0, monitor_scale);
    assert_eq!(converted_pixels, 78);
}

#[test]
fn converts_dip_to_physical_at_supported_dpi_scales() {
    assert_eq!(physical_from_dip(32.0, Dpi::from_raw(96)), 32);
    assert_eq!(physical_from_dip(32.0, Dpi::from_raw(120)), 40);
    assert_eq!(physical_from_dip(32.0, Dpi::from_raw(144)), 48);
    assert_eq!(physical_from_dip(32.0, Dpi::from_raw(192)), 64);
}

#[test]
fn places_shells_in_signed_secondary_work_area() {
    let work = PhysicalRect::new(-1920, -1040, 1920, 1040);
    let metrics = ShellMetrics::default();
    assert_eq!(
        topbar_rect(work, Dpi::from_raw(96), metrics),
        PhysicalRect::new(-1910, -1032, 1900, 32)
    );
    assert_eq!(
        dock_showcase_rect(work, Dpi::from_raw(96), metrics),
        PhysicalRect::new(-1247, -84, 574, 72)
    );
}

#[test]
fn places_shells_in_offset_high_dpi_work_area() {
    let work = PhysicalRect::new(3840, 100, 1600, 900);
    let metrics = ShellMetrics::default();
    assert_eq!(
        topbar_rect(work, Dpi::from_raw(192), metrics),
        PhysicalRect::new(3859, 116, 1562, 64)
    );
    assert_eq!(
        dock_showcase_rect(work, Dpi::from_raw(192), metrics),
        PhysicalRect::new(4065, 832, 1149, 144)
    );
}

#[test]
fn places_topbar_and_dock_inside_primary_work_area_when_scaled() {
    let primary_work_area = PhysicalRect::new(0, 0, 1920, 1040);
    let monitor_scale = Dpi::from_raw(120);
    let metrics = ShellMetrics::default();
    let topbar = topbar_rect(primary_work_area, monitor_scale, metrics);
    let dock = dock_showcase_rect(primary_work_area, monitor_scale, metrics);
    assert_eq!(topbar, PhysicalRect::new(12, 10, 1896, 40));
    assert_eq!(dock, PhysicalRect::new(601, 935, 718, 90));
}

#[test]
fn applies_wm_dpichanged_suggested_rect_without_rescaling_twice() {
    let current_rect = PhysicalRect::new(50, 50, 500, 64);
    let windows_suggested_rect = PhysicalRect::new(64, 80, 625, 80);
    let applied_rect = apply_dpi_suggested_rect(
        current_rect,
        windows_suggested_rect,
        Dpi::from_raw(96),
        Dpi::from_raw(120),
    );
    assert_eq!(applied_rect, windows_suggested_rect);
}

#[test]
fn rejects_points_outside_rounded_content_mask_when_window_is_transparent() {
    let shell_bounds = DipRect::new(0.0, 0.0, 120.0, 48.0);
    let transparent_corner = rounded_content_hit(shell_bounds, 14.0, DipPoint::new(2.0, 2.0));
    let visible_body = rounded_content_hit(shell_bounds, 14.0, DipPoint::new(60.0, 24.0));
    assert!(!transparent_corner);
    assert!(visible_body);
}

#[test]
fn lifecycle_rebuilds_device_resources_after_device_removed() {
    let ready_lifecycle = DeviceLifecycle::ready();
    let rebuilding_lifecycle = ready_lifecycle.transition(DeviceEvent::DeviceRemoved);
    let recovered_lifecycle = rebuilding_lifecycle.transition(DeviceEvent::ResourcesRebuilt);
    assert_eq!(rebuilding_lifecycle, DeviceLifecycle::Rebuilding);
    assert_eq!(recovered_lifecycle, DeviceLifecycle::Ready);
}

#[test]
fn premultiplies_tokens_before_rendering_to_composition_swapchain() {
    let surface_token = Rgba8::new(0x11, 0x15, 0x1B, 0xEF);
    let premultiplied_token = premultiply_srgb(surface_token);
    assert_eq!(premultiplied_token, Rgba8::new(0x10, 0x14, 0x19, 0xEF));
}
