#![deny(unsafe_code)]

use std::time::Instant;

use shell_core::{Effect, ShellState};
use shell_renderer::DipRect;
use shell_renderer::native::{ShellScenes, ShowcaseRole, SurfaceMetrics};
use windows::core::Result;

use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows};
use crate::win32_surface_runtime::{
    SurfaceFrame, SurfaceSizeChange, SurfaceTarget, SurfaceUpdate, present_frame,
};
use crate::win32_window::OwnedWindow;
use crate::{DockRenderAction, DockRenderChange, QueuedDockAction, classify_dock_render_action};

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
    pub(super) preview: &'a OwnedWindow,
    pub(super) settings: &'a OwnedWindow,
}

impl DockRenderWindows<'_> {
    fn surfaces(&self) -> SurfaceWindows<'_> {
        SurfaceWindows {
            topbar: self.topbar,
            dock: &*self.dock,
            popover: self.popover,
            preview: self.preview,
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
        let diagnostic_started =
            crate::diagnostics::enabled(crate::diagnostics::DiagnosticModule::DockPerformance)
                .then(Instant::now);
        let mut diagnostic_action = "classify";
        let result = (|| {
            let desired_width = self.dock_controller.preferred_width_dip();
            let state_changed = baseline.state != self.dock_controller.state();
            let visibility_changed = baseline.state.dock() != self.dock_controller.state().dock();
            if visibility_changed {
                diagnostic_action = "visibility";
                let update = self.begin_dock_visibility(windows.dock);
                self.complete_surface_update(update, windows.surfaces())?;
                return self.sync_dock_animation(windows.dock);
            }
            if (windows.dock.dock_width_dip() - desired_width).abs() > 0.25 {
                diagnostic_action = "resize-rebuild";
                if self.dock_visibility_motion.is_some() {
                    return Ok(());
                }
                let work = crate::win32_windowing::window_work_area(windows.dock.hwnd)?;
                windows
                    .dock
                    .set_dock_width_preserving_y(work, desired_width)?;
                return self.rebuild_dock_surface(windows.surfaces());
            }
            let render_action = classify_dock_render_action(DockRenderChange {
                state_changed,
                visual_changed: baseline.visual_generation
                    != self.dock_controller.visual_generation(),
                dock_visibility_changed: visibility_changed,
                rebuild_requested: actions_require_rebuild(actions),
            });
            diagnostic_action = match render_action {
                DockRenderAction::None => "none",
                DockRenderAction::RedrawDock => "redraw",
                DockRenderAction::RebuildDockSurface => "rebuild-dock",
                DockRenderAction::RebuildAllSurfaces => "rebuild-all",
            };
            match render_action {
                DockRenderAction::None => Ok(()),
                DockRenderAction::RedrawDock => self.redraw_dock(windows.surfaces()),
                DockRenderAction::RebuildDockSurface => {
                    self.rebuild_dock_surface(windows.surfaces())
                }
                DockRenderAction::RebuildAllSurfaces => {
                    self.rebuild_native_surfaces(windows.surfaces())
                }
            }
        })();
        record_slow_dock_render(diagnostic_started, diagnostic_action, &result);
        result
    }

    fn redraw_dock(&mut self, windows: SurfaceWindows<'_>) -> Result<()> {
        let window_size = windows.dock.surface_physical_size();
        self.dock_controller
            .update_surface(dip_surface(windows.dock));
        let dock_scene = self.dock_controller.scene();
        let update = present_frame(
            &mut self.surface_runtime,
            SurfaceFrame {
                target: SurfaceTarget {
                    hwnd: windows.dock.hwnd,
                    role: ShowcaseRole::Dock,
                    metrics: SurfaceMetrics::new(window_size.0, window_size.1, windows.dock.dpi()),
                },
                scenes: ShellScenes {
                    topbar: None,
                    dock: Some(&dock_scene),
                    popover: None,
                    context_menu: None,
                    settings: None,
                    preview: None,
                },
            },
            SurfaceSizeChange::Resize,
        );
        self.finish_dock_update(update, windows)
    }

    fn rebuild_dock_surface(&mut self, windows: SurfaceWindows<'_>) -> Result<()> {
        self.dock_controller
            .update_surface(dip_surface(windows.dock));
        let scene = self.dock_controller.scene();
        let (width, height) = windows.dock.surface_physical_size();
        let scenes = ShellScenes {
            topbar: None,
            dock: Some(&scene),
            popover: None,
            context_menu: None,
            settings: None,
            preview: None,
        };
        let metrics = SurfaceMetrics::new(width, height, windows.dock.dpi());
        let update = present_frame(
            &mut self.surface_runtime,
            SurfaceFrame {
                target: SurfaceTarget {
                    hwnd: windows.dock.hwnd,
                    role: ShowcaseRole::Dock,
                    metrics,
                },
                scenes,
            },
            SurfaceSizeChange::Resize,
        );
        self.finish_dock_update(update, windows)
    }

    fn finish_dock_update(
        &mut self,
        update: Result<SurfaceUpdate>,
        windows: SurfaceWindows<'_>,
    ) -> Result<()> {
        let qa_line = std::env::var_os("MINHA_UI_QA_TRACE")
            .is_some()
            .then(|| self.dock_controller.qa_trace_line());
        finish_dock_update_with(
            update,
            || {
                if let Some(line) = qa_line {
                    println!("{line}");
                }
            },
            || self.rebuild_native_surfaces(windows),
        )
    }
}

