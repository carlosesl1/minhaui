#![deny(unsafe_code)]

use windows::core::Result;

use crate::FullscreenPolicy;
use crate::win32_actions::apply_dock_actions;
use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows};
use crate::win32_window::OwnedWindow;
use crate::win32_windowing::{monitor_placement_inputs, window_monitor_id};

pub(super) struct FullscreenSyncWindows<'a> {
    pub(super) topbar: &'a OwnedWindow,
    pub(super) dock: &'a mut OwnedWindow,
    pub(super) popover: &'a OwnedWindow,
    pub(super) preview: &'a OwnedWindow,
    pub(super) settings: &'a OwnedWindow,
}

impl FullscreenSyncWindows<'_> {
    fn surfaces(&self) -> SurfaceWindows<'_> {
        SurfaceWindows {
            topbar: self.topbar,
            dock: &*self.dock,
            popover: self.popover,
            preview: self.preview,
            settings: self.settings,
        }
    }
}

impl RuntimeSurfaces {
    pub(super) fn sync_fullscreen_suppression(
        &mut self,
        observed: &[crate::ObservedWindow],
        windows: FullscreenSyncWindows<'_>,
    ) -> Result<()> {
        let fullscreen = observed
            .iter()
            .filter_map(crate::ObservedWindow::fullscreen)
            .collect::<Vec<_>>();
        let monitors = monitor_placement_inputs()?;
        let policy = FullscreenPolicy::hide_for_fullscreen(&fullscreen, &monitors);
        let suppressed = policy.suppresses(window_monitor_id(windows.dock.hwnd));
        if suppressed == self.fullscreen_suppressed {
            return Ok(());
        }
        let actions = self
            .dock_controller
            .set_fullscreen_autohide(suppressed)
            .map_err(|error| {
                windows::core::Error::new(
                    windows::core::HRESULT(0x8007_0057_u32 as i32),
                    error.to_string(),
                )
            })?;
        if !apply_dock_actions(&actions, self.dock_controller.state())? {
            return Ok(());
        }
        self.fullscreen_suppressed = suppressed;
        let update = self.begin_dock_visibility(windows.dock);
        self.complete_surface_update(update, windows.surfaces())?;
        self.sync_dock_animation(windows.dock)?;
        Ok(())
    }
}
