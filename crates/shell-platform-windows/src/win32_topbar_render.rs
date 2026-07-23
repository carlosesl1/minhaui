#![deny(unsafe_code)]

use shell_renderer::native::{ShellScenes, ShowcaseRole, SurfaceMetrics};
use windows::core::Result;

use crate::TopbarSnapshot;
use crate::win32_dock_render::dip_surface;
use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows};
use crate::win32_surface_runtime::{SurfaceFrame, SurfaceSizeChange, SurfaceTarget, present_frame};
use crate::win32_window::OwnedWindow;

impl RuntimeSurfaces {
    pub(super) fn apply_topbar_snapshot(
        &mut self,
        snapshot: &TopbarSnapshot,
        windows: SurfaceWindows<'_>,
    ) -> Result<()> {
        if self.topbar_controller.snapshot() == snapshot {
            return Ok(());
        }
        self.topbar_controller.update_snapshot(snapshot.clone());
        self.redraw_topbar(
            windows.topbar,
            windows.dock,
            windows.popover,
            windows.preview,
            windows.settings,
        )
    }

    pub(super) fn redraw_topbar(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        preview: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        let window_size = topbar.surface_physical_size();
        let windows = SurfaceWindows {
            topbar,
            dock,
            popover,
            preview,
            settings,
        };
        self.topbar_controller.update_surface(dip_surface(topbar));
        let scene = self.topbar_controller.scene();
        let scenes = ShellScenes {
            topbar: Some(&scene),
            dock: None,
            popover: None,
            context_menu: None,
            settings: None,
            preview: None,
        };
        let update = present_frame(
            &mut self.surface_runtime,
            SurfaceFrame {
                target: SurfaceTarget {
                    hwnd: topbar.hwnd,
                    role: ShowcaseRole::Topbar,
                    metrics: SurfaceMetrics::new(window_size.0, window_size.1, topbar.dpi()),
                },
                scenes,
            },
            SurfaceSizeChange::Resize,
        );
        self.complete_surface_update(update, windows)
    }
}

#[cfg(test)]
mod tests {
    use crate::win32_surface_runtime::{SurfaceRenderDecision, surface_render_decision};
    use shell_renderer::Dpi;
    use shell_renderer::native::SurfaceMetrics;

    #[test]
    fn topbar_surface_decision_distinguishes_missing_unchanged_and_resized() {
        let desired = SurfaceMetrics::new(387, 55, Dpi::from_raw(144));
        assert_eq!(
            surface_render_decision(None, desired),
            SurfaceRenderDecision::RebuildAll
        );
        assert_eq!(
            surface_render_decision(Some(desired), desired),
            SurfaceRenderDecision::Redraw
        );
        assert_eq!(
            surface_render_decision(
                Some(SurfaceMetrics::new(328, 55, Dpi::from_raw(144))),
                desired,
            ),
            SurfaceRenderDecision::Resize
        );
    }
}
