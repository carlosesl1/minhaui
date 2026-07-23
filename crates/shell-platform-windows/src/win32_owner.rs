#![deny(unsafe_code)]

use std::time::{Duration, Instant};

use shell_renderer::native::{
    DeviceKind, ShellScenes, ShowcaseRole, SurfaceMetrics, SurfaceVisibilityAnimation,
};
use shell_renderer::{PhysicalRect, WindowPreviewPanelLayout, WindowPreviewScene};
use windows::core::Result;

use crate::dock_edge_detection::dock_edge_probe_mode;
use crate::win32_actions::apply_dock_actions;
use crate::win32_dock_render::{DockRenderBaseline, DockRenderWindows, dip_surface};
use crate::win32_fullscreen_sync::FullscreenSyncWindows;
use crate::win32_pointer::{DockCursorLocation, dock_cursor_location};
use crate::win32_preview::DwmPreviewThumbnail;
use crate::win32_preview_interaction::PreviewHit;
use crate::win32_preview_render::{PreviewPresentation, PreviewRenderWindows};
use crate::win32_sample_state::density_for_width;
use crate::win32_shell_observation::ShellObservation;
use crate::win32_surface_runtime::{
    NativeSurfaceOptions, SurfaceBuildPlan, SurfaceFrame, SurfaceTarget, SurfaceUpdate,
    Win32NativeSurfaceRuntime, runtime_device_kind,
};
use crate::win32_timer::TimerGuard;
use crate::win32_window::OwnedWindow;
use crate::win32_windowing::{window_monitor_bounds, window_work_area};
use crate::{
    DefaultPopoverDataProvider, DockContextMenuController, DockController, DockVisibilityMotion,
    PlatformEvent, PopoverController, PreviewController, PreviewEffect, PreviewEntranceMotion,
    PreviewPhase, RuntimeAction, RuntimeOrchestrator, SettingsController, TopbarController,
};

const EDGE_REVEAL_GRACE: Duration = Duration::from_millis(450);

pub(super) struct RuntimeSurfaces {
    pub(super) reduced_motion: bool,
    pub(super) surface_runtime: Win32NativeSurfaceRuntime,
    pub(super) fullscreen_suppressed: bool,
    orchestration: RuntimeOrchestrator,
    pub(super) dock_controller: DockController,
    pub(super) topbar_controller: TopbarController,
    pub(super) popover_controller: PopoverController,
    pub(super) context_menu_controller: DockContextMenuController,
    pub(super) settings_controller: SettingsController,
    pub(super) popover_provider: DefaultPopoverDataProvider<crate::OfflineWeatherProvider>,
    dock_animation_timer: Option<TimerGuard>,
    dock_edge_probe_timer: Option<TimerGuard>,
    edge_reveal_active: bool,
    edge_reveal_grace_deadline: Option<Instant>,
    last_dock_animation_frame: Option<Instant>,
    pub(super) dock_visibility_motion: Option<DockVisibilityMotion>,
    pub(super) dock_visibility_opacity: f32,
    preview_controller: PreviewController,
    preview_timer: Option<TimerGuard>,
    pub(super) preview_entrance: Option<PreviewEntranceMotion>,
    pub(super) preview_thumbnails: Vec<DwmPreviewThumbnail>,
    pub(super) preview_scene: Option<WindowPreviewScene>,
    pub(super) preview_layout: Option<WindowPreviewPanelLayout>,
    preview_pressed_hit: Option<PreviewHit>,
    pub(super) active_topbar_anchor: Option<crate::TopbarOverlayAnchor>,
    pub(super) background_apps: Vec<crate::background_apps::BackgroundAppEntry>,
    pub(super) latest_observed_windows: Vec<crate::ObservedWindow>,
}

#[derive(Clone, Copy)]
pub(super) struct SurfaceWindows<'a> {
    pub(super) topbar: &'a OwnedWindow,
    pub(super) dock: &'a OwnedWindow,
    pub(super) popover: &'a OwnedWindow,
    pub(super) preview: &'a OwnedWindow,
    pub(super) settings: &'a OwnedWindow,
}

pub(super) struct ShellObservationWindows<'a> {
    pub(super) topbar: &'a mut OwnedWindow,
    pub(super) dock: &'a mut OwnedWindow,
    pub(super) popover: &'a mut OwnedWindow,
    pub(super) preview: &'a mut OwnedWindow,
    pub(super) settings: &'a mut OwnedWindow,
}

pub(super) struct RuntimeOptions {
    pub(super) force_warp: bool,
    pub(super) solid_material: bool,
    pub(super) reduced_motion: bool,
    pub(super) liquid_glass: bool,
}

impl RuntimeSurfaces {
    pub(super) fn new(
        options: RuntimeOptions,
        windows: SurfaceWindows<'_>,
        dock_controller: DockController,
        topbar_controller: TopbarController,
        config: shell_config::ShellConfigV1,
    ) -> Result<Self> {
        let mut runtime = Self {
            reduced_motion: options.reduced_motion,
            surface_runtime: Win32NativeSurfaceRuntime::new(NativeSurfaceOptions {
                force_warp: options.force_warp,
                solid_material: options.solid_material,
                liquid_glass: options.liquid_glass,
                reduced_motion: options.reduced_motion,
            }),
            fullscreen_suppressed: false,
            orchestration: RuntimeOrchestrator::new(),
            dock_controller,
            topbar_controller,
            popover_controller: PopoverController::new(),
            context_menu_controller: DockContextMenuController::new(),
            settings_controller: SettingsController::new(config),
            popover_provider: DefaultPopoverDataProvider::offline(),
            dock_animation_timer: None,
            dock_edge_probe_timer: None,
            edge_reveal_active: false,
            edge_reveal_grace_deadline: None,
            last_dock_animation_frame: None,
            dock_visibility_motion: None,
            dock_visibility_opacity: 1.0,
            preview_controller: PreviewController::new(options.reduced_motion),
            preview_timer: None,
            preview_entrance: None,
            preview_thumbnails: Vec::new(),
            preview_scene: None,
            preview_layout: None,
            preview_pressed_hit: None,
            active_topbar_anchor: None,
            background_apps: Vec::new(),
            latest_observed_windows: Vec::new(),
        };
        runtime.rebuild_native_surfaces(windows)?;
        Ok(runtime)
    }

