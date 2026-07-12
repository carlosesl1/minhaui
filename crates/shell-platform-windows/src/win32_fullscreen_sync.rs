#![deny(unsafe_code)]

use windows::core::Result;

use crate::FullscreenPolicy;
use crate::win32_owner::RuntimeSurfaces;
use crate::win32_window::OwnedWindow;
use crate::win32_windowing::{monitor_placement_inputs, window_monitor_id, window_work_area};

impl RuntimeSurfaces {
    pub(super) fn sync_fullscreen_suppression(
        &mut self,
        observed: &[crate::ObservedWindow],
        topbar: &OwnedWindow,
        dock: &mut OwnedWindow,
        popover: &OwnedWindow,
    ) -> Result<()> {
        let fullscreen = observed
            .iter()
            .filter_map(crate::ObservedWindow::fullscreen)
            .collect::<Vec<_>>();
        let monitors = monitor_placement_inputs()?;
        let policy = FullscreenPolicy::hide_for_fullscreen(&fullscreen, &monitors);
        let suppressed = policy.suppresses(window_monitor_id(dock.hwnd));
        if suppressed == self.fullscreen_suppressed {
            return Ok(());
        }
        self.fullscreen_suppressed = suppressed;
        let work = window_work_area(dock.hwnd)?;
        dock.apply_dock_visibility(
            work,
            self.dock_controller.config().with_autohide(true),
            suppressed,
        )?;
        let _ = (topbar, popover);
        Ok(())
    }
}
