#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod color;
mod geometry;
mod lifecycle;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 COM calls are isolated in this module")]
pub mod native;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Direct2D showcase drawing calls are isolated in this module"
)]
mod native_showcase;

pub use color::{Rgba8, premultiply_srgb};
pub use geometry::{
    DipPoint, DipRect, Dpi, PhysicalRect, ShellMetrics, apply_dpi_suggested_rect,
    dock_showcase_rect, physical_from_dip, rounded_content_hit, topbar_rect,
};
pub use lifecycle::{DeviceEvent, DeviceLifecycle};

#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-renderer"
}
