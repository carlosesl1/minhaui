#![deny(unsafe_code)]

use shell_core::Popover;
use shell_renderer::native::{ShellScenes, ShowcaseRole, SurfaceMetrics};
use shell_renderer::{
    PhysicalRect, PopoverSurfaceSize, context_menu_height_for_entries, physical_from_dip,
    popover_surface_size, quick_settings_surface_size,
};
use windows::core::Result;

use crate::win32_actions::apply_dock_actions;
use crate::win32_dock_render::{DockRenderBaseline, DockRenderWindows};
use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows};
use crate::win32_surface_runtime::{SurfaceFrame, SurfaceSizeChange, SurfaceTarget, present_frame};
use crate::win32_window::OwnedWindow;
use crate::{
    PopoverAction, QueuedContextMenuAction, QueuedPopoverAction, QueuedQuickSettingsAction,
    QueuedTopbarAction, QuickSettingsIntent, TopbarOverlayAnchor,
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
                    popover.set_backdrop_enabled(false);
                    if (matches!(*kind, Popover::QuickSettings | Popover::Volume)
                        && self.quick_settings_controller.is_open()
                        && self.active_topbar_anchor == Some(*anchor))
                        || self.popover_controller.active_kind() == Some(*kind)
                    {
                        self.clear_active_topbar_module();
                        self.popover_controller.dismiss();
                        self.quick_settings_controller.dismiss();
                        popover.hide();
                        continue;
                    }
                    settings.hide();
                    self.active_topbar_anchor = Some(*anchor);
                    let active_module = match anchor {
                        TopbarOverlayAnchor::Module(kind) => Some(*kind),
                        TopbarOverlayAnchor::Overflow => None,
                    };
                    self.topbar_controller.set_active_module(active_module);
                    self.external_menu_coordinator.popover_replaced();
                    self.cancel_pending_tray_activation();
                    self.release_external_menu_handles();
                    self.popover_controller.clear_external_active();
                    self.popover_controller.dismiss();
                    self.quick_settings_controller.dismiss();
                    if matches!(*kind, Popover::QuickSettings | Popover::Volume) {
                        if *kind == Popover::QuickSettings {
                            if self.media_session_worker.is_none() {
                                self.media_session_worker =
                                    Some(crate::media_session_worker::MediaSessionWorker::start(
                                        crate::win32_event_queue::native_window_id(topbar.hwnd),
                                    ));
                            } else if let Some(worker) = self.media_session_worker.as_ref() {
                                worker
                                    .send(crate::media_session_types::MediaWorkerCommand::Refresh);
                            }
                        }
                        let capabilities =
                            crate::win32_quick_settings_capabilities::read_capabilities();
                        let audio = crate::win32_audio_panel::read().unwrap_or_default();
                        if *kind == Popover::Volume {
                            self.quick_settings_controller
                                .open_audio(capabilities, audio);
                        } else {
                            self.quick_settings_controller.open(capabilities);
                            self.quick_settings_controller.replace_audio_panel(audio);
                        }
                        let work = crate::win32_windowing::window_work_area(topbar.hwnd)?;
                        let scene = self.quick_settings_controller.scene();
                        let (width, height) = quick_settings_surface_size(
                            &scene,
                            work.height as f32 / topbar.dpi().scale(),
                        );
                        popover.place_popover(
                            work,
                            self.topbar_anchor_rect(topbar, *anchor),
                            PopoverSurfaceSize::new(width, height),
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
                        continue;
                    }
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
                    let size = self
                        .popover_controller
                        .scene()
                        .as_ref()
                        .map_or(PopoverSurfaceSize::new(244.0, 58.0), popover_surface_size);
                    popover.place_popover(work, self.topbar_anchor_rect(topbar, *anchor), size)?;
                    popover.set_backdrop_enabled(false);
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
                        self.background_apps_worker.request(
                            generation,
                            crate::win32_event_queue::native_window_id(topbar.hwnd),
                        );
                    }
                }
                QueuedTopbarAction::OpenSearch => {
                    self.clear_active_topbar_module();
                    self.popover_controller.dismiss();
                    self.quick_settings_controller.dismiss();
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
        let active_changed = self.clear_active_topbar_module();
        self.dismiss_context_menu(topbar, dock, popover, preview, settings)?;
        popover.hide();
        self.popover_controller.dismiss();
        self.quick_settings_controller.dismiss();
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
        if active_changed {
            self.redraw_topbar(topbar, dock, popover, preview, settings)?;
        }
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
        popover: &mut OwnedWindow,
        preview: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        let had_topbar_invoker = self.active_topbar_anchor.is_some();
        let active_changed = self.clear_active_topbar_module();
        self.popover_controller.dismiss();
        self.quick_settings_controller.dismiss();
        self.dismiss_context_menu(topbar, dock, popover, preview, settings)?;
        popover.hide();
        if active_changed {
            self.redraw_topbar(topbar, dock, popover, preview, settings)?;
        }
        if had_topbar_invoker {
            topbar.restore_focus();
        }
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
            popover.place_popover(work, anchor, popover_surface_size(&scene))?;
            return Ok(true);
        }
        if self.quick_settings_controller.is_open() {
            let scene = self.quick_settings_controller.scene();
            let anchor = self.active_topbar_anchor.map_or(topbar.rect, |anchor| {
                self.topbar_anchor_rect(topbar, anchor)
            });
            let (width, height) =
                quick_settings_surface_size(&scene, work.height as f32 / topbar.dpi().scale());
            popover.place_popover(work, anchor, PopoverSurfaceSize::new(width, height))?;
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
        app_menu: &mut OwnedWindow,
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
                    let active_changed = self.clear_active_topbar_module();
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
                    if active_changed {
                        self.redraw_topbar(topbar, dock, popover, preview, settings)?;
                    }
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
                            let active_changed = self.clear_active_topbar_module();
                            self.popover_controller.dismiss();
                            popover.hide();
                            if active_changed {
                                self.redraw_topbar(topbar, dock, popover, preview, settings)?;
                            }
                        }
                        Err(error) => crate::diagnostics::record(
                            crate::diagnostics::DiagnosticModule::AppLifecycle,
                            crate::diagnostics::LogLevel::Error,
                            "background_app_open_failed",
                            &[("code", error.code())],
                        ),
                    }
                }
                QueuedPopoverAction::TypedIntent(PopoverAction::OpenBackgroundAppContextMenu(
                    id,
                )) => {
                    self.open_background_app_context_menu(
                        *id, topbar, dock, popover, app_menu, preview, settings,
                    )?;
                }
                QueuedPopoverAction::TypedIntent(action) => {
                    let hides_popover = matches!(
                        action,
                        PopoverAction::OpenTaskManager
                            | PopoverAction::OpenSystemRoute(_)
                            | PopoverAction::OpenQuickSettings
                    );
                    let result = crate::win32_system_actions::apply(action);
                    let succeeded = result.is_ok();
                    record_system_action("popover.system_action", result);
                    if hides_popover && succeeded {
                        let active_changed = self.clear_active_topbar_module();
                        self.popover_controller.dismiss();
                        popover.hide();
                        if active_changed {
                            self.redraw_topbar(topbar, dock, popover, preview, settings)?;
                        }
                    } else {
                        self.redraw_popover(topbar, dock, popover, preview, settings)?;
                    }
                }
                QueuedPopoverAction::Reload => {
                    self.popover_controller
                        .reload(&self.popover_provider, self.topbar_controller.snapshot())
                        .map_err(|error| {
                            windows::core::Error::new(invalid_arg(), error.to_string())
                        })?;
                    self.redraw_popover(topbar, dock, popover, preview, settings)?;
                }
                QueuedPopoverAction::Dismiss => {
                    let had_topbar_invoker = self.active_topbar_anchor.is_some();
                    let active_changed = self.clear_active_topbar_module();
                    popover.hide();
                    if active_changed {
                        self.redraw_topbar(topbar, dock, popover, preview, settings)?;
                    }
                    if had_topbar_invoker {
                        topbar.restore_focus();
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_quick_settings_actions(
        &mut self,
        actions: &[QueuedQuickSettingsAction],
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &mut OwnedWindow,
        preview: &OwnedWindow,
        settings: &mut OwnedWindow,
    ) -> Result<()> {
        let mut redraw_popover = false;
        let mut redraw_topbar = false;
        let mut reflow_popover = false;
        for action in actions {
            match action {
                QueuedQuickSettingsAction::Redraw => {
                    redraw_popover = true;
                }
                QueuedQuickSettingsAction::Reflow => {
                    redraw_popover = true;
                    reflow_popover = true;
                }
                QueuedQuickSettingsAction::Media(command) => {
                    if let Some(worker) = self.media_session_worker.as_ref() {
                        worker.send(*command);
                    }
                }
                QueuedQuickSettingsAction::NightLight(request) => {
                    crate::night_light_worker::request_night_light(
                        *request,
                        crate::win32_event_queue::native_window_id(topbar.hwnd),
                    );
                }
                QueuedQuickSettingsAction::Brightness(request) => {
                    crate::brightness_worker::request_brightness(
                        *request,
                        crate::win32_event_queue::native_window_id(topbar.hwnd),
                    );
                }
                QueuedQuickSettingsAction::Intent(QuickSettingsIntent::Dismiss) => {
                    self.quick_settings_controller.dismiss();
                    redraw_topbar |= self.clear_active_topbar_module();
                    popover.hide();
                    redraw_popover = false;
                }
                QueuedQuickSettingsAction::Intent(QuickSettingsIntent::EditControls) => {
                    self.settings_controller.set_supported_quick_controls(
                        self.quick_settings_controller
                            .capabilities()
                            .supported_kinds(),
                    );
                    self.settings_controller
                        .open_section(crate::settings_controller::SettingsSection::QuickControls);
                    self.quick_settings_controller.dismiss();
                    self.clear_active_topbar_module();
                    popover.hide();
                    let work = crate::win32_windowing::window_work_area(settings.hwnd)?;
                    settings.place_settings(work)?;
                    self.redraw_settings(topbar, dock, popover, preview, settings)?;
                    settings.show_activating();
                    redraw_popover = false;
                }
                QueuedQuickSettingsAction::Intent(intent) => {
                    let capability = match intent {
                        QuickSettingsIntent::Activate(kind)
                        | QuickSettingsIntent::SetValue { kind, .. } => {
                            self.quick_settings_controller.capabilities().get(*kind)
                        }
                        QuickSettingsIntent::SetDoNotDisturbMode(_) => self
                            .quick_settings_controller
                            .capabilities()
                            .get(shell_core::QuickControlKind::Focus),
                        QuickSettingsIntent::SetProjectionMode(_) => self
                            .quick_settings_controller
                            .capabilities()
                            .get(shell_core::QuickControlKind::Projection),
                        QuickSettingsIntent::OpenMediaSessions
                        | QuickSettingsIntent::MediaAction(_)
                        | QuickSettingsIntent::SelectMediaSession(_) => None,
                        QuickSettingsIntent::SetAudioSessionVolume { .. }
                        | QuickSettingsIntent::SelectAudioOutput(_)
                        | QuickSettingsIntent::OpenSoundSettings => None,
                        QuickSettingsIntent::EditControls | QuickSettingsIntent::Dismiss => None,
                    };
                    match crate::win32_quick_settings_actions::apply_quick_settings_intent(
                        intent,
                        capability,
                    ) {
                        crate::win32_quick_settings_actions::QuickSettingsActionResult::Applied(control) => {
                            self.quick_settings_controller.apply_capability_update(control);
                            redraw_popover = true;
                        }
                        crate::win32_quick_settings_actions::QuickSettingsActionResult::AudioChanged(snapshot) => {
                            self.quick_settings_controller.replace_audio_panel(snapshot);
                            self.quick_settings_controller.replace_capabilities(
                                crate::win32_quick_settings_capabilities::read_capabilities(),
                            );
                            redraw_popover = true;
                            reflow_popover = true;
                        }
                        crate::win32_quick_settings_actions::QuickSettingsActionResult::OpenSystemRoute(route) => {
                            let result = crate::win32_system_actions::apply(
                                &PopoverAction::OpenSystemRoute(route),
                            );
                            let succeeded = result.is_ok();
                            record_system_action("quick_settings.open_system_route", result);
                            if succeeded {
                                self.quick_settings_controller.dismiss();
                                redraw_topbar |= self.clear_active_topbar_module();
                                popover.hide();
                                redraw_popover = false;
                            } else {
                                self.quick_settings_controller
                                    .set_error("Could not open Windows settings");
                                redraw_popover = true;
                            }
                        }
                        crate::win32_quick_settings_actions::QuickSettingsActionResult::NoChange => {}
                        crate::win32_quick_settings_actions::QuickSettingsActionResult::Failed(error) => {
                            crate::diagnostics::record(
                                crate::diagnostics::DiagnosticModule::AppLifecycle,
                                crate::diagnostics::LogLevel::Error,
                                "quick_settings_action_failed",
                                &[("error", &error.to_string())],
                            );
                            self.quick_settings_controller.set_error("Action unavailable");
                            redraw_popover = true;
                        }
                    }
                }
            }
        }
        if redraw_topbar {
            self.redraw_topbar(topbar, dock, popover, preview, settings)?;
        }
        if redraw_popover && self.quick_settings_controller.is_open() {
            if reflow_popover {
                let work = crate::win32_windowing::window_work_area(topbar.hwnd)?;
                self.place_active_overlay(work, topbar, dock, popover)?;
            }
            self.redraw_popover(topbar, dock, popover, preview, settings)?;
            if reflow_popover {
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
            quick_settings: None,
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
        let scene = self
            .popover_controller
            .scene()
            .map(|scene| scene.with_anchor_x(popover.popover_anchor_x_dip()));
        let quick_settings = self.quick_settings_controller.is_open().then(|| {
            self.quick_settings_controller
                .scene()
                .with_anchor_x(popover.popover_anchor_x_dip())
        });
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
                    quick_settings: quick_settings.as_ref(),
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

    pub(super) fn clear_active_topbar_module(&mut self) -> bool {
        let visual_generation = self.topbar_controller.visual_generation();
        self.active_topbar_anchor = None;
        self.topbar_controller.set_active_module(None);
        self.topbar_controller.visual_generation() != visual_generation
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
