use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::Win32::Graphics::Dxgi::DXGI_SWAP_CHAIN_FLAG;
use windows::core::Result;

use super::{
    CompositionRenderer, PresentOutcome, ShellScenes, ShowcaseRole, SurfaceMetrics, WindowSurface,
};
use crate::ShowcaseTokens;
use crate::logical_surface_rect;
use crate::native_showcase_resources::create_dock_inset_bitmap;

impl CompositionRenderer {
    pub fn resize_surface(
        &self,
        surface: &mut WindowSurface,
        metrics: SurfaceMetrics,
        role: ShowcaseRole,
        scenes: ShellScenes<'_>,
    ) -> Result<PresentOutcome> {
        let SurfaceMetrics { width, height, dpi } = metrics;
        // SAFETY: Category 8 (FFI boundary). Releasing the current D2D target
        // drops its back-buffer reference before DXGI resizes the live chain.
        unsafe { self.d2d_context.SetTarget(None) };
        surface.back_buffers.borrow_mut().clear();
        // SAFETY: Category 8 (FFI boundary). The swap chain is live, dimensions
        // are non-zero HWND metrics, and existing flags/buffer count are retained.
        unsafe {
            surface.swap_chain.ResizeBuffers(
                0,
                width,
                height,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
        }
        surface.width = width;
        surface.height = height;
        surface.dpi = dpi;
        let logical_surface = logical_surface_rect(width, height, dpi);
        surface.dock_inset = if role == ShowcaseRole::Dock && !self.solid_material {
            Some(create_dock_inset_bitmap(
                &self.d2d_context,
                logical_surface.width,
                logical_surface.height,
                ShowcaseTokens::obsidian_glass().dock_radius,
            )?)
        } else {
            None
        };
        self.redraw_surface(surface, role, scenes)
    }
}
