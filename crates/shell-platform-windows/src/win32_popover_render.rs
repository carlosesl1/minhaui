#![deny(unsafe_code)]

use shell_renderer::native::{ShellScenes, ShowcaseRole};
use windows::core::Result;

use crate::win32_dock_render::handle_present;
use crate::win32_owner::RuntimeSurfaces;
use crate::win32_window::OwnedWindow;
use crate::{PopoverAction, QueuedPopoverAction, QueuedTopbarAction};

impl RuntimeSurfaces {
    pub(super) fn apply_topbar_actions(
        &mut self,
        actions: &[QueuedTopbarAction],
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &mut OwnedWindow,
        settings: &mut OwnedWindow,
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
                    self.redraw_popover(topbar, dock, popover, settings)?;
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
        settings: &mut OwnedWindow,
    ) -> Result<()> {
        for action in actions {
            match action {
                QueuedPopoverAction::Redraw | QueuedPopoverAction::RequestConfirmation(_) => {
                    self.redraw_popover(topbar, dock, popover, settings)?;
                }
                QueuedPopoverAction::TypedIntent(PopoverAction::OpenSettings) => {
                    let work = crate::win32_windowing::window_work_area(settings.hwnd)?;
                    settings.show_settings(work)?;
                    self.redraw_settings(topbar, dock, popover, settings)?;
                }
                QueuedPopoverAction::TypedIntent(_) => {
                    self.redraw_popover(topbar, dock, popover, settings)?;
                }
                QueuedPopoverAction::Dismiss => popover.hide(),
            }
        }
        Ok(())
    }

    pub(super) fn redraw_settings(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        let scene = self.settings_controller.scene();
        let outcome = match (self.renderer.as_ref(), self.settings.as_ref()) {
            (Some(renderer), Some(surface)) => renderer.redraw_surface(
                surface,
                ShowcaseRole::Settings,
                ShellScenes {
                    topbar: None,
                    dock: None,
                    popover: None,
                    settings: Some(&scene),
                },
            ),
            (None, _) | (_, None) => return self.rebuild(topbar, dock, popover, settings),
        };
        handle_present(outcome, self, topbar, dock, popover, settings, "settings")
    }

    fn redraw_popover(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        settings: &OwnedWindow,
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
                    settings: None,
                },
            ),
            (None, _) | (_, None) => return self.rebuild(topbar, dock, popover, settings),
        };
        handle_present(outcome, self, topbar, dock, popover, settings, "popover")
    }
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}
