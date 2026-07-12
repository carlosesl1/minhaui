#![deny(unsafe_code)]

use shell_renderer::native::{ShellScenes, ShowcaseRole};
use windows::core::Result;

use crate::win32_dock_render::handle_present;
use crate::win32_owner::RuntimeSurfaces;
use crate::win32_window::OwnedWindow;
use crate::{QueuedPopoverAction, QueuedTopbarAction};

impl RuntimeSurfaces {
    pub(super) fn apply_topbar_actions(
        &mut self,
        actions: &[QueuedTopbarAction],
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &mut OwnedWindow,
    ) -> Result<()> {
        for action in actions {
            match action {
                QueuedTopbarAction::OpenPopover(kind) => {
                    self.popover_controller
                        .open(*kind, &self.popover_provider)
                        .map_err(|error| {
                            windows::core::Error::new(invalid_arg(), error.to_string())
                        })?;
                    let work = crate::win32_windowing::window_work_area(topbar.hwnd)?;
                    popover.place_popover(work, topbar.rect)?;
                    self.redraw_popover(topbar, dock, popover)?;
                }
                QueuedTopbarAction::PollDeferred | QueuedTopbarAction::RedrawTopbar => {}
            }
        }
        Ok(())
    }

    pub(super) fn apply_popover_actions(
        &mut self,
        actions: &[QueuedPopoverAction],
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
    ) -> Result<()> {
        for action in actions {
            match action {
                QueuedPopoverAction::Redraw
                | QueuedPopoverAction::RequestConfirmation(_)
                | QueuedPopoverAction::TypedIntent(_) => {
                    self.redraw_popover(topbar, dock, popover)?;
                }
                QueuedPopoverAction::Dismiss => popover.hide(),
            }
        }
        Ok(())
    }

    fn redraw_popover(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
    ) -> Result<()> {
        let scene = self.popover_controller.scene();
        let outcome = match (self.renderer.as_ref(), self.popover.as_ref()) {
            (Some(renderer), Some(surface)) => renderer.redraw_surface(
                surface,
                ShowcaseRole::Popover,
                ShellScenes {
                    topbar: None,
                    dock: None,
                    popover: scene.as_ref(),
                },
            ),
            (None, _) | (_, None) => return self.rebuild(topbar, dock, popover),
        };
        handle_present(outcome, self, topbar, dock, popover, "popover")
    }
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}
