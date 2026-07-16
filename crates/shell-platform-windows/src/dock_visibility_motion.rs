#![deny(unsafe_code)]

use shell_renderer::PhysicalRect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockVisibilityMotion {
    start: PhysicalRect,
    target: PhysicalRect,
    start_opacity: f32,
    target_opacity: f32,
    progress: f32,
    duration_seconds: f32,
}

impl DockVisibilityMotion {
    #[must_use]
    #[cfg(test)]
    pub const fn new(start: PhysicalRect, target: PhysicalRect, duration_ms: u64) -> Self {
        let hiding = target.y > start.y;
        Self::new_with_opacity(
            start,
            target,
            duration_ms,
            if hiding { 1.0 } else { 0.0 },
            if hiding { 0.0 } else { 1.0 },
        )
    }

    #[must_use]
    pub const fn new_with_opacity(
        start: PhysicalRect,
        target: PhysicalRect,
        duration_ms: u64,
        start_opacity: f32,
        target_opacity: f32,
    ) -> Self {
        Self {
            start,
            target,
            start_opacity,
            target_opacity,
            progress: 0.0,
            duration_seconds: if duration_ms == 0 {
                0.001
            } else {
                duration_ms as f32 / 1_000.0
            },
        }
    }

    #[must_use]
    pub fn opacity(self) -> f32 {
        let progress = self.progress * self.progress * (3.0 - 2.0 * self.progress);
        (self.start_opacity + (self.target_opacity - self.start_opacity) * progress).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn rect(self) -> PhysicalRect {
        let eased = if self.target.y < self.start.y {
            1.0 - (1.0 - self.progress).powi(3)
        } else {
            self.progress.powi(3)
        };
        PhysicalRect::new(
            interpolate(self.start.x, self.target.x, eased),
            interpolate(self.start.y, self.target.y, eased),
            interpolate(self.start.width, self.target.width, eased),
            interpolate(self.start.height, self.target.height, eased),
        )
    }

    pub fn advance(&mut self, delta_seconds: f32) -> bool {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 || self.finished() {
            return false;
        }
        let previous = self.progress;
        self.progress = (self.progress + delta_seconds / self.duration_seconds).clamp(0.0, 1.0);
        self.progress != previous
    }

    pub fn advance_to_completion(&mut self, delta_seconds: f32) -> Option<(PhysicalRect, f32)> {
        self.advance(delta_seconds);
        self.finished()
            .then_some((self.target, self.target_opacity))
    }

    #[must_use]
    pub fn target_offset_y(self, base: PhysicalRect) -> f32 {
        (self.target.y - base.y) as f32
    }

    #[must_use]
    pub fn current_offset_y(self, base: PhysicalRect) -> f32 {
        (self.rect().y - base.y) as f32
    }

    #[must_use]
    pub const fn target_opacity(self) -> f32 {
        self.target_opacity
    }

    #[must_use]
    pub fn remaining_duration_seconds(self) -> f32 {
        (self.duration_seconds * (1.0 - self.progress)).max(0.0)
    }

    #[must_use]
    #[expect(
        dead_code,
        reason = "retained for reconstructing an interrupted native animation"
    )]
    pub const fn target(self) -> PhysicalRect {
        self.target
    }

    #[must_use]
    pub const fn finished(self) -> bool {
        self.progress >= 1.0
    }
}

fn interpolate(start: i32, target: i32, progress: f32) -> i32 {
    (start as f32 + (target - start) as f32 * progress).round() as i32
}

#[cfg(test)]
mod tests {
    use shell_renderer::PhysicalRect;

    use super::DockVisibilityMotion;

    #[test]
    fn hiding_fades_the_visual_out_before_the_reveal_strip_remains() {
        let visible = PhysicalRect::new(100, 900, 420, 55);
        let hidden = PhysicalRect::new(100, 1072, 420, 55);
        let mut motion = DockVisibilityMotion::new(visible, hidden, 180);

        assert_eq!(motion.opacity(), 1.0);
        assert!(motion.advance(0.09));
        assert!(motion.opacity() < 1.0);
        assert!(motion.advance(0.09));
        assert_eq!(motion.opacity(), 0.0);
    }

    #[test]
    fn revealing_fades_the_visual_in_from_a_transparent_reveal_strip() {
        let hidden = PhysicalRect::new(100, 1072, 420, 55);
        let visible = PhysicalRect::new(100, 900, 420, 55);
        let mut motion = DockVisibilityMotion::new(hidden, visible, 220);

        assert_eq!(motion.opacity(), 0.0);
        assert!(motion.advance(0.11));
        assert!(motion.opacity() > 0.0);
        assert!(motion.advance(0.11));
        assert_eq!(motion.opacity(), 1.0);
    }

    #[test]
    fn rebuild_snapshot_exposes_current_target_and_remaining_motion() {
        let visible = PhysicalRect::new(100, 900, 420, 55);
        let hidden = PhysicalRect::new(100, 1072, 420, 55);
        let mut motion = DockVisibilityMotion::new(visible, hidden, 180);

        assert!(motion.advance(0.09));

        assert_eq!(
            motion.current_offset_y(visible),
            (motion.rect().y - visible.y) as f32
        );
        assert_eq!(motion.target_offset_y(visible), 172.0);
        assert_eq!(motion.target_opacity(), 0.0);
        assert!((motion.remaining_duration_seconds() - 0.09).abs() < f32::EPSILON);
    }
}
