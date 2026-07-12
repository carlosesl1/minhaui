#![deny(unsafe_code)]

use shell_core::{Effect, ShellState};
use shell_renderer::DipRect;
use shell_renderer::native::{PresentOutcome, ShellScenes, ShowcaseRole, is_recoverable_hresult};
use windows::core::Result;

use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows, now_ms};
use crate::win32_window::OwnedWindow;
use crate::{
    DockRenderAction, DockRenderChange, PollBudget, QueuedDockAction, QueuedTopbarAction,
    classify_dock_render_action,
};

pub(super) struct DockRenderBaseline<'a> {
    state: &'a ShellState,
    visual_generation: u64,
}

impl<'a> DockRenderBaseline<'a> {
    pub(super) const fn new(state: &'a ShellState, visual_generation: u64) -> Self {
        Self {
            state,
            visual_generation,
        }
    }
}

pub(super) struct DockRenderWindows<'a> {
    pub(super) topbar: &'a OwnedWindow,
    pub(super) dock: &'a mut OwnedWindow,
    pub(super) popover: &'a OwnedWindow,
    pub(super) settings: &'a OwnedWindow,
}

impl DockRenderWindows<'_> {
    fn surfaces(&self) -> SurfaceWindows<'_> {
        SurfaceWindows {
            topbar: self.topbar,
            dock: &*self.dock,
            popover: self.popover,
            settings: self.settings,
        }
    }
}

impl RuntimeSurfaces {
    pub(super) fn render_dock_change(
        &mut self,
        baseline: DockRenderBaseline<'_>,
        actions: &[QueuedDockAction],
        windows: DockRenderWindows<'_>,
    ) -> Result<()> {
        let current = self.dock_controller.state();
        let render_action = classify_dock_render_action(DockRenderChange {
            state_changed: baseline.state != current,
            visual_changed: baseline.visual_generation != self.dock_controller.visual_generation(),
            dock_visibility_changed: baseline.state.dock() != current.dock(),
            rebuild_requested: actions_require_rebuild(actions),
        });
        match render_action {
            DockRenderAction::None => Ok(()),
            DockRenderAction::RedrawDock => self.redraw_dock(windows.surfaces()),
            DockRenderAction::RebuildSurfaces => {
                self.apply_dock_visibility(windows.dock)?;
                self.redraw_dock(windows.surfaces())
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

    fn redraw_dock(&mut self, windows: SurfaceWindows<'_>) -> Result<()> {
        self.dock_controller
            .update_surface(dip_surface(windows.dock));
        let dock_scene = self.dock_controller.scene();
        self.update_preview_thumbnail(windows.dock, &dock_scene);
        let outcome = match (self.renderer.as_ref(), self.dock.as_ref()) {
            (Some(renderer), Some(surface)) => renderer.redraw_surface(
                surface,
                ShowcaseRole::Dock,
                ShellScenes {
                    topbar: None,
                    dock: Some(&dock_scene),
                    popover: None,
                    settings: None,
                },
            ),
            (None, _) | (_, None) => {
                return self.rebuild(
                    windows.topbar,
                    windows.dock,
                    windows.popover,
                    windows.settings,
                );
            }
        };
        match outcome {
            Ok(PresentOutcome::Presented) => {
                if std::env::var_os("MINHA_UI_QA_TRACE").is_some() {
                    println!("{}", self.dock_controller.qa_trace_line());
                }
                Ok(())
            }
            Ok(PresentOutcome::DeviceLost(_)) => self.rebuild(
                windows.topbar,
                windows.dock,
                windows.popover,
                windows.settings,
            ),
            Ok(PresentOutcome::Failed(code)) => Err(windows::core::Error::from_hresult(code)),
            Err(error) if is_recoverable_hresult(error.code()) => self.rebuild(
                windows.topbar,
                windows.dock,
                windows.popover,
                windows.settings,
            ),
            Err(error) => Err(error),
        }
    }

    pub(super) fn refresh_topbar_status(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        let now = now_ms();
        let budget = PollBudget::new(self.last_topbar_poll_ms, 1_000);
        match self.topbar_controller.refresh_status(budget, now) {
            QueuedTopbarAction::PollDeferred => Ok(()),
            QueuedTopbarAction::OpenPopover(_) | QueuedTopbarAction::RedrawTopbar => {
                self.last_topbar_poll_ms = now;
                let snapshot = self.topbar_status.snapshot(now);
                self.topbar_controller.update_snapshot(snapshot);
                self.redraw_topbar(topbar, dock, popover, settings)
            }
        }
    }

    pub(super) fn redraw_topbar(
        &mut self,
        topbar: &OwnedWindow,
        dock: &OwnedWindow,
        popover: &OwnedWindow,
        settings: &OwnedWindow,
    ) -> Result<()> {
        self.topbar_controller.update_surface(dip_surface(topbar));
        let scene = self.topbar_controller.scene();
        let outcome = match (self.renderer.as_ref(), self.topbar.as_ref()) {
            (Some(renderer), Some(surface)) => renderer.redraw_surface(
                surface,
                ShowcaseRole::Topbar,
                ShellScenes {
                    topbar: Some(&scene),
                    dock: None,
                    popover: None,
                    settings: None,
                },
            ),
            (None, _) | (_, None) => return self.rebuild(topbar, dock, popover, settings),
        };
        handle_present(
            outcome,
            self,
            SurfaceWindows {
                topbar,
                dock,
                popover,
                settings,
            },
        )
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

pub(super) fn handle_present(
    outcome: Result<PresentOutcome>,
    runtime: &mut RuntimeSurfaces,
    windows: SurfaceWindows<'_>,
) -> Result<()> {
    match outcome {
        Ok(PresentOutcome::Presented) => Ok(()),
        Ok(PresentOutcome::DeviceLost(_)) => runtime.rebuild(
            windows.topbar,
            windows.dock,
            windows.popover,
            windows.settings,
        ),
        Ok(PresentOutcome::Failed(code)) => Err(windows::core::Error::from_hresult(code)),
        Err(error) if is_recoverable_hresult(error.code()) => runtime.rebuild(
            windows.topbar,
            windows.dock,
            windows.popover,
            windows.settings,
        ),
        Err(error) => Err(error),
    }
}
