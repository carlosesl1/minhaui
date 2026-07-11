#[cfg(windows)]
use shell_renderer::native::{DeviceLossKind, PresentOutcome, classify_present_hresult};
use shell_renderer::{
    DipPoint, DipRect, Dpi, PhysicalRect, Rgba8, ShellMetrics, ShowcasePrimitive, ShowcaseState,
    ShowcaseTokens, apply_dpi_suggested_rect, dock_showcase_rect, physical_from_dip,
    premultiply_srgb, rounded_content_hit, showcase_primitives, topbar_rect,
};
#[cfg(windows)]
use windows::Win32::Foundation::D2DERR_RECREATE_TARGET;
#[cfg(windows)]
use windows::Win32::Graphics::Dxgi::{DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET};
#[cfg(windows)]
use windows::core::HRESULT;

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
        PhysicalRect::new(-1480, -192, 1040, 180)
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
        PhysicalRect::new(3600, 616, 2080, 360)
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
    assert_eq!(dock, PhysicalRect::new(310, 800, 1300, 225));
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
fn showcase_tokens_match_design_contract() {
    let tokens = ShowcaseTokens::obsidian_glass();
    assert_eq!(tokens.dock_radius, 18.0);
    assert_eq!(tokens.surface_base, Rgba8::new(0x11, 0x15, 0x1B, 0xEF));
    assert_eq!(tokens.accent, Rgba8::new(0x4C, 0x9A, 0xFF, 0xFF));
    assert_eq!(tokens.error, Rgba8::new(0xFF, 0x73, 0x73, 0xFF));
}

#[test]
fn showcase_model_covers_required_states_and_primitives() {
    let primitives = showcase_primitives();
    for state in [
        ShowcaseState::Rest,
        ShowcaseState::Hover,
        ShowcaseState::Pressed,
        ShowcaseState::Active,
        ShowcaseState::FocusVisible,
        ShowcaseState::Unavailable,
        ShowcaseState::Error,
    ] {
        assert!(
            primitives
                .iter()
                .any(|primitive| primitive.state() == state)
        );
    }
    for expected in [
        ShowcasePrimitive::Button,
        ShowcasePrimitive::Slider,
        ShowcasePrimitive::DeviceRow,
        ShowcasePrimitive::CalendarCell,
        ShowcasePrimitive::Popover,
    ] {
        assert!(
            primitives
                .iter()
                .any(|primitive| primitive.kind() == expected)
        );
    }
}

#[test]
fn premultiplies_tokens_before_rendering_to_composition_swapchain() {
    let surface_token = Rgba8::new(0x11, 0x15, 0x1B, 0xEF);
    let premultiplied_token = premultiply_srgb(surface_token);
    assert_eq!(premultiplied_token, Rgba8::new(0x10, 0x14, 0x19, 0xEF));
}

#[cfg(windows)]
#[test]
fn classifies_present_hresult_for_success_and_recoverable_loss() {
    assert_eq!(
        classify_present_hresult(HRESULT(0)),
        PresentOutcome::Presented
    );
    assert_eq!(
        classify_present_hresult(DXGI_ERROR_DEVICE_REMOVED),
        PresentOutcome::DeviceLost(DeviceLossKind::Removed)
    );
    assert_eq!(
        classify_present_hresult(DXGI_ERROR_DEVICE_RESET),
        PresentOutcome::DeviceLost(DeviceLossKind::Reset)
    );
    assert_eq!(
        classify_present_hresult(D2DERR_RECREATE_TARGET),
        PresentOutcome::DeviceLost(DeviceLossKind::RecreateTarget)
    );
}
