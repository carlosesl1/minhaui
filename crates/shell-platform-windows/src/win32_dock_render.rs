#![deny(unsafe_code)]

use std::time::{Duration, Instant};

use shell_core::{AutohideState, Effect, ShellState};
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
    state: Option<&'a ShellState>,
    dock_visibility: AutohideState,
    visual_generation: u64,
}

impl<'a> DockRenderBaseline<'a> {
    pub(super) const fn new(state: &'a ShellState, visual_generation: u64) -> Self {
        Self {
            state: Some(state),
            dock_visibility: state.dock(),
            visual_generation,
        }
    }

    pub(super) const fn visual_only(
        dock_visibility: AutohideState,
        visual_generation: u64,
    ) -> Self {
        Self {
            state: None,
            dock_visibility,
            visual_generation,
        }
    }

    fn change(
        &self,
        current: &ShellState,
        current_visual_generation: u64,
        rebuild_requested: bool,
    ) -> DockRenderChange {
        DockRenderChange {
            state_changed: self.state.is_some_and(|state| state != current),
            visual_changed: self.visual_generation != current_visual_generation,
            dock_visibility_changed: self.dock_visibility != current.dock(),
            rebuild_requested,
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
            let change = baseline.change(
                self.dock_controller.state(),
                self.dock_controller.visual_generation(),
                actions_require_rebuild(actions),
            );
            if change.dock_visibility_changed {
                diagnostic_action = "visibility";
                let update = self.begin_dock_visibility(windows.dock);
                self.complete_surface_update(update, windows.surfaces())?;
                return self.sync_dock_animation(windows.dock);
            }
            let render_action = classify_dock_render_action(change);
            diagnostic_action = match render_action {
                DockRenderAction::None => "none",
                DockRenderAction::RedrawDock => "redraw",
                DockRenderAction::RebuildDockSurface => "rebuild-dock",
                DockRenderAction::RebuildAllSurfaces => "rebuild-all",
            };
            if matches!(render_action, DockRenderAction::None) {
                return Ok(());
            }
            if dock_width_measurement_required(change) {
                let desired_width = self.dock_controller.preferred_width_dip();
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
            }
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

    pub(super) fn redraw_dock(&mut self, windows: SurfaceWindows<'_>) -> Result<()> {
        let diagnostic_started =
            crate::diagnostics::enabled(crate::diagnostics::DiagnosticModule::DockPerformance)
                .then(Instant::now);
        let window_size = windows.dock.surface_physical_size();
        let scene_started = diagnostic_started.map(|_| Instant::now());
        self.dock_controller
            .update_surface(dip_surface(windows.dock));
        let dock_scene = self.dock_controller.scene();
        let scene_duration = scene_started.map_or(Duration::ZERO, |started| started.elapsed());
        let present_started = diagnostic_started.map(|_| Instant::now());
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
                    quick_settings: None,
                    context_menu: None,
                    settings: None,
                    preview: None,
                },
            },
            SurfaceSizeChange::Resize,
        );
        let present_duration = present_started.map_or(Duration::ZERO, |started| started.elapsed());
        let result = self.finish_dock_update(update, windows);
        record_slow_dock_redraw(
            diagnostic_started,
            scene_duration,
            present_duration,
            &result,
        );
        result
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
            quick_settings: None,
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
        match dock_update_disposition(update?) {
            DockUpdateDisposition::Presented => {
                self.pending_dock_redraw = false;
                if std::env::var_os("MINHA_UI_QA_TRACE").is_some() {
                    println!("{}", self.dock_controller.qa_trace_line());
                }
                Ok(())
            }
            DockUpdateDisposition::Retry => self.schedule_dock_redraw_retry(windows.dock),
            DockUpdateDisposition::Rebuild => {
                self.pending_dock_redraw = false;
                self.rebuild_native_surfaces(windows)
            }
        }
    }
}

const fn dock_width_measurement_required(change: DockRenderChange) -> bool {
    change.state_changed || change.rebuild_requested
}

#[cfg(test)]
const fn dock_qa_trace_required(update: Option<SurfaceUpdate>) -> bool {
    matches!(update, Some(SurfaceUpdate::Presented))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DockUpdateDisposition {
    Presented,
    Retry,
    Rebuild,
}

const fn dock_update_disposition(update: SurfaceUpdate) -> DockUpdateDisposition {
    match update {
        SurfaceUpdate::Presented => DockUpdateDisposition::Presented,
        SurfaceUpdate::FrameSkipped => DockUpdateDisposition::Retry,
        SurfaceUpdate::RebuildAllRequired => DockUpdateDisposition::Rebuild,
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

fn record_slow_dock_redraw(
    started: Option<Instant>,
    scene_duration: Duration,
    present_duration: Duration,
    result: &Result<()>,
) {
    let Some(started) = started else {
        return;
    };
    let duration = started.elapsed();
    if !crate::diagnostics::is_slow_dock_operation(duration) {
        return;
    }
    let duration_us = duration.as_micros().to_string();
    let scene_us = scene_duration.as_micros().to_string();
    let present_us = present_duration.as_micros().to_string();
    let outcome = if result.is_ok() {
        "succeeded"
    } else {
        "failed"
    };
    crate::diagnostics::record(
        crate::diagnostics::DiagnosticModule::DockPerformance,
        crate::diagnostics::LogLevel::Info,
        "dock.redraw.slow",
        &[
            ("duration_us", &duration_us),
            ("scene_us", &scene_us),
            ("present_us", &present_us),
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
    use crate::{DockRenderAction, DockRenderChange};
    use shell_core::ShellState;
    use shell_renderer::Dpi;
    use shell_renderer::native::SurfaceMetrics;

    use super::{
        DockUpdateDisposition, dock_qa_trace_required, dock_update_disposition,
        dock_width_measurement_required,
    };

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
        assert!(!dock_qa_trace_required(Some(SurfaceUpdate::FrameSkipped)));
        assert!(!dock_qa_trace_required(Some(
            SurfaceUpdate::RebuildAllRequired
        )));
        assert!(!dock_qa_trace_required(None));
    }

    #[test]
    fn dock_finish_policy_distinguishes_present_retry_and_recovery() {
        assert_eq!(
            dock_update_disposition(SurfaceUpdate::Presented),
            DockUpdateDisposition::Presented
        );
        assert_eq!(
            dock_update_disposition(SurfaceUpdate::FrameSkipped),
            DockUpdateDisposition::Retry
        );
        assert_eq!(
            dock_update_disposition(SurfaceUpdate::RebuildAllRequired),
            DockUpdateDisposition::Rebuild
        );
    }

    #[test]
    fn visual_only_frames_skip_the_expensive_dock_width_measurement() {
        let visual_only = DockRenderChange {
            state_changed: false,
            visual_changed: true,
            dock_visibility_changed: false,
            rebuild_requested: false,
        };
        assert_eq!(
            crate::classify_dock_render_action(visual_only),
            DockRenderAction::RedrawDock
        );
        assert!(!dock_width_measurement_required(visual_only));

        assert!(dock_width_measurement_required(DockRenderChange {
            state_changed: true,
            ..visual_only
        }));
    }

    #[test]
    fn visual_only_baseline_avoids_cloning_the_shell_state() {
        let state = ShellState::default();
        let baseline = super::DockRenderBaseline::visual_only(state.dock(), 7);

        assert_eq!(
            baseline.change(&state, 8, false),
            DockRenderChange {
                state_changed: false,
                visual_changed: true,
                dock_visibility_changed: false,
                rebuild_requested: false,
            }
        );
    }
}
