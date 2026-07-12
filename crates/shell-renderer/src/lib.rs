#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod color;
mod dock_layout;
mod dock_scene;
mod geometry;
mod lifecycle;
mod popover_layout;
mod popover_scene;
mod showcase_model;
mod topbar_layout;
mod topbar_scene;

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
    reason = "DXGI presentation and device-loss diagnostics are isolated here"
)]
mod native_present;

#[cfg(windows)]
#[allow(unsafe_code, reason = "D3D11 device creation is isolated here")]
mod native_device;

#[cfg(windows)]
mod native_showcase_dock;

#[cfg(windows)]
mod native_showcase_topbar;

#[cfg(windows)]
mod native_showcase_popover;

pub use color::{Rgba8, premultiply_srgb};
pub use dock_layout::{
    DockLaidOutItem, DockLayout, WindowPreviewLayout, WindowPreviewRenderKind, layout_dock_scene,
    layout_window_previews,
};
pub use dock_scene::{
    DockAlignment, DockItemVisual, DockItemVisualKind, DockLayoutConfig, DockScene,
    PreviewUnavailableReason, RunningIndicator, WindowPreviewCapture, WindowPreviewVisual,
};
pub use geometry::{
    DipPoint, DipRect, Dpi, PhysicalRect, ShellMetrics, apply_dpi_suggested_rect,
    dock_showcase_rect, physical_from_dip, rounded_content_hit, topbar_rect,
};
pub use lifecycle::{DeviceEvent, DeviceLifecycle};
pub use popover_layout::{
    PopoverLaidOutRow, PopoverLayout, layout_popover_scene, popover_anchor_rect,
};
pub use popover_scene::{PopoverContentState, PopoverRow, PopoverScene};
pub use showcase_model::{
    ShowcaseItem, ShowcasePrimitive, ShowcaseState, ShowcaseTokens, showcase_primitives,
};
pub use topbar_layout::{TopbarLaidOutItem, TopbarLayout, TopbarOverflow, layout_topbar_scene};
pub use topbar_scene::{TopbarDensity, TopbarModuleStatus, TopbarModuleVisual, TopbarScene};

#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-renderer"
}
