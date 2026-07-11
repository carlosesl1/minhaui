use shell_core::{AppId, DockItem, DockItemId, Effect, ShellState};
use shell_platform_windows::{
    ContextMenuCommand, DockController, DockPointerPhase, DockPointerSample, DockRuntimeConfig,
    QueuedDockAction,
};
use shell_renderer::{DipPoint, DipRect, DockAlignment};

fn app(value: &str) -> Result<AppId, Box<dyn std::error::Error>> {
    Ok(AppId::parse(value)?)
}

fn state() -> Result<ShellState, Box<dyn std::error::Error>> {
    Ok(ShellState::default().with_dock_items(vec![
        DockItem::pinned(DockItemId::new(1), app("app.notepad")?),
        DockItem::pinned(DockItemId::new(2), app("app.calculator")?),
    ]))
}

#[test]
fn primary_click_queues_launch_focus_or_minimize_without_waiting_for_animation()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a dock controller with a stopped pinned app under the pointer.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 96.0));

    // When: the user presses and releases the first item.
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        DipPoint::new(92.0, 48.0),
    ))?;
    let actions = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        DipPoint::new(92.0, 48.0),
    ))?;

    // Then: launch is queued immediately and animation time remains external.
    assert_eq!(
        actions,
        vec![QueuedDockAction::Effect(Effect::Launch(app(
            "app.notepad"
        )?))]
    );
    assert!(controller.animator().is_idle());
    Ok(())
}

#[test]
fn dragging_an_item_reorders_the_persisted_state() -> Result<(), Box<dyn std::error::Error>> {
    // Given: two pinned dock apps.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 96.0));

    // When: the second app is dragged before the first.
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        DipPoint::new(150.0, 48.0),
    ))?;
    let actions = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        DipPoint::new(88.0, 48.0),
    ))?;

    // Then: the controller emits a persisted reorder action.
    assert_eq!(
        actions,
        vec![QueuedDockAction::Effect(Effect::PersistConfiguration)]
    );
    assert_eq!(controller.state().dock_items()[0].id(), DockItemId::new(2));
    Ok(())
}

#[test]
fn context_menu_and_file_drop_pin_existing_windows_without_private_shell_hooks()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a dock controller with a public context-menu command and file drop.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 96.0));

    // When: unpin is selected and an executable is dropped.
    let unpin =
        controller.handle_context_menu(DipPoint::new(92.0, 48.0), ContextMenuCommand::Unpin)?;
    let drop = controller.handle_drop(
        DipPoint::new(250.0, 48.0),
        "C:\\Windows\\System32\\mspaint.exe",
    )?;

    // Then: both operations persist through the reducer.
    assert_eq!(
        unpin,
        vec![QueuedDockAction::Effect(Effect::PersistConfiguration)]
    );
    assert!(drop.contains(&QueuedDockAction::Effect(Effect::PersistConfiguration)));
    assert!(
        controller
            .state()
            .dock_items()
            .iter()
            .any(|item| item.app().as_str() == "mspaint.exe")
    );
    Ok(())
}

#[test]
fn autohide_reveal_zone_and_alignment_are_runtime_configurable()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a right-aligned autohide dock.
    let config = DockRuntimeConfig::new(DockAlignment::Right)
        .with_item_size(44.0)
        .with_spacing(4.0)
        .with_autohide(true)
        .with_reveal_zone_height(9.0);
    let mut controller = DockController::new(state()?, config)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 96.0));

    // When: the pointer leaves, then enters the bottom reveal zone.
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Exited,
        DipPoint::new(10.0, 10.0),
    ))?;
    let reveal = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Moved,
        DipPoint::new(300.0, 92.0),
    ))?;

    // Then: reveal is handled as state, not by blocking animation.
    assert!(controller.state().dock().is_revealed());
    assert!(reveal.is_empty());
    assert_eq!(controller.config().alignment(), DockAlignment::Right);
    Ok(())
}
