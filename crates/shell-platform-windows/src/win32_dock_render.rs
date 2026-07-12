#![deny(unsafe_code)]

use shell_core::{Effect, ShellState};
use shell_renderer::DipRect;
use shell_renderer::native::{
    PresentOutcome, ShowcaseRole, device_loss_hresult, is_recoverable_hresult,
};
use windows::core::Result;

use crate::win32_owner::RuntimeSurfaces;
use crate::win32_window::OwnedWindow;
use crate::{DockRenderAction, DockRenderChange, QueuedDockAction, classify_dock_render_action};

impl RuntimeSurfaces {
    pub(super) fn render_dock_change(
        &mut self,
        before: &ShellState,
        visual_before: u64,
        actions: &[QueuedDockAction],
        topbar: &OwnedWindow,
        dock: &mut OwnedWindow,
    ) -> Result<()> {
        let current = self.dock_controller.state();
        let render_action = classify_dock_render_action(DockRenderChange {
            state_changed: before != current,
            visual_changed: visual_before != self.dock_controller.visual_generation(),
            dock_visibility_changed: before.dock() != current.dock(),
            rebuild_requested: actions_require_rebuild(actions),
        });
        match render_action {
            DockRenderAction::None => Ok(()),
            DockRenderAction::RedrawDock => self.redraw_dock(topbar, dock),
            DockRenderAction::RebuildSurfaces => {
                self.apply_dock_visibility(dock)?;
                self.rebuild(topbar, dock)
            }
        }
    }

    fn apply_dock_visibility(&self, dock: &mut OwnedWindow) -> Result<()> {
        let work = crate::win32_windowing::window_work_area(dock.hwnd)?;
        dock.apply_dock_visibility(
            work,
            self.dock_controller.config(),
            !self.dock_controller.state().dock().is_revealed(),
        )
    }

    fn redraw_dock(&mut self, topbar: &OwnedWindow, dock: &OwnedWindow) -> Result<()> {
        self.dock_controller.update_surface(dip_surface(dock));
        let dock_scene = self.dock_controller.scene();
        self.update_preview_thumbnail(dock, &dock_scene);
        let outcome = match (self.renderer.as_ref(), self.dock.as_ref()) {
            (Some(renderer), Some(surface)) => {
                renderer.redraw_surface(surface, ShowcaseRole::Dock, Some(&dock_scene))
            }
            (None, _) | (_, None) => return self.rebuild(topbar, dock),
        };
        match outcome {
            Ok(PresentOutcome::Presented) => {
                if std::env::var_os("MINHA_UI_QA_TRACE").is_some() {
                    println!("{}", self.dock_controller.qa_trace_line());
                }
                Ok(())
            }
            Ok(PresentOutcome::DeviceLost(kind)) => {
                let error = windows::core::Error::new(
                    device_loss_hresult(kind),
                    format!("recoverable device loss during dock redraw: {kind:?}"),
                );
                if is_recoverable_hresult(error.code()) {
                    self.rebuild(topbar, dock)
                } else {
                    Err(error)
                }
            }
            Ok(PresentOutcome::Failed(code)) => Err(windows::core::Error::from_hresult(code)),
            Err(error) if is_recoverable_hresult(error.code()) => self.rebuild(topbar, dock),
            Err(error) => Err(error),
        }
    }
}

pub(super) fn dip_surface(window: &OwnedWindow) -> DipRect {
    let scale = window.dpi().scale();
    DipRect::new(
        0.0,
        0.0,
        window.rect.width as f32 / scale,
        window.rect.height as f32 / scale,
    )
}

fn actions_require_rebuild(actions: &[QueuedDockAction]) -> bool {
    actions
        .iter()
        .any(|action| matches!(action, QueuedDockAction::Effect(Effect::RebuildSurfaces)))
}
