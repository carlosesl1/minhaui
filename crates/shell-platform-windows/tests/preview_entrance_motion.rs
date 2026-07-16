use crate::{PreviewEntranceMotion, PreviewMotionSpec};
use shell_renderer::PhysicalRect;

#[test]
fn preview_entrance_combines_a_gentle_rise_with_a_smooth_fade() {
    let target = PhysicalRect::new(100, 200, 344, 252);
    let motion = PreviewEntranceMotion::new(
        target,
        PreviewMotionSpec {
            started_ms: 1_000,
            duration_ms: 280,
            offset_px: 6,
        },
    );

    assert_eq!(motion.rect_at(1_000).y, 206);
    assert_eq!(motion.rect_at(1_140).y, 200);
    assert_eq!(motion.rect_at(1_280), target);
    assert_eq!(motion.opacity_at(1_000), 0.0);
    assert!((motion.opacity_at(1_140) - 0.972).abs() < 0.01);
    assert_eq!(motion.opacity_at(1_280), 1.0);
    assert!(!motion.finished(1_279));
    assert!(motion.finished(1_280));
}

#[test]
fn preview_entrance_responds_with_a_fast_out_curve() {
    // Given: a preview entrance using the standard 200 ms motion token.
    let motion = PreviewEntranceMotion::new(
        PhysicalRect::new(100, 200, 344, 252),
        PreviewMotionSpec {
            started_ms: 1_000,
            duration_ms: 200,
            offset_px: 4,
        },
    );

    // When: only the first quarter of the entrance has elapsed.
    let early_opacity = motion.opacity_at(1_050);
    let early_rect = motion.rect_at(1_050);

    // Then: the preview is already legible and close to its target position.
    assert!((early_opacity - 0.826).abs() < 0.01);
    assert!(motion.opacity_at(1_016) > 0.4);
    assert!(early_rect.y <= 202);
    assert!(!motion.finished(1_199));
    assert!(motion.finished(1_200));
}
