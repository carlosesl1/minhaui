use crate::{
    PollBudget, QueuedTopbarAction, TopbarController, TopbarKey, TopbarPointerPhase,
    TopbarPointerSample, TopbarSnapshot,
};
use shell_core::{Popover, ShellState, TopbarModuleKind};
use shell_renderer::{DipPoint, DipRect, TopbarDensity};

#[test]
fn topbar_click_opens_typed_module_intent_without_rebuilding_resources()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a topbar controller with visible base modules.
    let mut controller = TopbarController::new(ShellState::default(), TopbarDensity::Comfortable)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 680.0, 40.0));
    controller.update_snapshot(TopbarSnapshot::privacy_safe_fixture());
    let network_point = module_center(&controller, TopbarModuleKind::Network)?;

    // When: the network module is clicked.
    controller.handle_pointer(TopbarPointerSample::new(
        TopbarPointerPhase::Pressed,
        network_point,
    ))?;
    let actions = controller.handle_pointer(TopbarPointerSample::new(
        TopbarPointerPhase::Released,
        network_point,
    ))?;

    // Then: a typed popover intent is dispatched and hover-only visual work is bounded.
    assert_eq!(
        actions,
        vec![QueuedTopbarAction::OpenPopover(Popover::Network)]
    );
    assert_eq!(controller.resource_generation(), 0);
    Ok(())
}

#[test]
fn status_polling_respects_budget_and_keeps_existing_snapshot_when_deferred()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a controller with no polling budget left.
    let mut controller = TopbarController::new(ShellState::default(), TopbarDensity::Compact)?;
    controller.update_snapshot(TopbarSnapshot::privacy_safe_fixture());
    let before = controller.snapshot().clone();

    // When: a status refresh is requested before the minimum interval.
    let action = controller.refresh_status(PollBudget::new(1_000, 500), 1_100);

    // Then: no adapter polling occurs and the privacy-safe snapshot remains stable.
    assert_eq!(action, QueuedTopbarAction::PollDeferred);
    assert_eq!(controller.snapshot(), &before);
    Ok(())
}

#[test]
fn topbar_keyboard_focus_opens_visible_modules_without_pointer_input()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a topbar controller with keyboard focus on no module yet.
    let mut controller = TopbarController::new(ShellState::default(), TopbarDensity::Comfortable)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 680.0, 40.0));
    controller.update_snapshot(TopbarSnapshot::privacy_safe_fixture());

    // When: keyboard navigation reaches the network module and activates it.
    assert_eq!(
        controller.handle_key(TopbarKey::Next)?,
        vec![QueuedTopbarAction::RedrawTopbar]
    );
    assert_eq!(
        controller.handle_key(TopbarKey::Next)?,
        vec![QueuedTopbarAction::RedrawTopbar]
    );
    let actions = controller.handle_key(TopbarKey::Activate)?;

    // Then: the focused module is visible in the scene and opens a typed popover.
    assert_eq!(
        controller.scene().focused_module(),
        Some(TopbarModuleKind::Network)
    );
    assert_eq!(
        actions,
        vec![QueuedTopbarAction::OpenPopover(Popover::Network)]
    );

    // When: Escape is pressed.
    assert_eq!(
        controller.handle_key(TopbarKey::Escape)?,
        vec![QueuedTopbarAction::RedrawTopbar]
    );

    // Then: keyboard focus is cleared.
    assert_eq!(controller.scene().focused_module(), None);
    Ok(())
}

#[test]
fn topbar_uses_windows_symbol_glyphs_instead_of_placeholder_words()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: the default topbar modules are composed for native rendering.
    let controller = TopbarController::new(ShellState::default(), TopbarDensity::Compact)?;

    // When: the scene exposes its visual modules.
    let scene = controller.scene();

    // Then: every icon is a single non-ASCII Windows symbol glyph.
    assert!(
        scene
            .modules()
            .iter()
            .all(|module| { module.icon().chars().count() == 1 && !module.icon().is_ascii() })
    );
    Ok(())
}

fn module_center(
    controller: &TopbarController,
    kind: TopbarModuleKind,
) -> Result<DipPoint, Box<dyn std::error::Error>> {
    let layout = shell_renderer::layout_topbar_scene(
        &controller.scene(),
        DipRect::new(0.0, 0.0, 680.0, 40.0),
    );
    let item = layout
        .visible_items()
        .iter()
        .find(|item| item.kind() == kind)
        .ok_or("topbar module was not laid out")?;
    let bounds = item.bounds();
    Ok(DipPoint::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    ))
}
