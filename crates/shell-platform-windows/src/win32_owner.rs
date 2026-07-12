#![deny(unsafe_code)]

use shell_renderer::native::{
    CompositionRenderer, DeviceKind, WindowSurface, is_recoverable_hresult,
};
use windows::core::Result;

use crate::win32_actions::apply_dock_actions;
use crate::win32_discovery::discover_running_windows;
use crate::win32_dock_render::dip_surface;
use crate::win32_window::OwnedWindow;
use crate::win32_windowing::primary_work_area;
use crate::{DockController, PlatformEvent, RuntimeAction, RuntimeOrchestrator};

pub(super) struct RuntimeSurfaces {
    force_warp: bool,
    pub(super) renderer: Option<CompositionRenderer>,
    topbar: Option<WindowSurface>,
    pub(super) dock: Option<WindowSurface>,
    orchestration: RuntimeOrchestrator,
    pub(super) dock_controller: DockController,
}

impl RuntimeSurfaces {
    pub(super) fn new(
        force_warp: bool,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        dock_controller: DockController,
    ) -> Result<Self> {
        let mut runtime = Self {
            force_warp,
            renderer: None,
            topbar: None,
            dock: None,
            orchestration: RuntimeOrchestrator::new(),
            dock_controller,
        };
        runtime.build(topbar, dock)?;
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
                self.render_dock_change(&before, visual_before, &actions, topbar, dock)?;
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
                self.render_dock_change(&before, visual_before, &actions, topbar, dock)?;
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
                self.render_dock_change(&before, visual_before, &actions, topbar, dock)?;
                return Ok(true);
            }
            PlatformEvent::SyncWindows => {
                let before = self.dock_controller.state().clone();
                let visual_before = self.dock_controller.visual_generation();
                let observed = discover_running_windows(&[topbar.hwnd, dock.hwnd])?;
                let actions = self
                    .dock_controller
                    .sync_running_windows(&observed)
                    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
                if !apply_dock_actions(&actions)? {
                    return Ok(false);
                }
                self.render_dock_change(&before, visual_before, &actions, topbar, dock)?;
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
            RuntimeAction::Rebuild => self.rebuild(topbar, dock)?,
            RuntimeAction::RepositionAndRebuild => {
                let work = primary_work_area()?;
                topbar.reposition(work)?;
                dock.apply_dock_visibility(
                    work,
                    self.dock_controller.config(),
                    !self.dock_controller.state().dock().is_revealed(),
                )?;
                self.rebuild(topbar, dock)?;
            }
            RuntimeAction::ResizeAndRebuild(_) => {
                topbar.refresh_rect()?;
                dock.refresh_rect()?;
                self.rebuild(topbar, dock)?;
            }
        }
        Ok(true)
    }

    pub(super) fn rebuild(&mut self, topbar: &OwnedWindow, dock: &OwnedWindow) -> Result<()> {
        self.topbar = None;
        self.dock = None;
        self.renderer = None;
        self.build(topbar, dock)
    }

    fn build(&mut self, topbar: &OwnedWindow, dock: &OwnedWindow) -> Result<()> {
        match self.build_once(topbar, dock) {
            Err(error) if is_recoverable_hresult(error.code()) => {
                self.topbar = None;
                self.dock = None;
                self.renderer = None;
                self.build_once(topbar, dock)
            }
            result => result,
        }
    }

    fn build_once(&mut self, topbar: &OwnedWindow, dock: &OwnedWindow) -> Result<()> {
        let renderer = CompositionRenderer::new(self.force_warp)?;
        let topbar_surface = renderer.create_surface(
            topbar.hwnd,
            topbar.role,
            topbar.rect.width.max(1) as u32,
            topbar.rect.height.max(1) as u32,
            None,
        )?;
        self.dock_controller.update_surface(dip_surface(dock));
        let dock_scene = self.dock_controller.scene();
        let dock_surface = renderer.create_surface(
            dock.hwnd,
            dock.role,
            dock.rect.width.max(1) as u32,
            dock.rect.height.max(1) as u32,
            Some(&dock_scene),
        )?;
        self.renderer = Some(renderer);
        self.topbar = Some(topbar_surface);
        self.dock = Some(dock_surface);
        self.trace_dock_state();
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

    pub(super) fn trace_dock_state(&self) {
        if std::env::var_os("MINHA_UI_QA_TRACE").is_some() {
            println!("{}", self.dock_controller.qa_trace_line());
        }
    }
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}
