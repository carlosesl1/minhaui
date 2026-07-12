#![deny(unsafe_code)]

use shell_renderer::native::{
    CompositionRenderer, DeviceKind, ShellScenes, WindowSurface, is_recoverable_hresult,
};
use windows::core::Result;

use crate::win32_actions::apply_dock_actions;
use crate::win32_discovery::discover_running_windows;
use crate::win32_dock_render::{DockRenderBaseline, DockRenderWindows, dip_surface};
use crate::win32_preview::DwmPreviewThumbnail;
use crate::win32_preview_qa::seed_restricted_preview_for_qa;
use crate::win32_sample_state::density_for_width;
use crate::win32_topbar_status::TopbarStatusReader;
use crate::win32_window::OwnedWindow;
use crate::win32_windowing::window_work_area;
use crate::{
    DefaultPopoverDataProvider, DockController, PlatformEvent, PopoverController, RuntimeAction,
    RuntimeOrchestrator, SettingsController, TopbarController,
};

pub(super) struct RuntimeSurfaces {
    force_warp: bool,
    pub(super) renderer: Option<CompositionRenderer>,
    pub(super) topbar: Option<WindowSurface>,
    pub(super) dock: Option<WindowSurface>,
    pub(super) popover: Option<WindowSurface>,
    pub(super) settings: Option<WindowSurface>,
    preview_thumbnail: Option<DwmPreviewThumbnail>,
    pub(super) fullscreen_suppressed: bool,
    orchestration: RuntimeOrchestrator,
    pub(super) dock_controller: DockController,
    pub(super) topbar_controller: TopbarController,
    pub(super) popover_controller: PopoverController,
    pub(super) settings_controller: SettingsController,
    pub(super) popover_provider: DefaultPopoverDataProvider<crate::OfflineWeatherProvider>,
    pub(super) topbar_status: TopbarStatusReader,
    pub(super) last_topbar_poll_ms: u64,
}

pub(super) struct SurfaceWindows<'a> {
    pub(super) topbar: &'a OwnedWindow,
    pub(super) dock: &'a OwnedWindow,
    pub(super) popover: &'a OwnedWindow,
    pub(super) settings: &'a OwnedWindow,
}

impl RuntimeSurfaces {
    pub(super) fn new(
        force_warp: bool,
        windows: SurfaceWindows<'_>,
        dock_controller: DockController,
        topbar_controller: TopbarController,
    ) -> Result<Self> {
        let mut runtime = Self {
            force_warp,
            renderer: None,
            topbar: None,
            dock: None,
            popover: None,
            settings: None,
            preview_thumbnail: None,
            fullscreen_suppressed: false,
            orchestration: RuntimeOrchestrator::new(),
            dock_controller,
            topbar_controller,
            popover_controller: PopoverController::new(),
            settings_controller: SettingsController::new(shell_config::ShellConfigV1::default()),
            popover_provider: DefaultPopoverDataProvider::offline(),
            topbar_status: TopbarStatusReader::default(),
            last_topbar_poll_ms: 0,
        };
        runtime.build(
            windows.topbar,
            windows.dock,
            windows.popover,
            windows.settings,
        )?;
        Ok(runtime)
    }

    pub(super) fn device_kind(&self) -> DeviceKind {
        self.renderer
            .as_ref()
            .map_or(DeviceKind::Warp, CompositionRenderer::device_kind)
    }

