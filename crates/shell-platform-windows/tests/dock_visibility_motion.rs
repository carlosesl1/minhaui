use crate::DockVisibilityMotion;
use shell_renderer::PhysicalRect;

#[test]
fn reveal_eases_out_between_hidden_and_visible_positions() {
    let hidden = PhysicalRect::new(100, 887, 720, 96);
    let visible = PhysicalRect::new(100, 800, 720, 96);
    let mut motion = DockVisibilityMotion::new(hidden, visible, 220);

    assert_eq!(motion.rect(), hidden);
    motion.advance(0.11);
    assert!(motion.rect().y < 843);
    assert_eq!(motion.rect().height, visible.height);
    motion.advance(0.11);
    assert_eq!(motion.rect(), visible);
    assert!(motion.finished());
}

#[test]
fn hide_eases_in_and_can_restart_from_an_intermediate_position() {
    let visible = PhysicalRect::new(100, 800, 720, 96);
    let hidden = PhysicalRect::new(100, 887, 720, 96);
    let mut hiding = DockVisibilityMotion::new(visible, hidden, 180);
    hiding.advance(0.09);
    let intermediate = hiding.rect();

    assert!(intermediate.y < 844);
    assert_eq!(intermediate.height, visible.height);

    let mut reversing = DockVisibilityMotion::new(intermediate, visible, 220);
    assert_eq!(reversing.rect(), intermediate);
    reversing.advance(0.22);
    assert_eq!(reversing.rect(), visible);
}

#[test]
fn non_finite_frame_delta_does_not_keep_the_motion_busy() {
    let visible = PhysicalRect::new(100, 800, 720, 96);
    let hidden = PhysicalRect::new(100, 887, 720, 96);
    let mut motion = DockVisibilityMotion::new(visible, hidden, 180);

    assert!(!motion.advance(f32::NAN));
    assert_eq!(motion.rect(), visible);
}

#[test]
fn compositor_motion_defers_the_hwnd_commit_until_the_transition_finishes() {
    // Given: a hide transition whose visual is animated by DirectComposition.
    let visible = PhysicalRect::new(100, 800, 720, 96);
    let hidden = PhysicalRect::new(100, 887, 720, 96);
    let mut motion = DockVisibilityMotion::new(visible, hidden, 180);

    // When: the compositor transition is only halfway complete.
    let halfway = motion.advance_to_completion(0.09);

    // Then: the HWND remains at its stable base until the compositor is done.
    assert_eq!(halfway, None);
    assert_eq!(motion.target_offset_y(visible), 87.0);

    // When: the remaining duration elapses.
    let completed = motion.advance_to_completion(0.09);

    // Then: the final HWND placement and opacity are committed once.
    assert_eq!(completed, Some((hidden, 0.0)));
}
