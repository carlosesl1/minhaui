use shell_core::{Popover, ShellState, TopbarModuleKind};
use shell_platform_windows::{
    PollBudget, QueuedTopbarAction, TopbarController, TopbarPointerPhase, TopbarPointerSample,
    TopbarSnapshot,
};
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
