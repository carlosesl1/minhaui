#[cfg(windows)]
use crate::native::{DeviceLossKind, PresentOutcome, classify_present_hresult};
use crate::{
    DipPoint, DipRect, DockInsetShadow, Dpi, PhysicalRect, Rgba8, ShellMetrics, ShowcasePrimitive,
    ShowcaseState, ShowcaseTokens, apply_dpi_suggested_rect, dock_inset_shadows,
    dock_showcase_rect, logical_surface_rect, physical_from_dip, premultiply_srgb,
    rounded_content_hit, showcase_primitives, topbar_height_for_text_scale, topbar_rect,
};
#[cfg(windows)]
use windows::Win32::Foundation::D2DERR_RECREATE_TARGET;
#[cfg(windows)]
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_ERROR_WAS_STILL_DRAWING,
};
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
fn topbar_height_preserves_vertical_insets_at_large_text_scale() {
    assert_eq!(topbar_height_for_text_scale(1.0), 32.0);
    assert_eq!(topbar_height_for_text_scale(2.0), 56.0);
    assert_eq!(topbar_height_for_text_scale(2.5), 68.0);
}

#[test]
fn converts_physical_render_targets_back_to_logical_dips() {
    let logical = logical_surface_rect(1520, 110, Dpi::from_raw(192));
    assert_eq!(logical, DipRect::new(0.0, 0.0, 760.0, 55.0));
}

#[test]
fn places_shells_in_signed_secondary_work_area() {
    let work = PhysicalRect::new(-1920, -1040, 1920, 1040);
    let metrics = ShellMetrics::default();
    assert_eq!(
        topbar_rect(work, Dpi::from_raw(96), metrics),
        PhysicalRect::new(-1920, -1040, 1920, 32)
    );
    assert_eq!(
        dock_showcase_rect(work, Dpi::from_raw(96), metrics),
        PhysicalRect::new(-1340, -59, 760, 55)
    );
}

#[test]
fn places_shells_in_offset_high_dpi_work_area() {
    let work = PhysicalRect::new(3840, 100, 1600, 900);
    let metrics = ShellMetrics::default();
    assert_eq!(
        topbar_rect(work, Dpi::from_raw(192), metrics),
        PhysicalRect::new(3840, 100, 1600, 64)
    );
    assert_eq!(
        dock_showcase_rect(work, Dpi::from_raw(192), metrics),
        PhysicalRect::new(3880, 882, 1520, 110)
    );
}

#[test]
fn places_topbar_and_dock_inside_primary_work_area_when_scaled() {
    let primary_work_area = PhysicalRect::new(0, 0, 1920, 1040);
    let monitor_scale = Dpi::from_raw(120);
    let metrics = ShellMetrics::default();
    let topbar = topbar_rect(primary_work_area, monitor_scale, metrics);
    let dock = dock_showcase_rect(primary_work_area, monitor_scale, metrics);
    assert_eq!(topbar, PhysicalRect::new(0, 0, 1920, 40));
    assert_eq!(dock, PhysicalRect::new(485, 966, 950, 69));
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
    assert_eq!(tokens.dock_radius, 15.0);
    assert_eq!(tokens.surface_base, Rgba8::new(0x20, 0x22, 0x26, 0xD2));
    assert_eq!(tokens.dock_luminance, Rgba8::new(0x4D, 0x4D, 0x4D, 0x4D));
    assert_eq!(tokens.dock_veil, Rgba8::new(0x1A, 0x1A, 0x1A, 0x1A));
    assert_eq!(tokens.dock_reflection, Rgba8::new(0xFF, 0xFF, 0xFF, 0x14));
    assert_eq!(tokens.dock_depth, Rgba8::new(0x16, 0x16, 0x16, 0x18));
    assert_eq!(tokens.dock_inset_edge, Rgba8::new(0x80, 0x80, 0x80, 0xFF));
    assert_eq!(tokens.accent, Rgba8::new(0x4C, 0x9A, 0xFF, 0xFF));
    assert_eq!(tokens.rim_outer, Rgba8::new(0x00, 0x00, 0x00, 0x40));
    assert_eq!(tokens.rim_inner, Rgba8::new(0xFF, 0xFF, 0xFF, 0x24));
    assert_eq!(tokens.popover_radius, 12.0);
    assert_eq!(tokens.warning, Rgba8::new(0xF2, 0xB8, 0x4B, 0xFF));
    assert_eq!(tokens.error, Rgba8::new(0xFF, 0x73, 0x73, 0xFF));
}

#[test]
fn dock_inset_shadows_match_the_supplied_glass_contract() {
    assert_eq!(
        dock_inset_shadows(),
        [
            DockInsetShadow::new(2.5, 1.5, -2.5, Rgba8::new(0x80, 0x80, 0x80, 0xFF)),
            DockInsetShadow::new(-2.5, 1.5, -2.5, Rgba8::new(0x80, 0x80, 0x80, 0xFF)),
            DockInsetShadow::new(16.0, 16.0, -16.0, Rgba8::new(0x16, 0x16, 0x16, 0xFF)),
            DockInsetShadow::new(-16.0, 16.0, -16.0, Rgba8::new(0x16, 0x16, 0x16, 0xFF)),
        ]
    );
}

#[test]
fn solid_fallback_uses_opaque_shell_bodies() {
    let tokens = ShowcaseTokens::solid_fallback();
    assert_eq!(tokens.dock_luminance.a, 0xFF);
    assert_eq!(tokens.topbar_tint.a, 0xFF);
    assert_eq!(tokens.surface_base.a, 0xFF);
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
    let surface_token = Rgba8::new(0x11, 0x15, 0x1B, 0xE2);
    let premultiplied_token = premultiply_srgb(surface_token);
    assert_eq!(premultiplied_token, Rgba8::new(0x0F, 0x13, 0x18, 0xE2));
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

#[cfg(windows)]
#[test]
fn treats_present_backpressure_as_a_skipped_frame() {
    assert_eq!(
        classify_present_hresult(DXGI_ERROR_WAS_STILL_DRAWING),
        PresentOutcome::FrameSkipped
    );
}
