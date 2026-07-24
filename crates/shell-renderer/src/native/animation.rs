use windows::core::Result;

use super::WindowSurface;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceVisibilityAnimation {
    pub start_offset_y: f32,
    pub target_offset_y: f32,
    pub start_opacity: f32,
    pub target_opacity: f32,
    pub duration_seconds: f32,
}

impl WindowSurface {
    pub fn animate_visibility(&self, spec: SurfaceVisibilityAnimation) -> Result<()> {
        let attachment = &self.composition;
        let duration = spec.duration_seconds.max(0.001);
        let offset = smoothstep_coefficients(spec.start_offset_y, spec.target_offset_y, duration);
        let opacity = smoothstep_coefficients(spec.start_opacity, spec.target_opacity, duration);

        // SAFETY: Category 8 (FFI boundary). The composition device returns two
        // owned animations whose coefficients are finite and duration is positive.
        let offset_animation = unsafe { attachment.dcomp.CreateAnimation() }?;
        // SAFETY: Category 8 (FFI boundary). The same live device creates the
        // independent opacity channel used by this surface's effect group.
        let opacity_animation = unsafe { attachment.dcomp.CreateAnimation() }?;
        // SAFETY: Category 8 (FFI boundary). Both animations and targets belong to
        // this live composition device and all values form bounded cubic segments.
        unsafe {
            offset_animation.AddCubic(0.0, offset[0], offset[1], offset[2], offset[3])?;
            offset_animation.End(f64::from(duration), spec.target_offset_y)?;
            opacity_animation.AddCubic(0.0, opacity[0], opacity[1], opacity[2], opacity[3])?;
            opacity_animation.End(f64::from(duration), spec.target_opacity)?;
            attachment.visual.SetOffsetY(&offset_animation)?;
            attachment.opacity_effect.SetOpacity(&opacity_animation)?;
            attachment.dcomp.Commit()
        }
    }

    pub fn set_visibility_state(&self, offset_y: f32, opacity: f32) -> Result<()> {
        self.composition.set_visibility_state(offset_y, opacity)
    }
}

fn smoothstep_coefficients(start: f32, target: f32, duration: f32) -> [f32; 4] {
    let delta = target - start;
    [
        start,
        0.0,
        3.0 * delta / duration.powi(2),
        -2.0 * delta / duration.powi(3),
    ]
}

#[cfg(test)]
mod tests {
    use super::smoothstep_coefficients;

    #[test]
    fn smoothstep_channel_reaches_both_endpoints_with_zero_edge_velocity() {
        let duration = 0.2;
        let coefficients = smoothstep_coefficients(8.0, 0.0, duration);
        let at_end = coefficients[0]
            + coefficients[1] * duration
            + coefficients[2] * duration.powi(2)
            + coefficients[3] * duration.powi(3);
        let end_velocity = coefficients[1]
            + 2.0 * coefficients[2] * duration
            + 3.0 * coefficients[3] * duration.powi(2);

        assert_eq!(coefficients[0], 8.0);
        assert!(at_end.abs() < 0.001);
        assert_eq!(coefficients[1], 0.0);
        assert!(end_velocity.abs() < 0.001);
    }
}
