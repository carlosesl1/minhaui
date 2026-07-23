#![deny(unsafe_code)]

use shell_core::Popover;
use shell_renderer::native::{ShellScenes, ShowcaseRole, SurfaceMetrics};
use shell_renderer::{
    PhysicalRect, context_menu_height_for_entries, physical_from_dip, popover_height_for_rows,
};
use windows::core::Result;

use crate::win32_actions::apply_dock_actions;
use crate::win32_dock_render::{DockRenderBaseline, DockRenderWindows};
use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows};
use crate::win32_surface_runtime::{SurfaceFrame, SurfaceSizeChange, SurfaceTarget, present_frame};
use crate::win32_window::OwnedWindow;
use crate::{
    PopoverAction, QueuedContextMenuAction, QueuedPopoverAction, QueuedTopbarAction,
    TopbarOverlayAnchor,
};

const POPOVER_SURFACE_ROLE: ShowcaseRole = ShowcaseRole::Popover;

impl RuntimeSurfaces {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_topbar_actions(
        &mut self,
        actions: &[QueuedTopbarAction],
        topbar: &OwnedWindow,
        dock: &mut OwnedWindow,
        popover: &mut OwnedWindow,
        preview: &OwnedWindow,
        settings: &mut OwnedWindow,
    ) -> Result<()> {
        for action in actions {
            match action {
                QueuedTopbarAction::OpenPopover {
                    popover: kind,
                    anchor,
                } => {
                    self.dismiss_context_menu(topbar, dock, popover, preview, settings)?;
                    popover.hide();
                    popover.set_backdrop_enabled(true);
                    if self.popover_controller.active_kind() == Some(*kind) {
                        self.popover_controller.dismiss();
                        popover.hide();
                        continue;
                    }
                    settings.hide();
                    self.active_topbar_anchor = Some(*anchor);
                    let background_generation = if *kind == Popover::BackgroundApps {
                        Some(self.popover_controller.begin_loading(*kind))
                    } else {
                        self.popover_controller
                            .open_with_snapshot(
                                *kind,
                                &self.popover_provider,
                                self.topbar_controller.snapshot(),
                            )
                            .map_err(|error| {
                                windows::core::Error::new(invalid_arg(), error.to_string())
                            })?;
                        None
                    };
                    let work = crate::win32_windowing::window_work_area(topbar.hwnd)?;
                    let row_count = self
                        .popover_controller
                        .scene()
                        .map_or(0, |scene| scene.rows().len());
                    popover.place_popover(
                        work,
                        self.topbar_anchor_rect(topbar, *anchor),
                        popover_height_for_rows(row_count),
                    )?;
                    self.redraw_popover(topbar, dock, popover, preview, settings)?;
                    self.animate_surface_entrance(
                        POPOVER_SURFACE_ROLE,
                        SurfaceWindows {
                            topbar,
                            dock,
                            popover,
                            preview,
                            settings,
                        },
                    )?;
                    popover.show_activating();
                    if let Some(generation) = background_generation {
                        crate::background_apps_worker::request_background_apps(
                            generation,
                            crate::win32_event_queue::native_window_id(topbar.hwnd),
                        );
                    }
                }
                QueuedTopbarAction::OpenSearch => {
                    self.popover_controller.dismiss();
                    popover.hide();
                    settings.hide();
                    record_system_action(
                        "topbar.search",
                        crate::win32_system_actions::open_search(),
                    );
                }
                QueuedTopbarAction::RedrawTopbar => {}
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn open_dock_context_menu(
        &mut self,
        point: shell_renderer::DipPoint,
        topbar: &OwnedWindow,
        dock: &mut OwnedWindow,
        popover: &mut OwnedWindow,
        preview: &OwnedWindow,
        settings: &mut OwnedWindow,
    ) -> Result<()> {
        self.dismiss_context_menu(topbar, dock, popover, preview, settings)?;
        popover.hide();
        self.popover_controller.dismiss();
        popover.set_backdrop_enabled(false);
        settings.hide();
        let before = self.dock_controller.state().clone();
        let visual_before = self.dock_controller.visual_generation();
        let hold_actions = self
            .dock_controller
            .hold_revealed_for_overlay()
            .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
        self.render_dock_change(
            DockRenderBaseline::new(&before, visual_before),
            &hold_actions,
            DockRenderWindows {
                topbar,
                dock,
                popover,
                preview,
                settings,
            },
        )?;
        self.context_menu_controller
            .open(point, self.dock_controller.context_menu_items(point));
        let work = crate::win32_windowing::window_work_area(dock.hwnd)?;
        self.place_active_overlay(work, topbar, dock, popover)?;
        self.redraw_popover(topbar, dock, popover, preview, settings)?;
        self.animate_surface_entrance(
            POPOVER_SURFACE_ROLE,
            SurfaceWindows {
                topbar,
                dock,
                popover,
                preview,
                settings,
            },
        )?;
        popover.show_activating();
        Ok(())
    }

    pub(super) fn dismiss_transient_overlays(
        &mut self,
        topbar: &OwnedWindow,
        dock: &mut OwnedWindow,
        popover: &OwnedWindow,
        preview: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        self.popover_controller.dismiss();
        self.dismiss_context_menu(topbar, dock, popover, preview, settings)?;
        popover.hide();
        Ok(())
    }

    fn dismiss_context_menu(
        &mut self,
        topbar: &OwnedWindow,
        dock: &mut OwnedWindow,
        popover: &OwnedWindow,
        preview: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        if !self.context_menu_controller.is_active() && !self.dock_controller.overlay_hold_active()
        {
            return Ok(());
        }
        let before = self.dock_controller.state().clone();
        let visual_before = self.dock_controller.visual_generation();
        self.context_menu_controller.dismiss();
        popover.hide();
        let dock_actions = self
            .dock_controller
            .release_overlay_hold()
            .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
        self.render_dock_change(
            DockRenderBaseline::new(&before, visual_before),
            &dock_actions,
            DockRenderWindows {
                topbar,
                dock,
                popover,
                preview,
                settings,
            },
        )
    }

    pub(super) fn place_active_overlay(
        &self,
        work: PhysicalRect,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &mut OwnedWindow,
    ) -> Result<bool> {
        if let Some(scene) = self.context_menu_controller.scene()
            && let Some(point) = self.context_menu_controller.target_point()
        {
            let anchor = PhysicalRect::new(
                dock.rect.x + physical_from_dip(point.x, dock.dpi()),
                dock.rect.y + physical_from_dip(point.y, dock.dpi()),
                1,
                1,
            );
            popover.place_context_menu(
                work,
                anchor,
                context_menu_height_for_entries(scene.entries()),
            )?;
            return Ok(true);
        }
        if let Some(scene) = self.popover_controller.scene() {
            let anchor = self.active_topbar_anchor.map_or(topbar.rect, |anchor| {
                self.topbar_anchor_rect(topbar, anchor)
            });
            popover.place_popover(work, anchor, popover_height_for_rows(scene.rows().len()))?;
            return Ok(true);
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_context_menu_actions(
        &mut self,
        actions: &[QueuedContextMenuAction],
        topbar: &OwnedWindow,
        dock: &mut OwnedWindow,
        popover: &OwnedWindow,
        preview: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<bool> {
        for action in actions {
            match action {
                QueuedContextMenuAction::Redraw => {
                    self.redraw_popover(topbar, dock, popover, preview, settings)?;
                }
                QueuedContextMenuAction::Dismiss => {
                    self.dismiss_context_menu(topbar, dock, popover, preview, settings)?;
                }
                QueuedContextMenuAction::Execute { point, command } => {
                    let before = self.dock_controller.state().clone();
                    let visual_before = self.dock_controller.visual_generation();
                    self.context_menu_controller.dismiss();
                    popover.hide();
                    let mut dock_actions =
                        self.dock_controller
                            .release_overlay_hold()
                            .map_err(|error| {
                                windows::core::Error::new(invalid_arg(), error.to_string())
                            })?;
                    dock_actions.extend(
                        self.dock_controller
                            .handle_context_menu(*point, *command)
                            .map_err(|error| {
                                windows::core::Error::new(invalid_arg(), error.to_string())
                            })?,
                    );
                    if !apply_dock_actions(&dock_actions, self.dock_controller.state())? {
                        return Ok(false);
                    }
                    self.render_dock_change(
                        DockRenderBaseline::new(&before, visual_before),
                        &dock_actions,
                        DockRenderWindows {
                            topbar,
                            dock,
                            popover,
                            preview,
                            settings,
                        },
                    )?;
                }
            }
        }
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_popover_actions(
        &mut self,
        actions: &[QueuedPopoverAction],
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        preview: &OwnedWindow,
        settings: &mut OwnedWindow,
    ) -> Result<()> {
        for action in actions {
            match action {
                QueuedPopoverAction::Redraw | QueuedPopoverAction::RequestConfirmation(_) => {
                    self.redraw_popover(topbar, dock, popover, preview, settings)?;
                }
                QueuedPopoverAction::TypedIntent(PopoverAction::OpenSettings) => {
                    let work = crate::win32_windowing::window_work_area(settings.hwnd)?;
                    self.popover_controller.dismiss();
                    popover.hide();
                    settings.place_settings(work)?;
                    self.redraw_settings(topbar, dock, popover, preview, settings)?;
                    self.animate_surface_entrance(
                        ShowcaseRole::Settings,
                        SurfaceWindows {
                            topbar,
                            dock,
                            popover,
                            preview,
                            settings,
                        },
                    )?;
                    settings.show_activating();
                }
                QueuedPopoverAction::TypedIntent(PopoverAction::OpenBackgroundApp(id)) => {
                    let Some(entry) = self.background_apps.iter().find(|entry| entry.id() == *id)
                    else {
                        continue;
                    };
                    match crate::win32_background_apps::activate_background_app(
                        entry,
                        &self.latest_observed_windows,
                    ) {
                        Ok(()) => {
                            self.popover_controller.dismiss();
                            popover.hide();
                        }
                        Err(error) => crate::diagnostics::record(
                            crate::diagnostics::DiagnosticModule::AppLifecycle,
                            crate::diagnostics::LogLevel::Error,
                            "background_app_open_failed",
                            &[("code", error.code())],
                        ),
                    }
                }
                QueuedPopoverAction::TypedIntent(action) => {
                    record_system_action(
                        "popover.system_action",
                        crate::win32_system_actions::apply(action),
                    );
                    self.redraw_popover(topbar, dock, popover, preview, settings)?;
                }
                QueuedPopoverAction::Reload => {
                    self.popover_controller
                        .reload(&self.popover_provider, self.topbar_controller.snapshot())
                        .map_err(|error| {
                            windows::core::Error::new(invalid_arg(), error.to_string())
                        })?;
                    self.redraw_popover(topbar, dock, popover, preview, settings)?;
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
        preview: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        let windows = SurfaceWindows {
            topbar,
            dock,
            popover,
            preview,
            settings,
        };
        let window_size = settings.surface_physical_size();
        let scene = self.settings_controller.scene();
        let scenes = ShellScenes {
            topbar: None,
            dock: None,
            popover: None,
            context_menu: None,
            settings: Some(&scene),
            preview: None,
        };
        let update = present_frame(
            &mut self.surface_runtime,
            SurfaceFrame {
                target: SurfaceTarget {
                    hwnd: settings.hwnd,
                    role: ShowcaseRole::Settings,
                    metrics: SurfaceMetrics::new(window_size.0, window_size.1, settings.dpi()),
                },
                scenes,
            },
            SurfaceSizeChange::Resize,
        );
        self.complete_surface_update(update, windows)
    }

    pub(super) fn redraw_popover(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        preview: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        let windows = SurfaceWindows {
            topbar,
            dock,
            popover,
            preview,
            settings,
        };
        let scene = self.popover_controller.scene();
        let context_menu = self.context_menu_controller.scene();
        let (width, height) = popover.surface_physical_size();
        let update = present_frame(
            &mut self.surface_runtime,
            SurfaceFrame {
                target: SurfaceTarget {
                    hwnd: popover.hwnd,
                    role: POPOVER_SURFACE_ROLE,
                    metrics: SurfaceMetrics::new(width, height, popover.dpi()),
                },
                scenes: ShellScenes {
                    topbar: None,
                    dock: None,
                    popover: scene.as_ref(),
                    context_menu: context_menu.as_ref(),
                    settings: None,
                    preview: None,
                },
            },
            SurfaceSizeChange::Resize,
        );
        self.complete_surface_update(update, windows)
    }

    fn animate_surface_entrance(
        &mut self,
        role: ShowcaseRole,
        windows: SurfaceWindows<'_>,
    ) -> Result<()> {
        let update = self
            .surface_runtime
            .animate_entrance(role, self.reduced_motion);
        self.complete_surface_update(update, windows)
    }

    pub(super) fn topbar_anchor_rect(
        &self,
        topbar: &OwnedWindow,
        anchor: TopbarOverlayAnchor,
    ) -> PhysicalRect {
        let TopbarOverlayAnchor::Module(kind) = anchor else {
            return topbar.rect;
        };
        let Some(bounds) = self.topbar_controller.module_bounds(kind) else {
            return topbar.rect;
        };
        PhysicalRect::new(
            topbar.rect.x + physical_from_dip(bounds.x, topbar.dpi()),
            topbar.rect.y + physical_from_dip(bounds.y, topbar.dpi()),
            physical_from_dip(bounds.width, topbar.dpi()),
            physical_from_dip(bounds.height, topbar.dpi()),
        )
    }
}

fn record_system_action(event: &'static str, result: Result<()>) {
    if let Err(error) = result {
        let message = error.to_string();
        crate::diagnostics::record(
            crate::diagnostics::DiagnosticModule::AppLifecycle,
            crate::diagnostics::LogLevel::Error,
            event,
            &[("error", &message)],
        );
    }
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}

#[cfg(test)]
mod tests {
    use shell_renderer::Dpi;
    use shell_renderer::native::{ShowcaseRole, SurfaceMetrics};

    use crate::win32_surface_runtime::{SurfaceRenderDecision, surface_render_decision};

    use super::POPOVER_SURFACE_ROLE;

    #[test]
    fn settings_surface_policy_covers_missing_redraw_and_resize() {
        let desired = SurfaceMetrics::new(640, 480, Dpi::from_raw(144));
        let cases = [
            (None, SurfaceRenderDecision::RebuildAll),
            (Some(desired), SurfaceRenderDecision::Redraw),
            (
                Some(SurfaceMetrics::new(600, 480, Dpi::from_raw(144))),
                SurfaceRenderDecision::Resize,
            ),
        ];
        for (current, expected) in cases {
            assert_eq!(surface_render_decision(current, desired), expected);
        }
    }

    #[test]
    fn popover_and_context_share_role_size_policy_and_recovery() {
        let desired = SurfaceMetrics::new(320, 240, Dpi::from_raw(144));
        assert_eq!(POPOVER_SURFACE_ROLE, ShowcaseRole::Popover);
        assert_eq!(
            surface_render_decision(Some(desired), desired),
            SurfaceRenderDecision::Redraw
        );
        assert_eq!(
            surface_render_decision(
                Some(SurfaceMetrics::new(320, 200, Dpi::from_raw(144))),
                desired,
            ),
            SurfaceRenderDecision::Resize
        );
    }
}
