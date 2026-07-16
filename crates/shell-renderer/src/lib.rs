#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod color;
mod context_menu_layout;
mod context_menu_scene;
mod dock_inset_raster;
mod dock_item_visual;
mod dock_layout;
mod dock_scene;
mod geometry;
#[allow(
    dead_code,
    reason = "device lifecycle is an internal recovery seam, not part of the renderer facade"
)]
mod lifecycle;
mod popover_layout;
mod popover_scene;
mod settings_scene;
mod showcase_model;
mod topbar_layout;
mod topbar_scene;
mod window_preview_layout;
mod window_preview_scene;

#[cfg(test)]
mod integration_tests;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 COM calls are isolated in this module")]
pub mod native;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Direct2D showcase drawing calls are isolated in this module"
)]
mod native_showcase;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Direct2D drawing primitives are isolated in this module"
)]
mod native_showcase_primitives;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "DirectWrite formats and Direct2D brushes are isolated here"
)]
mod native_showcase_resources;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "DXGI presentation and device-loss diagnostics are isolated here"
)]
mod native_present;

#[cfg(windows)]
#[allow(unsafe_code, reason = "D3D11 device creation is isolated here")]
mod native_device;

#[cfg(windows)]
mod native_showcase_dock;

#[cfg(windows)]
mod native_showcase_context_menu;

#[cfg(windows)]
#[cfg(windows)]
mod native_showcase_dock_states;
#[cfg(windows)]
mod native_showcase_material;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Windows Shell and WIC icon conversion is isolated here"
)]
mod native_icons;

#[cfg(windows)]
mod native_showcase_topbar;

#[cfg(windows)]
mod native_showcase_popover;

#[cfg(windows)]
mod native_showcase_preview;

#[cfg(windows)]
mod native_showcase_settings;

pub(crate) use color::Rgba8;
#[cfg(test)]
pub(crate) use color::premultiply_srgb;
pub use context_menu_layout::{
    ContextMenuLaidOutRow, ContextMenuLayout, ContextMenuSeparator, context_menu_anchor_rect,
    context_menu_height_for_entries, context_menu_surface_width, layout_context_menu_scene,
};
pub use context_menu_scene::{ContextMenuEntry, ContextMenuScene};
pub use dock_item_visual::{DockIcon, DockItemVisual, DockItemVisualKind, RunningIndicator};
pub use dock_layout::DockLaidOutItem;
pub(crate) use dock_layout::dock_material_bounds;
pub use dock_layout::{DockLayout, dock_scene_max_width, layout_dock_scene};
#[cfg(test)]
pub(crate) use dock_layout::{WindowPreviewRenderKind, dock_scene_width, layout_window_previews};
pub use dock_scene::{
    DockAlignment, DockLayoutConfig, DockScene, PreviewUnavailableReason, WindowPreviewCapture,
    WindowPreviewVisual,
};
#[cfg(test)]
pub(crate) use geometry::apply_dpi_suggested_rect;
pub(crate) use geometry::logical_surface_rect;
pub use geometry::{
    DipPoint, DipRect, Dpi, PhysicalRect, ShellMetrics, dock_showcase_rect, physical_from_dip,
    rounded_content_hit, topbar_height_for_text_scale, topbar_rect,
};
pub use popover_layout::{
    PopoverLaidOutRow, PopoverLayout, layout_popover_scene, popover_anchor_rect,
    popover_anchor_rect_with_height, popover_height_for_rows,
};
pub use popover_scene::{PopoverContentState, PopoverRow, PopoverScene};
pub use settings_scene::{SettingsRow, SettingsScene};
pub(crate) use showcase_model::{DockInsetShadow, ShowcaseTokens, dock_inset_shadows};
#[cfg(test)]
pub(crate) use showcase_model::{ShowcasePrimitive, ShowcaseState, showcase_primitives};
pub use topbar_layout::{TopbarLaidOutItem, TopbarLayout, TopbarOverflow, layout_topbar_scene};
pub use topbar_scene::{TopbarDensity, TopbarModuleStatus, TopbarModuleVisual, TopbarScene};
pub(crate) use window_preview_layout::WINDOW_PREVIEW_CARD_RADIUS;
pub use window_preview_layout::{
    PreviewCardLayout, PreviewPanelSize, PreviewPlacementInput, WINDOW_PREVIEW_THUMBNAIL_RADIUS,
    WindowPreviewPanelLayout, layout_window_preview, place_window_preview, preview_panel_size,
    preview_panel_size_for_scene,
};
pub use window_preview_scene::{PreviewCardVisual, PreviewSourceSize, WindowPreviewScene};

#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-renderer"
}