const fn dock_qa_trace_required(update: Option<SurfaceUpdate>) -> bool {
    matches!(update, Some(SurfaceUpdate::Presented))
}

fn finish_dock_update_with<E>(
    update: std::result::Result<SurfaceUpdate, E>,
    on_presented: impl FnOnce(),
    on_rebuild: impl FnOnce() -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    let update = update?;
    if dock_qa_trace_required(Some(update)) {
        on_presented();
        Ok(())
    } else {
        on_rebuild()
    }
}

fn record_slow_dock_render(started: Option<Instant>, action: &str, result: &Result<()>) {
    let Some(started) = started else {
        return;
    };
    let duration = started.elapsed();
    if !crate::diagnostics::is_slow_dock_operation(duration) {
        return;
    }
    let duration_us = duration.as_micros().to_string();
    let outcome = if result.is_ok() {
        "succeeded"
    } else {
        "failed"
    };
    crate::diagnostics::record(
        crate::diagnostics::DiagnosticModule::DockPerformance,
        crate::diagnostics::LogLevel::Info,
        "dock.render.slow",
        &[
            ("action", action),
            ("duration_us", &duration_us),
            ("outcome", outcome),
        ],
    );
}

pub(super) fn dip_surface(window: &OwnedWindow) -> DipRect {
    let scale = window.dpi().scale();
    let (width, height) = window.surface_physical_size();
    DipRect::new(0.0, 0.0, width as f32 / scale, height as f32 / scale)
}

fn actions_require_rebuild(actions: &[QueuedDockAction]) -> bool {
    actions
        .iter()
        .any(|action| matches!(action, QueuedDockAction::Effect(Effect::RebuildSurfaces)))
}

#[cfg(test)]
mod tests {
    use crate::win32_surface_runtime::{
        SurfaceRenderDecision, SurfaceUpdate, surface_render_decision,
    };
    use shell_renderer::Dpi;
    use shell_renderer::native::SurfaceMetrics;

    use super::{dock_qa_trace_required, finish_dock_update_with};

    #[test]
    fn dock_surface_policy_covers_missing_redraw_resize_and_recovery() {
        let desired = SurfaceMetrics::new(387, 55, Dpi::from_raw(144));
        assert_eq!(
            surface_render_decision(None, desired),
            SurfaceRenderDecision::RebuildAll
        );
        assert_eq!(
            surface_render_decision(Some(desired), desired),
            SurfaceRenderDecision::Redraw
        );
        assert_eq!(
            surface_render_decision(
                Some(SurfaceMetrics::new(328, 55, Dpi::from_raw(144))),
                desired,
            ),
            SurfaceRenderDecision::Resize
        );
    }

    #[test]
    fn dock_qa_trace_is_emitted_only_after_presented_updates() {
        assert!(dock_qa_trace_required(Some(SurfaceUpdate::Presented)));
        assert!(!dock_qa_trace_required(Some(
            SurfaceUpdate::RebuildAllRequired
        )));
        assert!(!dock_qa_trace_required(None));
    }

    #[test]
    fn dock_finish_wiring_runs_qa_for_presented_and_rebuild_only_for_recovery() {
        let calls = std::cell::RefCell::new(Vec::new());
        finish_dock_update_with(
            Ok::<_, &'static str>(SurfaceUpdate::Presented),
            || calls.borrow_mut().push("qa"),
            || {
                calls.borrow_mut().push("rebuild");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls.borrow().as_slice(), ["qa"]);

        calls.borrow_mut().clear();
        finish_dock_update_with(
            Ok::<_, &'static str>(SurfaceUpdate::RebuildAllRequired),
            || calls.borrow_mut().push("qa"),
            || {
                calls.borrow_mut().push("rebuild");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls.borrow().as_slice(), ["rebuild"]);

        calls.borrow_mut().clear();
        let error = finish_dock_update_with(
            Err::<SurfaceUpdate, _>("fatal"),
            || calls.borrow_mut().push("qa"),
            || {
                calls.borrow_mut().push("rebuild");
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(error, "fatal");
        assert!(calls.borrow().is_empty());
    }
}
