use shell_core::{AppId, DockItem, DockItemId, Effect, ShellState, WindowId};
use shell_platform_windows::{
    ContextMenuCommand, DockController, DockPointerPhase, DockPointerSample, DockRuntimeConfig,
    ObservedWindow, QueuedDockAction,
};
use shell_renderer::{DipPoint, DipRect, DockItemVisualKind};

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
fn pointer_move_sets_hovered_item_for_magnified_scene_and_exit_clears_it()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a dock controller with a surface large enough for two apps.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 96.0));
    let first_item = controller
        .scene()
        .items()
        .iter()
        .find(|item| item.id() == 1)
        .expect("first item should be visible")
        .id();

    // When: the pointer moves over the first app.
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Moved,
        item_center(
            &controller,
            DockItemId::new(first_item),
            DipRect::new(0.0, 0.0, 320.0, 96.0),
        )?,
    ))?;

    // Then: the scene records hover and the hovered bounds are magnified.
    let scene = controller.scene();
    let layout = shell_renderer::layout_dock_scene(&scene, DipRect::new(0.0, 0.0, 320.0, 96.0));
    assert_eq!(scene.hovered_item(), Some(first_item));
    assert!(
        layout.items()[0].bounds().width > controller.config().layout().item_size(),
        "hovered item should be larger than the base dock size",
    );

    // When: the pointer exits the dock.
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Exited,
        DipPoint::new(-1.0, -1.0),
    ))?;

    // Then: hover state is cleared from the next scene.
    assert_eq!(controller.scene().hovered_item(), None);
    Ok(())
}

#[test]
fn drop_preserves_launchable_path_with_spaces_separate_from_identity()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a dropped executable path containing spaces.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 420.0, 96.0));
    let dropped_path = "C:\\Program Files\\Minha UI QA\\Notepad Copy.exe";

    // When: the file is pinned through the drop path and then clicked.
    controller.handle_drop(DipPoint::new(250.0, 48.0), dropped_path)?;
    let dropped = controller
        .state()
        .dock_items()
        .last()
        .expect("dropped item should be present");
    let dropped_id = dropped.id();
    let dropped_app = dropped.app().as_str().to_owned();
    let launch_target = controller
        .launch_target_for_item(dropped_id)
        .expect("dropped item should have a launch target")
        .to_owned();
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        item_center(&controller, dropped_id, DipRect::new(0.0, 0.0, 420.0, 96.0))?,
    ))?;
    let actions = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        item_center(&controller, dropped_id, DipRect::new(0.0, 0.0, 420.0, 96.0))?,
    ))?;

    // Then: identity is parse-safe while the queued launch keeps the original path.
    assert_eq!(dropped_app, "notepad-copy.exe");
    assert!(launch_target.ends_with("Program Files\\Minha UI QA\\Notepad Copy.exe"));
    assert_eq!(actions, vec![QueuedDockAction::Launch(launch_target)]);
    Ok(())
}

#[test]
fn separators_split_running_apps_and_are_not_interactive() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a discovered unpinned app creates a visual separator after pinned apps.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 360.0, 96.0));
    controller.sync_running_windows(&[ObservedWindow::new(
        WindowId::new(777),
        app("mspaint.exe")?,
        false,
        false,
    )])?;
    let scene = controller.scene();
    let separator = scene
        .items()
        .iter()
        .find(|item| item.kind() == DockItemVisualKind::Separator)
        .expect("separator should split pinned and unpinned apps");
    let separator_center = item_center(
        &controller,
        DockItemId::new(separator.id()),
        DipRect::new(0.0, 0.0, 360.0, 96.0),
    )?;

    // When: the separator is clicked and used as a drag target.
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        separator_center,
    ))?;
    let click = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        separator_center,
    ))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        item_center(
            &controller,
            DockItemId::new(2),
            DipRect::new(0.0, 0.0, 360.0, 96.0),
        )?,
    ))?;
    let drag = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        separator_center,
    ))?;

    // Then: the separator never activates and drag ordering ignores it as an app id.
    assert!(click.is_empty());
    assert_eq!(
        drag,
        vec![QueuedDockAction::Effect(Effect::PersistConfiguration)]
    );
    assert_ne!(
        controller.state().dock_items()[0].id(),
        DockItemId::new(separator.id())
    );
    Ok(())
}

fn item_center(
    controller: &DockController,
    id: DockItemId,
    surface: DipRect,
) -> Result<DipPoint, Box<dyn std::error::Error>> {
    let scene = controller.scene();
    let layout = shell_renderer::layout_dock_scene(&scene, surface);
    let item = layout
        .items()
        .iter()
        .find(|item| item.id() == id.value())
        .ok_or("dock item was not laid out")?;
    let bounds = item.bounds();
    Ok(DipPoint::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    ))
}
