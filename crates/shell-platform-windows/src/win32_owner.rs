#![deny(unsafe_code)]

use shell_renderer::native::{
    CompositionRenderer, DeviceKind, WindowSurface, is_recoverable_hresult,
};
use windows::core::Result;

use crate::win32::OwnedWindow;
use crate::win32_windowing::primary_work_area;
use crate::{PlatformEvent, RuntimeAction, RuntimeOrchestrator};

pub(super) struct RuntimeSurfaces {
    force_warp: bool,
    renderer: Option<CompositionRenderer>,
    topbar: Option<WindowSurface>,
    dock: Option<WindowSurface>,
    orchestration: RuntimeOrchestrator,
}

impl RuntimeSurfaces {
    pub(super) fn new(force_warp: bool, topbar: &OwnedWindow, dock: &OwnedWindow) -> Result<Self> {
        let mut runtime = Self {
            force_warp,
            renderer: None,
            topbar: None,
            dock: None,
            orchestration: RuntimeOrchestrator::new(),
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
        let action = self.orchestration.handle(event);
        match action {
            RuntimeAction::None => {}
            RuntimeAction::Quit => return Ok(false),
            RuntimeAction::Rebuild => self.rebuild(topbar, dock)?,
            RuntimeAction::RepositionAndRebuild => {
                let work = primary_work_area()?;
                topbar.reposition(work)?;
                dock.reposition(work)?;
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

    fn rebuild(&mut self, topbar: &OwnedWindow, dock: &OwnedWindow) -> Result<()> {
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
        )?;
        let dock_surface = renderer.create_surface(
            dock.hwnd,
            dock.role,
            dock.rect.width.max(1) as u32,
            dock.rect.height.max(1) as u32,
        )?;
        self.renderer = Some(renderer);
        self.topbar = Some(topbar_surface);
        self.dock = Some(dock_surface);
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
}