    pub(super) fn device_kind(&self) -> DeviceKind {
        runtime_device_kind(&self.surface_runtime)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_event(
        &mut self,
        event: PlatformEvent,
        topbar: &mut OwnedWindow,
        dock: &mut OwnedWindow,
        popover: &mut OwnedWindow,
        preview: &mut OwnedWindow,
        settings: &mut OwnedWindow,
    ) -> Result<bool> {
        let event = match event {
            PlatformEvent::BackgroundAppsLoaded(result) => {
                let (generation, captured) = result.into_parts();
                let (items, catalog) = match captured {
                    Ok(entries) => (
                        Ok(crate::popover_adapters::background_app_items(&entries)),
                        entries,
                    ),
                    Err(error) => (
                        Err(crate::PopoverDataError::Adapter(error.code())),
                        Vec::new(),
                    ),
                };
                if self
                    .popover_controller
                    .complete_background_apps(generation, items)
                {
                    self.background_apps = catalog;
                    if let Some(anchor) = self.active_topbar_anchor {
                        let work = window_work_area(topbar.hwnd)?;
                        let rows = self
                            .popover_controller
                            .scene()
                            .map_or(0, |scene| scene.rows().len());
                        popover.place_popover(
                            work,
                            self.topbar_anchor_rect(topbar, anchor),
                            shell_renderer::popover_height_for_rows(rows),
                        )?;
                    }
                    self.redraw_popover(topbar, dock, popover, preview, settings)?;
                }
                return Ok(true);
            }
            event => event,
        };
        match &event {
            PlatformEvent::DismissTransientOverlays => {
                self.dismiss_transient_overlays(topbar, dock, popover, preview, settings)?;
                let effects = self.preview_controller.dismiss(now_ms());
                self.apply_preview_effects(&effects, topbar, dock, popover, preview, settings)?;
                self.sync_preview_timer(dock)?;
                return Ok(true);
            }
            PlatformEvent::DockPointer(sample) => {
                self.edge_reveal_active = false;
                self.edge_reveal_grace_deadline = None;
                if sample.phase == crate::DockPointerPhase::Pressed {
                    settings.hide();
                }
                let before = self.dock_controller.state().clone();
                let visual_before = self.dock_controller.visual_generation();
                let actions = self
                    .dock_controller
                    .handle_pointer(*sample)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                if !apply_dock_actions(&actions, self.dock_controller.state())? {
                    return Ok(false);
                }
                self.render_dock_change(
                    DockRenderBaseline::new(&before, visual_before),
                    &actions,
                    DockRenderWindows {
                        topbar,
                        dock,
                        popover,
                        preview,
                        settings,
                    },
                )?;
                self.sync_dock_animation(dock)?;
                let target = self.dock_controller.hovered_item().filter(|item| {
                    !self
                        .dock_controller
                        .preview_windows_for_item(*item)
                        .is_empty()
                });
                let preview_effects = self
                    .preview_controller
                    .dock_target_changed(target, now_ms());
                self.apply_preview_effects(
                    &preview_effects,
                    topbar,
                    dock,
                    popover,
                    preview,
                    settings,
                )?;
                self.sync_preview_timer(dock)?;
                return Ok(true);
            }
            PlatformEvent::DockEdgeProbe => {
                let Some(location) = dock_cursor_location(dock.hwnd, dock.rect) else {
                    self.sync_dock_edge_probe(dock)?;
                    return Ok(true);
                };
                if !self.dock_controller.state().dock().is_revealed()
                    && location == DockCursorLocation::PhysicalBottom
                {
                    let before = self.dock_controller.state().clone();
                    let visual_before = self.dock_controller.visual_generation();
                    let actions =
                        self.dock_controller
                            .reveal_from_physical_edge()
                            .map_err(|error| {
                                windows::core::Error::new(invalid_arg(), error.to_string())
                            })?;
                    if !apply_dock_actions(&actions, self.dock_controller.state())? {
                        return Ok(false);
                    }
                    self.edge_reveal_active = true;
                    self.edge_reveal_grace_deadline = Some(Instant::now() + EDGE_REVEAL_GRACE);
                    self.render_dock_change(
                        DockRenderBaseline::new(&before, visual_before),
                        &actions,
                        DockRenderWindows {
                            topbar,
                            dock,
                            popover,
                            preview,
                            settings,
                        },
                    )?;
                    self.sync_dock_edge_probe(dock)?;
                    return Ok(true);
                }
                let grace_elapsed = self
                    .edge_reveal_grace_deadline
                    .is_none_or(|deadline| Instant::now() >= deadline);
                if self.edge_reveal_active && location == DockCursorLocation::Away && grace_elapsed
                {
                    self.edge_reveal_active = false;
                    self.edge_reveal_grace_deadline = None;
                    let before = self.dock_controller.state().clone();
                    let visual_before = self.dock_controller.visual_generation();
                    let actions = self
                        .dock_controller
                        .handle_pointer(crate::DockPointerSample::new(
                            crate::DockPointerPhase::Exited,
                            shell_renderer::DipPoint::new(-1.0, -1.0),
                        ))
                        .map_err(|error| {
                            windows::core::Error::new(invalid_arg(), error.to_string())
                        })?;
                    if !apply_dock_actions(&actions, self.dock_controller.state())? {
                        return Ok(false);
                    }
                    self.render_dock_change(
                        DockRenderBaseline::new(&before, visual_before),
                        &actions,
                        DockRenderWindows {
                            topbar,
                            dock,
                            popover,
                            preview,
                            settings,
                        },
                    )?;
                }
                self.sync_dock_edge_probe(dock)?;
                return Ok(true);
            }
            PlatformEvent::PreviewTimer => {
                let now = now_ms();
                let update = self.advance_preview_entrance(now, preview);
                self.complete_surface_update(
                    update,
                    SurfaceWindows {
                        topbar,
                        dock,
                        popover,
                        preview: &*preview,
                        settings,
                    },
                )?;
                let effects = self.preview_controller.tick(now);
                self.apply_preview_effects(&effects, topbar, dock, popover, preview, settings)?;
                self.sync_preview_timer(dock)?;
                return Ok(true);
            }
            PlatformEvent::PreviewPointerMoved(point) => {
                if point.x >= 0.0 && point.y >= 0.0 {
                    self.preview_controller.preview_entered();
                    self.update_preview_hover(
                        *point,
                        SurfaceWindows {
                            topbar,
                            dock,
                            popover,
                            preview,
                            settings,
                        },
                    )?;
                } else {
                    self.preview_pressed_hit = None;
                    self.update_preview_hover(
                        shell_renderer::DipPoint::new(-1.0, -1.0),
                        SurfaceWindows {
                            topbar,
                            dock,
                            popover,
                            preview,
                            settings,
                        },
                    )?;
                    let effects = self.preview_controller.dock_target_changed(None, now_ms());
                    self.apply_preview_effects(&effects, topbar, dock, popover, preview, settings)?;
                }
                self.sync_preview_timer(dock)?;
                return Ok(true);
            }
            PlatformEvent::PreviewPointerPressed(point) => {
                self.preview_pressed_hit = self.preview_hit(*point);
                return Ok(true);
            }
            PlatformEvent::PreviewPointerReleased(point) => {
                let released_hit = self.preview_hit(*point);
                let pressed_hit = self.preview_pressed_hit.take();
                if let Some(hit) = released_hit.filter(|hit| Some(*hit) == pressed_hit) {
                    match hit {
                        PreviewHit::Window(window, action) => {
                            let actions = self
                                .dock_controller
                                .handle_preview_action(window, action)
                                .map_err(|error| {
                                    windows::core::Error::new(invalid_arg(), error.to_string())
                                })?;
                            if !apply_dock_actions(&actions, self.dock_controller.state())? {
                                return Ok(false);
                            }
                            let effects = self.preview_controller.dismiss(now_ms());
                            self.apply_preview_effects(
                                &effects, topbar, dock, popover, preview, settings,
                            )?;
                        }
                        PreviewHit::Page(page) => {
                            let effects = self.preview_controller.set_page(page);
                            self.apply_preview_effects(
                                &effects, topbar, dock, popover, preview, settings,
                            )?;
                        }
                    }
                }
                self.sync_preview_timer(dock)?;
                return Ok(true);
            }
            PlatformEvent::PreviewDismissed => {
                self.preview_pressed_hit = None;
                let effects = self.preview_controller.dismiss(now_ms());
                self.apply_preview_effects(&effects, topbar, dock, popover, preview, settings)?;
                self.sync_preview_timer(dock)?;
                return Ok(true);
            }
            PlatformEvent::DockAnimationFrame => {
                if self.reduced_motion {
                    self.dock_controller.snap_animation_to_target();
                    self.cancel_dock_visibility_motion();
                    self.dock_animation_timer = None;
                    self.last_dock_animation_frame = None;
                    return Ok(true);
                }
                if self.dock_animation_timer.is_none() {
                    return Ok(true);
                }
                if self.dock_animations_idle() {
                    self.dock_animation_timer = None;
                    self.last_dock_animation_frame = None;
                    return Ok(true);
                }
                let dock_visibility_before = self.dock_controller.state().dock();
                let visual_before = self.dock_controller.visual_generation();
                let now = Instant::now();
                let delta_seconds = self
                    .last_dock_animation_frame
                    .replace(now)
                    .map_or(1.0 / 120.0, |previous| {
                        now.saturating_duration_since(previous).as_secs_f32()
                    });
                if !self.dock_controller.animations_idle() {
                    self.dock_controller.advance_animation(delta_seconds);
                }
                let update = self.advance_dock_visibility(dock, delta_seconds);
                self.complete_surface_update(
                    update,
                    SurfaceWindows {
                        topbar,
                        dock: &*dock,
                        popover,
                        preview,
                        settings,
                    },
                )?;
                self.render_dock_change(
                    DockRenderBaseline::visual_only(dock_visibility_before, visual_before),
                    &[],
                    DockRenderWindows {
                        topbar,
                        dock,
                        popover,
                        preview,
                        settings,
                    },
                )?;
                if self.dock_animations_idle() {
                    self.dock_animation_timer = None;
                    self.last_dock_animation_frame = None;
                }
                self.sync_dock_edge_probe(dock)?;
                return Ok(true);
            }
            PlatformEvent::DockKey(key) => {
                if matches!(key, crate::DockKey::Preview)
                    && let Some(item) = self.dock_controller.focused_item()
                    && !self
                        .dock_controller
                        .preview_windows_for_item(item)
                        .is_empty()
                {
                    let effects = self.preview_controller.open_from_keyboard(item, now_ms());
                    self.apply_preview_effects(&effects, topbar, dock, popover, preview, settings)?;
                    self.sync_preview_timer(dock)?;
                    return Ok(true);
                }
                if matches!(key, crate::DockKey::Escape)
                    && self.preview_controller.visible_item().is_some()
                {
                    let effects = self.preview_controller.dismiss(now_ms());
                    self.apply_preview_effects(&effects, topbar, dock, popover, preview, settings)?;
                    self.sync_preview_timer(dock)?;
                    return Ok(true);
                }
                let before = self.dock_controller.state().clone();
                let visual_before = self.dock_controller.visual_generation();
                let actions = self
                    .dock_controller
                    .handle_key(*key)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                if !apply_dock_actions(&actions, self.dock_controller.state())? {
                    return Ok(false);
                }
                self.render_dock_change(
                    DockRenderBaseline::new(&before, visual_before),
                    &actions,
                    DockRenderWindows {
                        topbar,
                        dock,
                        popover,
                        preview,
                        settings,
                    },
                )?;
                return Ok(true);
            }
            PlatformEvent::DockContextMenu { point, command } => {
                let before = self.dock_controller.state().clone();
                let visual_before = self.dock_controller.visual_generation();
                let actions = self
                    .dock_controller
                    .handle_context_menu(*point, *command)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                if !apply_dock_actions(&actions, self.dock_controller.state())? {
                    return Ok(false);
                }
                self.render_dock_change(
                    DockRenderBaseline::new(&before, visual_before),
                    &actions,
                    DockRenderWindows {
                        topbar,
                        dock,
                        popover,
                        preview,
                        settings,
                    },
                )?;
                return Ok(true);
            }
            PlatformEvent::DockContextMenuRequested { point } => {
                self.open_dock_context_menu(*point, topbar, dock, popover, preview, settings)?;
                return Ok(true);
            }
            PlatformEvent::DockDrop { point, path } => {
                let before = self.dock_controller.state().clone();
                let visual_before = self.dock_controller.visual_generation();
                let actions = self
                    .dock_controller
                    .handle_drop(*point, path)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                if !apply_dock_actions(&actions, self.dock_controller.state())? {
                    return Ok(false);
                }
                self.render_dock_change(
                    DockRenderBaseline::new(&before, visual_before),
                    &actions,
                    DockRenderWindows {
                        topbar,
                        dock,
                        popover,
                        preview,
                        settings,
                    },
                )?;
                return Ok(true);
            }
            PlatformEvent::TopbarPointer(sample) => {
                let actions = self
                    .topbar_controller
                    .handle_pointer(*sample)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                self.apply_topbar_actions(&actions, topbar, dock, popover, preview, settings)?;
                self.redraw_topbar(topbar, dock, popover, preview, settings)?;
                return Ok(true);
            }
            PlatformEvent::TopbarKey(key) => {
                let actions = self
                    .topbar_controller
                    .handle_key(*key)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                self.apply_topbar_actions(&actions, topbar, dock, popover, preview, settings)?;
                self.redraw_topbar(topbar, dock, popover, preview, settings)?;
                return Ok(true);
            }
            PlatformEvent::PopoverKey(key) => {
                if self.context_menu_controller.is_active() {
                    let actions = self.context_menu_controller.handle_key(*key);
                    return self.apply_context_menu_actions(
                        &actions, topbar, dock, popover, preview, settings,
                    );
                }
                let actions = self.popover_controller.handle_key(*key);
                self.apply_popover_actions(&actions, topbar, dock, popover, preview, settings)?;
                return Ok(true);
            }
            PlatformEvent::PopoverPointer(point) => {
                if self.context_menu_controller.is_active() {
                    let actions = self.context_menu_controller.handle_pointer_click(
                        *point,
                        crate::win32_dock_render::dip_surface(popover),
                    );
                    return self.apply_context_menu_actions(
                        &actions, topbar, dock, popover, preview, settings,
                    );
                }
                let actions = self
                    .popover_controller
                    .handle_pointer(*point, crate::win32_dock_render::dip_surface(popover));
                self.apply_popover_actions(&actions, topbar, dock, popover, preview, settings)?;
                return Ok(true);
            }
            PlatformEvent::PopoverPointerMoved(point) => {
                if self.context_menu_controller.is_active() {
                    let actions = self.context_menu_controller.handle_pointer_move(
                        *point,
                        crate::win32_dock_render::dip_surface(popover),
                    );
                    return self.apply_context_menu_actions(
                        &actions, topbar, dock, popover, preview, settings,
                    );
                }
                return Ok(true);
            }
            PlatformEvent::PopoverScroll(rows) => {
                if !self.context_menu_controller.is_active() {
                    let actions = self.popover_controller.handle_scroll(*rows);
                    self.apply_popover_actions(&actions, topbar, dock, popover, preview, settings)?;
                }
                return Ok(true);
            }
            PlatformEvent::SettingsKey(key) => {
                let actions = self
                    .settings_controller
                    .handle_key(*key)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                if actions.contains(&crate::QueuedSettingsAction::Dismiss) {
                    settings.hide();
                }
                self.redraw_settings(topbar, dock, popover, preview, settings)?;
                return Ok(true);
            }
            PlatformEvent::SyncWindows => {
                return Ok(true);
            }
            PlatformEvent::BackgroundAppsLoaded(_) => unreachable!("handled before routing"),
            PlatformEvent::TaskbarCreated
            | PlatformEvent::AppBarPositionChanged
            | PlatformEvent::DpiChanged(_)
            | PlatformEvent::DisplayChanged
            | PlatformEvent::PowerResumed
            | PlatformEvent::DeviceLost
            | PlatformEvent::QaExitRequested
            | PlatformEvent::CloseRequested
            | PlatformEvent::Destroyed => {}
        }
        let action = self.orchestration.handle(event);
        match action {
            RuntimeAction::None => {}
            RuntimeAction::Quit => return Ok(false),
            RuntimeAction::Rebuild => self.rebuild_native_surfaces(SurfaceWindows {
                topbar,
                dock,
                popover,
                preview,
                settings,
            })?,
            RuntimeAction::RepositionAndRebuild => {
                let work = window_work_area(dock.hwnd)?;
                topbar.reposition(window_monitor_bounds(topbar.hwnd)?)?;
                let (config, hidden) = crate::resolve_dock_visibility(
                    self.dock_controller.config(),
                    self.dock_controller.state().dock().is_revealed(),
                    self.fullscreen_suppressed,
                );
                self.snap_dock_visibility(dock, work, config, hidden)?;
                self.place_active_overlay(work, topbar, dock, popover)?;
                self.rebuild_native_surfaces(SurfaceWindows {
                    topbar,
                    dock,
                    popover,
                    preview,
                    settings,
                })?;
            }
            RuntimeAction::ResizeAndRebuild(_) => {
                topbar.refresh_rect()?;
                dock.refresh_rect()?;
                let work = window_work_area(dock.hwnd)?;
                let (config, hidden) = crate::resolve_dock_visibility(
                    self.dock_controller.config(),
                    self.dock_controller.state().dock().is_revealed(),
                    self.fullscreen_suppressed,
                );
                self.snap_dock_visibility(dock, work, config, hidden)?;
                popover.refresh_rect()?;
                preview.refresh_rect()?;
                settings.refresh_rect()?;
                self.rebuild_native_surfaces(SurfaceWindows {
                    topbar,
                    dock,
                    popover,
                    preview,
                    settings,
                })?;
            }
        }
        Ok(true)
    }

    pub(super) fn apply_shell_observation(
        &mut self,
        observation: &ShellObservation,
        windows: ShellObservationWindows<'_>,
    ) -> Result<bool> {
        let ShellObservationWindows {
            topbar,
            dock,
            popover,
            preview,
            settings,
        } = windows;
        self.latest_observed_windows = observation.windows().to_vec();
        self.apply_topbar_snapshot(
            observation.topbar(),
            SurfaceWindows {
                topbar,
                dock,
                popover,
                preview,
                settings,
            },
        )?;
        self.sync_fullscreen_suppression(
            observation.windows(),
            FullscreenSyncWindows {
                topbar,
                dock,
                popover,
                preview,
                settings,
            },
        )?;
        let before = self.dock_controller.state().clone();
        let visual_before = self.dock_controller.visual_generation();
        let actions = self
            .dock_controller
            .sync_running_windows_with_previews(observation.windows())
            .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
        if !apply_dock_actions(&actions, self.dock_controller.state())? {
            return Ok(false);
        }
        self.render_dock_change(
            DockRenderBaseline::new(&before, visual_before),
            &actions,
            DockRenderWindows {
                topbar,
                dock,
                popover,
                preview,
                settings,
            },
        )?;
        self.refit_window_previews(preview);
        Ok(true)
    }

    pub(super) fn sync_dock_animation(&mut self, dock: &OwnedWindow) -> Result<()> {
        if self.reduced_motion {
            self.dock_controller.snap_animation_to_target();
            self.dock_animation_timer = None;
            self.last_dock_animation_frame = None;
        } else if self.dock_animations_idle() {
            self.dock_animation_timer = None;
            self.last_dock_animation_frame = None;
        } else if self.dock_animation_timer.is_none() {
            self.dock_animation_timer = Some(TimerGuard::start_dock_animation(dock.hwnd)?);
            self.last_dock_animation_frame = Some(Instant::now());
        }
        self.sync_dock_edge_probe(dock)?;
        Ok(())
    }

    fn sync_dock_edge_probe(&mut self, dock: &OwnedWindow) -> Result<()> {
        let (effective_config, hidden) = crate::resolve_dock_visibility(
            self.dock_controller.config(),
            self.dock_controller.state().dock().is_revealed(),
            self.fullscreen_suppressed,
        );
        let hidden_probe = effective_config.autohide() && hidden;
        let visibility_animating = self.dock_visibility_motion.is_some();
        let cursor_near = if hidden_probe || self.edge_reveal_active || visibility_animating {
            dock_cursor_location(dock.hwnd, dock.rect).is_some_and(|location| {
                matches!(
                    location,
                    DockCursorLocation::PhysicalBottom
                        | DockCursorLocation::Dock
                        | DockCursorLocation::ApproachCorridor
                        | DockCursorLocation::NearPhysicalBottom
                )
            })
        } else {
            false
        };
        let mode = dock_edge_probe_mode(
            hidden_probe,
            self.edge_reveal_active,
            cursor_near,
            visibility_animating,
        );
        let Some(interval_ms) = mode.interval_ms() else {
            self.dock_edge_probe_timer = None;
            return Ok(());
        };
        if let Some(timer) = self.dock_edge_probe_timer.as_mut() {
            timer.rearm(interval_ms)?;
        } else {
            self.dock_edge_probe_timer =
                Some(TimerGuard::start_dock_edge_probe(dock.hwnd, interval_ms)?);
        }
        Ok(())
    }

    fn dock_animations_idle(&self) -> bool {
        self.dock_controller.animations_idle() && self.dock_visibility_motion.is_none()
    }

    pub(super) fn cancel_dock_visibility_motion(&mut self) {
        self.dock_visibility_motion = None;
    }

    fn sync_preview_timer(&mut self, dock: &OwnedWindow) -> Result<()> {
        let needs_timer = match self.preview_controller.phase() {
            PreviewPhase::Dwelling { .. } | PreviewPhase::Closing { .. } => true,
            PreviewPhase::Visible {
                bridge_deadline_ms, ..
            } => bridge_deadline_ms.is_some(),
            PreviewPhase::Closed => false,
        } || self.preview_entrance.is_some();
        if !needs_timer {
            self.preview_timer = None;
        } else if self.preview_timer.is_none() {
            self.preview_timer = Some(TimerGuard::start_preview(dock.hwnd)?);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_preview_effects(
        &mut self,
        effects: &[PreviewEffect],
        topbar: &OwnedWindow,
        dock: &mut OwnedWindow,
        popover: &OwnedWindow,
        preview: &mut OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        for effect in effects {
            match *effect {
                PreviewEffect::Show {
                    item,
                    page,
                    from_keyboard,
                } => self.show_window_preview(
                    PreviewPresentation::initial(item, page, from_keyboard),
                    PreviewRenderWindows {
                        topbar,
                        dock,
                        popover,
                        preview,
                        settings,
                    },
                )?,
                PreviewEffect::Update { item, page } => {
                    self.show_window_preview(
                        PreviewPresentation::update(item, page),
                        PreviewRenderWindows {
                            topbar,
                            dock,
                            popover,
                            preview,
                            settings,
                        },
                    )?;
                }
                PreviewEffect::BeginClose => {}
                PreviewEffect::Hide => self.hide_window_preview(preview),
                PreviewEffect::HoldDockReveal | PreviewEffect::ReleaseDockReveal => {
                    let before = self.dock_controller.state().clone();
                    let visual_before = self.dock_controller.visual_generation();
                    let actions = if matches!(effect, PreviewEffect::HoldDockReveal) {
                        self.dock_controller.hold_revealed_for_overlay()
                    } else {
                        self.dock_controller.release_overlay_hold()
                    }
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                    self.render_dock_change(
                        DockRenderBaseline::new(&before, visual_before),
                        &actions,
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
        Ok(())
    }

    pub(super) fn rebuild_native_surfaces(&mut self, windows: SurfaceWindows<'_>) -> Result<()> {
        let now = now_ms();
        let preview_scene = self.preview_scene.clone();
        self.topbar_controller
            .update_density(density_for_width(windows.topbar.rect.width));
        self.topbar_controller
            .update_surface(dip_surface(windows.topbar));
        let topbar_scene = self.topbar_controller.scene();
        self.dock_controller
            .update_surface(dip_surface(windows.dock));
        let dock_scene = self.dock_controller.scene();
        let popover_scene = self.popover_controller.scene();
        let context_menu_scene = self.context_menu_controller.scene();
        let settings_scene = self.settings_controller.scene();
        let targets = canonical_surface_targets(SurfaceEndpoints {
            topbar: surface_endpoint(windows.topbar),
            dock: surface_endpoint(windows.dock),
            popover: surface_endpoint(windows.popover),
            preview: surface_endpoint(windows.preview),
            settings: surface_endpoint(windows.settings),
        });
        let scenes = CanonicalSurfaceScenes {
            topbar: ShellScenes {
                topbar: Some(&topbar_scene),
                dock: None,
                popover: None,
                context_menu: None,
                settings: None,
                preview: None,
            },
            dock: ShellScenes {
                topbar: None,
                dock: Some(&dock_scene),
                popover: None,
                context_menu: None,
                settings: None,
                preview: None,
            },
            popover: ShellScenes {
                topbar: None,
                dock: None,
                popover: popover_scene.as_ref(),
                context_menu: context_menu_scene.as_ref(),
                settings: None,
                preview: None,
            },
            preview: ShellScenes {
                preview: preview_scene.as_ref(),
                ..empty_scenes()
            },
            settings: ShellScenes {
                topbar: None,
                dock: None,
                popover: None,
                context_menu: None,
                settings: Some(&settings_scene),
                preview: None,
            },
        };
        let composition = surface_composition_state(
            self.dock_visibility_motion,
            windows.dock.rect,
            self.dock_visibility_opacity,
            self.preview_entrance,
            now,
        );
        let plan = surface_build_plan(targets, scenes, composition);
        self.surface_runtime.rebuild(plan)?;
        if std::env::var_os("MINHA_UI_QA_TRACE").is_some() {
            println!("{}", self.dock_controller.qa_trace_line());
        }
        println!(
            "RESOURCE generation={} renderer={}",
            self.orchestration.generation(),
            match self.device_kind() {
                DeviceKind::Hardware => "hardware",
                DeviceKind::Warp => "warp",
            }
        );
        Ok(())
    }

    pub(super) fn complete_surface_update(
        &mut self,
        outcome: Result<SurfaceUpdate>,
        windows: SurfaceWindows<'_>,
    ) -> Result<()> {
        match outcome? {
            SurfaceUpdate::Presented => Ok(()),
            SurfaceUpdate::RebuildAllRequired => self.rebuild_native_surfaces(windows),
        }
    }
}

#[derive(Clone, Copy)]
struct SurfaceEndpoint {
    hwnd: windows::Win32::Foundation::HWND,
    metrics: SurfaceMetrics,
}

#[derive(Clone, Copy)]
struct SurfaceEndpoints {
    topbar: SurfaceEndpoint,
    dock: SurfaceEndpoint,
    popover: SurfaceEndpoint,
    preview: SurfaceEndpoint,
    settings: SurfaceEndpoint,
}

#[derive(Clone, Copy)]
struct CanonicalSurfaceTargets {
    topbar: SurfaceTarget,
    dock: SurfaceTarget,
    popover: SurfaceTarget,
    preview: SurfaceTarget,
    settings: SurfaceTarget,
}

#[derive(Clone, Copy)]
struct CanonicalSurfaceScenes<'scene> {
    topbar: ShellScenes<'scene>,
    dock: ShellScenes<'scene>,
    popover: ShellScenes<'scene>,
    preview: ShellScenes<'scene>,
    settings: ShellScenes<'scene>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SurfaceCompositionState {
    dock_offset_y: f32,
    dock_opacity: f32,
    dock_animation: Option<SurfaceVisibilityAnimation>,
    preview_opacity: f32,
}

fn surface_composition_state(
    dock_motion: Option<DockVisibilityMotion>,
    dock_base: PhysicalRect,
    dock_opacity: f32,
    preview_motion: Option<PreviewEntranceMotion>,
    now_ms: u64,
) -> SurfaceCompositionState {
    let (dock_offset_y, dock_opacity, dock_animation) =
        dock_motion.map_or((0.0, dock_opacity, None), |motion| {
            let current_opacity = motion.opacity();
            let remaining = motion.remaining_duration_seconds();
            (
                motion.current_offset_y(dock_base),
                current_opacity,
                (remaining > 0.0).then_some(SurfaceVisibilityAnimation {
                    start_offset_y: motion.current_offset_y(dock_base),
                    target_offset_y: motion.target_offset_y(dock_base),
                    start_opacity: current_opacity,
                    target_opacity: motion.target_opacity(),
                    duration_seconds: remaining,
                }),
            )
        });
    SurfaceCompositionState {
        dock_offset_y,
        dock_opacity,
        dock_animation,
        preview_opacity: preview_motion.map_or(1.0, |motion| motion.opacity_at(now_ms)),
    }
}

fn surface_endpoint(window: &OwnedWindow) -> SurfaceEndpoint {
    let (width, height) = window.surface_physical_size();
    SurfaceEndpoint {
        hwnd: window.hwnd,
        metrics: SurfaceMetrics::new(width, height, window.dpi()),
    }
}

const fn canonical_surface_targets(endpoints: SurfaceEndpoints) -> CanonicalSurfaceTargets {
    CanonicalSurfaceTargets {
        topbar: SurfaceTarget {
            hwnd: endpoints.topbar.hwnd,
            role: ShowcaseRole::Topbar,
            metrics: endpoints.topbar.metrics,
        },
        dock: SurfaceTarget {
            hwnd: endpoints.dock.hwnd,
            role: ShowcaseRole::Dock,
            metrics: endpoints.dock.metrics,
        },
        popover: SurfaceTarget {
            hwnd: endpoints.popover.hwnd,
            role: ShowcaseRole::Popover,
            metrics: endpoints.popover.metrics,
        },
        preview: SurfaceTarget {
            hwnd: endpoints.preview.hwnd,
            role: ShowcaseRole::Preview,
            metrics: endpoints.preview.metrics,
        },
        settings: SurfaceTarget {
            hwnd: endpoints.settings.hwnd,
            role: ShowcaseRole::Settings,
            metrics: endpoints.settings.metrics,
        },
    }
}

const fn surface_build_plan<'scene>(
    targets: CanonicalSurfaceTargets,
    scenes: CanonicalSurfaceScenes<'scene>,
    composition: SurfaceCompositionState,
) -> SurfaceBuildPlan<'scene> {
    SurfaceBuildPlan {
        topbar: SurfaceFrame {
            target: targets.topbar,
            scenes: scenes.topbar,
        },
        dock: SurfaceFrame {
            target: targets.dock,
            scenes: scenes.dock,
        },
        popover: SurfaceFrame {
            target: targets.popover,
            scenes: scenes.popover,
        },
        preview: SurfaceFrame {
            target: targets.preview,
            scenes: scenes.preview,
        },
        settings: SurfaceFrame {
            target: targets.settings,
            scenes: scenes.settings,
        },
        dock_visibility_offset_y: composition.dock_offset_y,
        dock_visibility_opacity: composition.dock_opacity,
        dock_visibility_animation: composition.dock_animation,
        preview_opacity: composition.preview_opacity,
    }
}

const fn empty_scenes() -> ShellScenes<'static> {
    ShellScenes {
        topbar: None,
        dock: None,
        popover: None,
        context_menu: None,
        settings: None,
        preview: None,
    }
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}

pub(super) fn now_ms() -> u64 {
    static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    u64::try_from(STARTED.get_or_init(Instant::now).elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use shell_core::DockItemId;
    use shell_renderer::native::{ShellScenes, ShowcaseRole, SurfaceMetrics};
    use shell_renderer::{Dpi, PhysicalRect, WindowPreviewScene};
    use windows::Win32::Foundation::HWND;

    use super::{
        CanonicalSurfaceScenes, SurfaceCompositionState, SurfaceEndpoint, SurfaceEndpoints,
        canonical_surface_targets, empty_scenes, surface_build_plan, surface_composition_state,
    };
    use crate::{DockVisibilityMotion, PreviewEntranceMotion, PreviewMotionSpec};

    fn endpoint(id: usize, width: u32, height: u32) -> SurfaceEndpoint {
        SurfaceEndpoint {
            hwnd: HWND(id as *mut std::ffi::c_void),
            metrics: SurfaceMetrics::new(width, height, Dpi::from_raw(96 + id as u32)),
        }
    }

    fn endpoints(generation: usize) -> SurfaceEndpoints {
        SurfaceEndpoints {
            topbar: endpoint(generation + 1, 101 + generation as u32, 11),
            dock: endpoint(generation + 2, 102 + generation as u32, 12),
            popover: endpoint(generation + 3, 103 + generation as u32, 13),
            preview: endpoint(generation + 4, 104 + generation as u32, 14),
            settings: endpoint(generation + 5, 105 + generation as u32, 15),
        }
    }

    fn scenes(preview: Option<&WindowPreviewScene>) -> CanonicalSurfaceScenes<'_> {
        CanonicalSurfaceScenes {
            topbar: empty_scenes(),
            dock: empty_scenes(),
            popover: empty_scenes(),
            preview: ShellScenes {
                preview,
                ..empty_scenes()
            },
            settings: empty_scenes(),
        }
    }

    const fn composition(dock_opacity: f32, preview_opacity: f32) -> SurfaceCompositionState {
        SurfaceCompositionState {
            dock_offset_y: 0.0,
            dock_opacity,
            dock_animation: None,
            preview_opacity,
        }
    }

    #[test]
    fn canonical_plan_maps_five_distinct_endpoints_to_their_slots() {
        let plan = surface_build_plan(
            canonical_surface_targets(endpoints(10)),
            scenes(None),
            composition(0.25, 0.75),
        );
        let actual = [
            plan.topbar.target,
            plan.dock.target,
            plan.popover.target,
            plan.preview.target,
            plan.settings.target,
        ]
        .map(|target| (target.hwnd.0 as usize, target.role, target.metrics.size()));
        assert_eq!(
            actual,
            [
                (11, ShowcaseRole::Topbar, (111, 11)),
                (12, ShowcaseRole::Dock, (112, 12)),
                (13, ShowcaseRole::Popover, (113, 13)),
                (14, ShowcaseRole::Preview, (114, 14)),
                (15, ShowcaseRole::Settings, (115, 15)),
            ]
        );
        assert_eq!(plan.dock_visibility_opacity, 0.25);
        assert_eq!(plan.preview_opacity, 0.75);
    }

    #[test]
    fn successive_plan_builds_observe_fresh_targets_scenes_and_opacity() {
        let first_scene = WindowPreviewScene::new(DockItemId::new(101), "First", Vec::new());
        let second_scene = WindowPreviewScene::new(DockItemId::new(202), "Second", Vec::new());
        let first = surface_build_plan(
            canonical_surface_targets(endpoints(100)),
            scenes(Some(&first_scene)),
            composition(0.1, 0.2),
        );
        let second = surface_build_plan(
            canonical_surface_targets(endpoints(200)),
            scenes(Some(&second_scene)),
            composition(0.9, 0.8),
        );

        assert_eq!(first.preview.target.hwnd.0 as usize, 104);
        assert_eq!(second.preview.target.hwnd.0 as usize, 204);
        assert_eq!(first.preview.scenes.preview.unwrap().item().value(), 101);
        assert_eq!(second.preview.scenes.preview.unwrap().item().value(), 202);
        assert_eq!(first.dock_visibility_opacity, 0.1);
        assert_eq!(second.dock_visibility_opacity, 0.9);
        assert_eq!(first.preview_opacity, 0.2);
        assert_eq!(second.preview_opacity, 0.8);
    }

    #[test]
    fn composition_snapshot_preserves_in_flight_dock_and_preview_state() {
        let visible = PhysicalRect::new(100, 900, 420, 55);
        let hidden = PhysicalRect::new(100, 1072, 420, 55);
        let mut dock_motion = DockVisibilityMotion::new(visible, hidden, 180);
        assert!(dock_motion.advance(0.09));
        let preview_motion = PreviewEntranceMotion::new(
            PhysicalRect::new(120, 700, 320, 200),
            PreviewMotionSpec {
                started_ms: 100,
                duration_ms: 200,
                offset_px: 4,
            },
        );

        let snapshot =
            surface_composition_state(Some(dock_motion), visible, 1.0, Some(preview_motion), 200);

        assert_eq!(
            snapshot.dock_offset_y,
            dock_motion.current_offset_y(visible)
        );
        assert_eq!(snapshot.dock_opacity, dock_motion.opacity());
        let animation = snapshot.dock_animation.unwrap();
        assert_eq!(animation.start_offset_y, snapshot.dock_offset_y);
        assert_eq!(animation.target_offset_y, 172.0);
        assert_eq!(animation.target_opacity, 0.0);
        assert!((animation.duration_seconds - 0.09).abs() < f32::EPSILON);
        assert_eq!(snapshot.preview_opacity, preview_motion.opacity_at(200));
    }
}