    pub(super) fn handle_event(
        &mut self,
        event: PlatformEvent,
        topbar: &mut OwnedWindow,
        dock: &mut OwnedWindow,
        popover: &mut OwnedWindow,
        settings: &mut OwnedWindow,
    ) -> Result<bool> {
        match &event {
            PlatformEvent::DockPointer(sample) => {
                let before = self.dock_controller.state().clone();
                let visual_before = self.dock_controller.visual_generation();
                let actions = self
                    .dock_controller
                    .handle_pointer(*sample)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                if !apply_dock_actions(&actions)? {
                    return Ok(false);
                }
                self.render_dock_change(
                    DockRenderBaseline::new(&before, visual_before),
                    &actions,
                    DockRenderWindows {
                        topbar,
                        dock,
                        popover,
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
                if !apply_dock_actions(&actions)? {
                    return Ok(false);
                }
                self.render_dock_change(
                    DockRenderBaseline::new(&before, visual_before),
                    &actions,
                    DockRenderWindows {
                        topbar,
                        dock,
                        popover,
                        settings,
                    },
                )?;
                return Ok(true);
            }
            PlatformEvent::DockDrop { point, path } => {
                let before = self.dock_controller.state().clone();
                let visual_before = self.dock_controller.visual_generation();
                let actions = self
                    .dock_controller
                    .handle_drop(*point, path)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                if !apply_dock_actions(&actions)? {
                    return Ok(false);
                }
                self.render_dock_change(
                    DockRenderBaseline::new(&before, visual_before),
                    &actions,
                    DockRenderWindows {
                        topbar,
                        dock,
                        popover,
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
                self.apply_topbar_actions(&actions, topbar, dock, popover, settings)?;
                self.redraw_topbar(topbar, dock, popover, settings)?;
                return Ok(true);
            }
            PlatformEvent::PopoverKey(key) => {
                let actions = self.popover_controller.handle_key(*key);
                self.apply_popover_actions(&actions, topbar, dock, popover, settings)?;
                return Ok(true);
            }
            PlatformEvent::SettingsKey(key) => {
                match key {
                    crate::PopoverKey::Next => self.settings_controller.focus_next(),
                    crate::PopoverKey::Previous => self.settings_controller.focus_previous(),
                    crate::PopoverKey::Activate => {}
                    crate::PopoverKey::Escape => settings.hide(),
                }
                self.redraw_settings(topbar, dock, popover, settings)?;
                return Ok(true);
            }
            PlatformEvent::SyncWindows => {
                self.refresh_topbar_status(topbar, dock, popover, settings)?;
                let before = self.dock_controller.state().clone();
                let visual_before = self.dock_controller.visual_generation();
                let mut observed =
                    discover_running_windows(&[topbar.hwnd, dock.hwnd], Some(dock.hwnd))?;
                seed_restricted_preview_for_qa(&mut observed)?;
                self.sync_fullscreen_suppression(&observed, topbar, dock, popover)?;
                let actions = self
                    .dock_controller
                    .sync_running_windows_with_previews(&observed)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                if !apply_dock_actions(&actions)? {
                    return Ok(false);
                }
                self.render_dock_change(
                    DockRenderBaseline::new(&before, visual_before),
                    &actions,
                    DockRenderWindows {
                        topbar,
                        dock,
                        popover,
                        settings,
                    },
                )?;
                return Ok(true);
            }
            PlatformEvent::TaskbarCreated
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
            RuntimeAction::Rebuild => self.rebuild(topbar, dock, popover, settings)?,
            RuntimeAction::RepositionAndRebuild => {
                let work = window_work_area(dock.hwnd)?;
                topbar.reposition(work)?;
                dock.apply_dock_visibility(
                    work,
                    self.dock_controller.config(),
                    !self.dock_controller.state().dock().is_revealed(),
                )?;
                if self.popover_controller.active_kind().is_some() {
                    popover.place_popover(work, topbar.rect)?;
                }
                self.rebuild(topbar, dock, popover, settings)?;
            }
            RuntimeAction::ResizeAndRebuild(_) => {
                topbar.refresh_rect()?;
                dock.refresh_rect()?;
                popover.refresh_rect()?;
                settings.refresh_rect()?;
                self.rebuild(topbar, dock, popover, settings)?;
            }
        }
        Ok(true)
    }

    pub(super) fn rebuild(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        self.topbar = None;
        self.dock = None;
        self.popover = None;
        self.settings = None;
        self.renderer = None;
        self.preview_thumbnail = None;
        self.build(topbar, dock, popover, settings)
    }

    fn build(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        match self.build_once(topbar, dock, popover, settings) {
            Err(error) if is_recoverable_hresult(error.code()) => {
                self.topbar = None;
                self.dock = None;
                self.popover = None;
                self.settings = None;
                self.renderer = None;
                self.build_once(topbar, dock, popover, settings)
            }
            result => result,
        }
    }

    fn build_once(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        let renderer = CompositionRenderer::new(self.force_warp)?;
        self.topbar_controller
            .update_density(density_for_width(topbar.rect.width));
        self.topbar_controller.update_surface(dip_surface(topbar));
        self.topbar_controller
            .update_snapshot(self.topbar_status.snapshot(now_ms()));
        let topbar_scene = self.topbar_controller.scene();
        let topbar_surface = renderer.create_surface(
            topbar.hwnd,
            topbar.role,
            topbar.rect.width.max(1) as u32,
            topbar.rect.height.max(1) as u32,
            ShellScenes {
                topbar: Some(&topbar_scene),
                dock: None,
                popover: None,
                settings: None,
            },
        )?;
        self.dock_controller.update_surface(dip_surface(dock));
        let dock_scene = self.dock_controller.scene();
        let dock_surface = renderer.create_surface(
            dock.hwnd,
            dock.role,
            dock.rect.width.max(1) as u32,
            dock.rect.height.max(1) as u32,
            ShellScenes {
                topbar: None,
                dock: Some(&dock_scene),
                popover: None,
                settings: None,
            },
        )?;
        let popover_scene = self.popover_controller.scene();
        let popover_surface = renderer.create_surface(
            popover.hwnd,
            popover.role,
            popover.rect.width.max(1) as u32,
            popover.rect.height.max(1) as u32,
            ShellScenes {
                topbar: None,
                dock: None,
                popover: popover_scene.as_ref(),
                settings: None,
            },
        )?;
        let settings_scene = self.settings_controller.scene();
        let settings_surface = renderer.create_surface(
            settings.hwnd,
            settings.role,
            settings.rect.width.max(1) as u32,
            settings.rect.height.max(1) as u32,
            ShellScenes {
                topbar: None,
                dock: None,
                popover: None,
                settings: Some(&settings_scene),
            },
        )?;
        self.renderer = Some(renderer);
        self.topbar = Some(topbar_surface);
        self.dock = Some(dock_surface);
        self.popover = Some(popover_surface);
        self.settings = Some(settings_surface);
        self.update_preview_thumbnail(dock, &dock_scene);
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

    pub(super) fn update_preview_thumbnail(
        &mut self,
        dock: &OwnedWindow,
        scene: &shell_renderer::DockScene,
    ) {
        let Some(preview) = scene.window_previews().first().copied() else {
            self.preview_thumbnail = None;
            return;
        };
        if self
            .preview_thumbnail
            .as_ref()
            .is_some_and(|current| current.window() == preview.window())
        {
            return;
        }
        let destination = shell_renderer::PhysicalRect::new(
            16,
            8,
            (dock.rect.width - 32).clamp(1, 260),
            (dock.rect.height - 24).clamp(1, 132),
        );
        self.preview_thumbnail =
            DwmPreviewThumbnail::show(dock.hwnd, preview, destination).unwrap_or(None);
    }
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}
