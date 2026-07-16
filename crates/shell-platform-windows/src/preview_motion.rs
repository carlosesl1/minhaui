#![deny(unsafe_code)]

use shell_renderer::PhysicalRect;

pub(crate) const PREVIEW_ENTRANCE_DURATION_MS: u64 = 200;

pub(crate) const fn preview_reveal_state(first_visible: f32) -> (bool, f32, f32) {
    (true, 0.0, first_visible)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewMotionSpec {
    pub started_ms: u64,
    pub duration_ms: u64,
    pub offset_px: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewEntranceMotion {
    target: PhysicalRect,
    spec: PreviewMotionSpec,
}

impl PreviewEntranceMotion {
    #[must_use]
    pub const fn new(target: PhysicalRect, spec: PreviewMotionSpec) -> Self {
        Self { target, spec }
    }

    #[must_use]
    pub fn rect_at(self, now_ms: u64) -> PhysicalRect {
        let remaining = 1.0 - self.opacity_at(now_ms);
        let offset = (self.spec.offset_px as f32 * remaining).round() as i32;
        PhysicalRect::new(
            self.target.x,
            self.target.y + offset,
            self.target.width,
            self.target.height,
        )
    }

    #[must_use]
    pub fn opacity_at(self, now_ms: u64) -> f32 {
        let elapsed = now_ms.saturating_sub(self.spec.started_ms);
        let duration = self.spec.duration_ms.max(1);
        let progress = (elapsed as f32 / duration as f32).clamp(0.0, 1.0);
        standard_ease(progress)
    }

    #[must_use]
    pub const fn finished(self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.spec.started_ms) >= self.spec.duration_ms
    }
}

fn standard_ease(progress: f32) -> f32 {
    if progress <= 0.0 {
        return 0.0;
    }
    if progress >= 1.0 {
        return 1.0;
    }

    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..12 {
        let parameter = (lower + upper) * 0.5;
        if cubic_bezier_component(parameter, 0.16, 0.3) < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    cubic_bezier_component((lower + upper) * 0.5, 1.0, 1.0)
}

fn cubic_bezier_component(parameter: f32, first: f32, second: f32) -> f32 {
    let remaining = 1.0 - parameter;
    3.0 * remaining * remaining * parameter * first
        + 3.0 * remaining * parameter * parameter * second
        + parameter * parameter * parameter
}

#[cfg(test)]
mod tests {
    use super::preview_reveal_state;

    #[test]
    fn live_preview_warms_up_invisibly_before_its_first_composed_frame() {
        let (cloaked, warmup, first_visible) = preview_reveal_state(0.48);

        assert!(cloaked, "the DWM frame must stay hidden during warmup");
        assert_eq!(warmup, 0.0);
        assert_eq!(first_visible, 0.48);
    }
}
