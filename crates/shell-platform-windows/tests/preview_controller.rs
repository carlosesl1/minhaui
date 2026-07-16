use crate::{PREVIEW_BRIDGE_MS, PREVIEW_DWELL_MS, PreviewController, PreviewEffect, PreviewPhase};
use shell_core::DockItemId;

#[test]
fn first_hover_opens_at_the_dwell_boundary() {
    // Given: a closed preview receives its first eligible dock target.
    let mut controller = PreviewController::new(false);
    assert_eq!(
        controller.dock_target_changed(Some(DockItemId::new(1)), 1_000),
        Vec::new()
    );

    // When: time advances to just before the dwell deadline.
    let early = controller.tick(1_000 + PREVIEW_DWELL_MS - 1);

    // Then: the preview stays in the dwelling phase.
    assert!(early.is_empty());
    assert!(matches!(controller.phase(), PreviewPhase::Dwelling { .. }));

    // When: time reaches the exact dwell deadline.
    let boundary = controller.tick(1_000 + PREVIEW_DWELL_MS);

    // Then: the preview is shown and the dock reveal is held.
    assert_eq!(
        boundary,
        vec![
            PreviewEffect::HoldDockReveal,
            PreviewEffect::Show {
                item: DockItemId::new(1),
                page: 0,
                from_keyboard: false,
            },
        ]
    );
    assert!(matches!(controller.phase(), PreviewPhase::Visible { .. }));
}

#[test]
fn first_hover_opens_within_the_responsive_dwell_budget() {
    assert_eq!(PREVIEW_DWELL_MS, 300);
    // Given: a closed preview receives its first eligible dock target.
    let mut controller = PreviewController::new(false);
    controller.dock_target_changed(Some(DockItemId::new(1)), 1_000);

    // When: the responsive 300 ms dwell budget has elapsed.
    let effects = controller.tick(1_300);

    // Then: the preview is already visible instead of adding avoidable latency.
    assert_eq!(
        effects,
        vec![
            PreviewEffect::HoldDockReveal,
            PreviewEffect::Show {
                item: DockItemId::new(1),
                page: 0,
                from_keyboard: false,
            },
        ]
    );
    assert!(matches!(controller.phase(), PreviewPhase::Visible { .. }));
}

#[test]
fn visible_preview_switches_dock_targets_immediately() {
    // Given: the preview for the first dock item is already visible.
    let mut controller = PreviewController::new(false);
    controller.open_from_keyboard(DockItemId::new(1), 10);

    // When: the pointer moves directly to another eligible dock item.
    let effects = controller.dock_target_changed(Some(DockItemId::new(2)), 20);

    // Then: content updates without another dwell or hide cycle.
    assert_eq!(
        effects,
        vec![PreviewEffect::Update {
            item: DockItemId::new(2),
            page: 0,
        }]
    );
    assert_eq!(controller.visible_item(), Some(DockItemId::new(2)));
}

#[test]
fn pointer_bridge_keeps_preview_open_until_its_deadline() {
    // Given: a visible pointer-opened preview loses the dock target.
    let mut controller = PreviewController::new(true);
    controller.open_from_keyboard(DockItemId::new(1), 100);
    controller.dock_target_changed(None, 200);

    // When: the bridge has not reached its deadline.
    let early = controller.tick(200 + PREVIEW_BRIDGE_MS - 1);

    // Then: no dismissal occurs.
    assert!(early.is_empty());
    assert!(matches!(controller.phase(), PreviewPhase::Visible { .. }));

    // When: the exact bridge deadline is reached.
    let boundary = controller.tick(200 + PREVIEW_BRIDGE_MS);

    // Then: reduced motion hides immediately and releases the dock.
    assert_eq!(
        boundary,
        vec![PreviewEffect::Hide, PreviewEffect::ReleaseDockReveal]
    );
    assert_eq!(controller.phase(), PreviewPhase::Closed);
}

#[test]
fn entering_preview_cancels_the_pointer_bridge() {
    // Given: a visible preview is inside the dock-to-preview bridge.
    let mut controller = PreviewController::new(false);
    controller.open_from_keyboard(DockItemId::new(1), 100);
    controller.dock_target_changed(None, 200);

    // When: the pointer enters the preview host before the deadline.
    controller.preview_entered();
    let effects = controller.tick(200 + PREVIEW_BRIDGE_MS);

    // Then: the preview remains visible.
    assert!(effects.is_empty());
    assert_eq!(controller.visible_item(), Some(DockItemId::new(1)));
}

#[test]
fn pointer_motion_inside_the_same_item_keeps_original_dwell_deadline() {
    let mut controller = PreviewController::new(false);
    controller.dock_target_changed(Some(DockItemId::new(1)), 1_000);
    controller.dock_target_changed(Some(DockItemId::new(1)), 1_300);

    let effects = controller.tick(1_000 + PREVIEW_DWELL_MS);

    assert!(effects.contains(&PreviewEffect::Show {
        item: DockItemId::new(1),
        page: 0,
        from_keyboard: false,
    }));
}
