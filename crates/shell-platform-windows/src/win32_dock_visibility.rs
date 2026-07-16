#![deny(unsafe_code)]

use shell_renderer::PhysicalRect;
use shell_renderer::native::{ShowcaseRole, SurfaceVisibilityAnimation};
use std::time::Duration;
use windows::core::Result;

use crate::win32_owner::RuntimeSurfaces;
use crate::win32_surface_runtime::SurfaceUpdate;
use crate::win32_window::OwnedWindow;
use crate::{DockRuntimeConfig, DockVisibilityMotion};

const DOCK_REVEAL_DURATION_MS: u64 = 220;
const DOCK_HIDE_DURATION_MS: u64 = 180;

impl RuntimeSurfaces {
    pub(super) fn begin_dock_visibility(
        &mut self,
        dock: &mut OwnedWindow,
    ) -> Result<SurfaceUpdate> {
        let work = crate::win32_windowing::window_work_area(dock.hwnd)?;
        let (config, hidden) = crate::resolve_dock_visibility(
            self.dock_controller.config(),
            self.dock_controller.state().dock().is_revealed(),
            self.fullscreen_suppressed,
        );
        let target = dock.dock_visibility_target(work, config, hidden)?;
        let target_opacity = if hidden { 0.0 } else { 1.0 };
        let (start, start_opacity) = self
            .dock_visibility_motion
            .map_or((dock.rect, self.dock_visibility_opacity), |motion| {
                (motion.rect(), motion.opacity())
            });
        if self.reduced_motion || start == target {
            self.dock_visibility_motion = None;
            self.dock_visibility_opacity = target_opacity;
            dock.place_dock_motion(target)?;
            return if self.surface_runtime.size(ShowcaseRole::Dock).is_some() {
                self.surface_runtime
                    .set_visibility_state(ShowcaseRole::Dock, 0.0, target_opacity)
            } else {
                Ok(SurfaceUpdate::Presented)
            };
        }
        let duration = if hidden {
            DOCK_HIDE_DURATION_MS
        } else {
            DOCK_REVEAL_DURATION_MS
        };
        let motion = DockVisibilityMotion::new_with_opacity(
            start,
            target,
            duration,
            start_opacity,
            target_opacity,
        );
        let base = dock.dock_visibility_target(work, config, false)?;
        if self.surface_runtime.size(ShowcaseRole::Dock).is_none() {
            dock.place_dock_motion(target)?;
            self.dock_visibility_opacity = target_opacity;
            self.dock_visibility_motion = None;
            return Ok(SurfaceUpdate::Presented);
        }
        dock.place_dock_motion(base)?;
        self.dock_visibility_opacity = start_opacity;
        self.dock_visibility_motion = Some(motion);
        self.surface_runtime.animate_visibility(
            ShowcaseRole::Dock,
            SurfaceVisibilityAnimation {
                start_offset_y: (start.y - base.y) as f32,
                target_offset_y: motion.target_offset_y(base),
                start_opacity,
                target_opacity,
                duration_seconds: Duration::from_millis(duration).as_secs_f32(),
            },
        )
    }

    pub(super) fn snap_dock_visibility(
        &mut self,
        dock: &mut OwnedWindow,
        work: PhysicalRect,
        config: DockRuntimeConfig,
        hidden: bool,
    ) -> Result<()> {
        self.dock_visibility_motion = None;
        self.dock_visibility_opacity = if hidden { 0.0 } else { 1.0 };
        dock.apply_dock_visibility(work, config, hidden)?;
        Ok(())
    }

    pub(super) fn advance_dock_visibility(
        &mut self,
        dock: &mut OwnedWindow,
        delta_seconds: f32,
    ) -> Result<SurfaceUpdate> {
        let Some(motion) = self.dock_visibility_motion.as_mut() else {
            return Ok(SurfaceUpdate::Presented);
        };
        let completed = motion.advance_to_completion(delta_seconds);
        self.dock_visibility_opacity = motion.opacity();
        if let Some((target, target_opacity)) = completed {
            dock.place_dock_motion(target)?;
            self.dock_visibility_opacity = target_opacity;
            self.dock_visibility_motion = None;
            if self.surface_runtime.size(ShowcaseRole::Dock).is_some() {
                return self.surface_runtime.set_visibility_state(
                    ShowcaseRole::Dock,
                    0.0,
                    target_opacity,
                );
            }
        }
        Ok(SurfaceUpdate::Presented)
    }
}
