use crate::{
    ContextMenuCommand, DockAnimator, DockContextMenuItem, DockController, DockKey,
    DockPointerPhase, DockPointerSample, DockRuntimeConfig, ObservedWindow, QueuedDockAction,
};
use shell_core::{
    AppId, DockItem, DockItemId, DockLayoutEntry, Effect, PinState, ShellState, WindowId,
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
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let second = item_center(&controller, DockItemId::new(2), surface)?;
    let mut before_first = item_center(&controller, DockItemId::new(1), surface)?;
    before_first.x -= 1.0;

    // When: the second app is dragged before the first.
    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Pressed, second))?;
    let preview_actions = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        before_first,
    ))?;

    assert!(preview_actions.is_empty());
    assert_eq!(controller.state().dock_items()[0].id(), DockItemId::new(1));
    assert_eq!(controller.scene().items()[0].id(), 2);

    let actions = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        before_first,
    ))?;

    assert_eq!(
        actions,
        vec![QueuedDockAction::Effect(Effect::PersistConfiguration)]
    );
    assert_eq!(controller.state().dock_items()[0].id(), DockItemId::new(2));
    Ok(())
}

#[test]
fn neighboring_slots_glide_to_their_new_positions_during_reorder()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let second = item_center(&controller, DockItemId::new(2), surface)?;
    let mut before_first = item_center(&controller, DockItemId::new(1), surface)?;
    before_first.x -= 1.0;

    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Pressed, second))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        before_first,
    ))?;
    let moving_x = shell_renderer::layout_dock_scene(&controller.scene(), surface)
        .items()
        .iter()
        .find(|item| item.id() == 1)
        .expect("neighbor should remain visible")
        .bounds()
        .x;

    assert!(!controller.animations_idle());
    controller.snap_animation_to_target();
    let settled_x = shell_renderer::layout_dock_scene(&controller.scene(), surface)
        .items()
        .iter()
        .find(|item| item.id() == 1)
        .expect("neighbor should remain visible")
        .bounds()
        .x;
    assert!((moving_x - settled_x).abs() > 20.0);
    assert!(controller.animations_idle());
    Ok(())
}

#[test]
fn off_dock_release_is_invalid_and_restores_the_original_order()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let second = item_center(&controller, DockItemId::new(2), surface)?;
    let first = item_center(&controller, DockItemId::new(1), surface)?;

    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Pressed, second))?;
    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Dragged, first))?;
    assert!(controller.scene().drag_target_valid());
    assert!(controller.scene().drag_insertion_x().is_some());
    let actions = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        DipPoint::new(first.x, 160.0),
    ))?;

    assert!(actions.is_empty());
    assert_eq!(controller.state().dock_items()[0].id(), DockItemId::new(1));
    Ok(())
}

#[test]
fn context_menu_reorders_apps_without_pointer_drag() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let second = item_center(&controller, DockItemId::new(2), surface)?;

    let actions = controller.handle_context_menu(second, ContextMenuCommand::MoveLeft)?;

    assert_eq!(
        actions,
        vec![QueuedDockAction::Effect(Effect::PersistConfiguration)]
    );
    assert_eq!(controller.state().dock_items()[0].id(), DockItemId::new(2));
    Ok(())
}

#[test]
fn short_pointer_motion_remains_a_click_instead_of_starting_reorder()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let first = item_center(&controller, DockItemId::new(1), surface)?;

    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Pressed, first))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        DipPoint::new(first.x + 2.0, first.y),
    ))?;
    let actions = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        DipPoint::new(first.x + 2.0, first.y),
    ))?;

    assert!(matches!(actions.as_slice(), [QueuedDockAction::Launch(_)]));
    assert_eq!(controller.state().dock_items()[0].id(), DockItemId::new(1));
    Ok(())
}

#[test]
fn custom_separators_can_be_added_dragged_removed_and_cancelled()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 360.0, 96.0);
    controller.update_surface(surface);
    let first = item_center(&controller, DockItemId::new(1), surface)?;

    let added = controller.handle_context_menu(first, ContextMenuCommand::AddSeparator)?;
    assert_eq!(
        added,
        vec![QueuedDockAction::Effect(Effect::PersistConfiguration)]
    );
    let separator = controller
        .state()
        .dock_layout()
        .iter()
        .find_map(|entry| match entry {
            DockLayoutEntry::Separator(id) => Some(*id),
            DockLayoutEntry::App(_) => None,
        })
        .ok_or("separator should be added")?;
    let separator_center = visual_center(&controller, separator.value(), surface)?;
    let second = item_center(&controller, DockItemId::new(2), surface)?;

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        separator_center,
    ))?;
    assert!(
        controller
            .handle_pointer(DockPointerSample::new(
                DockPointerPhase::Dragged,
                DipPoint::new(second.x + 80.0, second.y),
            ))?
            .is_empty()
    );
    assert_eq!(
        controller.scene().items().last().map(|item| item.id()),
        Some(separator.value())
    );
    assert!(
        controller
            .handle_pointer(DockPointerSample::new(
                DockPointerPhase::Cancelled,
                DipPoint::new(second.x + 80.0, second.y),
            ))?
            .is_empty()
    );
    assert_ne!(
        controller.scene().items().last().map(|item| item.id()),
        Some(separator.value())
    );

    let separator_center = visual_center(&controller, separator.value(), surface)?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        separator_center,
    ))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        DipPoint::new(second.x + 80.0, second.y),
    ))?;
    let committed = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        DipPoint::new(second.x + 80.0, second.y),
    ))?;
    assert_eq!(
        committed,
        vec![QueuedDockAction::Effect(Effect::PersistConfiguration)]
    );
    assert_eq!(
        controller.state().dock_layout().last(),
        Some(&DockLayoutEntry::Separator(separator))
    );

    let separator_center = visual_center(&controller, separator.value(), surface)?;
    assert!(
        controller
            .context_menu_items(separator_center)
            .iter()
            .any(|item| item.command() == Some(ContextMenuCommand::RemoveSeparator))
    );
    let removed =
        controller.handle_context_menu(separator_center, ContextMenuCommand::RemoveSeparator)?;
    assert_eq!(
        removed,
        vec![QueuedDockAction::Effect(Effect::PersistConfiguration)]
    );
    assert!(
        controller
            .state()
            .dock_layout()
            .iter()
            .all(|entry| *entry != DockLayoutEntry::Separator(separator))
    );
    Ok(())
}

#[test]
fn context_menu_and_file_drop_pin_existing_windows_without_private_shell_hooks()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a dock controller with a public context-menu command and file drop.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let first = item_center(&controller, DockItemId::new(1), surface)?;

    // When: unpin is selected and an executable is dropped.
    let unpin = controller.handle_context_menu(first, ContextMenuCommand::Unpin)?;
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
fn context_menu_model_is_specific_to_the_clicked_item_and_open_activates_it()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let first = item_center(&controller, DockItemId::new(1), surface)?;

    let items = controller.context_menu_items(first);
    assert_eq!(
        items,
        vec![
            DockContextMenuItem::action("Open", ContextMenuCommand::Open, true),
            DockContextMenuItem::action("Remove from Dock", ContextMenuCommand::Unpin, true,),
            DockContextMenuItem::separator(),
            DockContextMenuItem::action("Move Left", ContextMenuCommand::MoveLeft, false),
            DockContextMenuItem::action("Move Right", ContextMenuCommand::MoveRight, true),
            DockContextMenuItem::separator(),
            DockContextMenuItem::action("Add Separator", ContextMenuCommand::AddSeparator, true,),
            DockContextMenuItem::separator(),
            DockContextMenuItem::action(
                "Abrir Gerenciador de Tarefas",
                ContextMenuCommand::OpenTaskManager,
                true,
            ),
            DockContextMenuItem::action("Quit Minha UI", ContextMenuCommand::Quit, true,),
        ]
    );

    let actions = controller.handle_context_menu(first, ContextMenuCommand::Open)?;
    assert!(matches!(actions.as_slice(), [QueuedDockAction::Launch(_)]));
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
    controller.snap_animation_to_target();

    // Then: the scene records hover and the hovered bounds are magnified.
    let scene = controller.scene();
    let layout = shell_renderer::layout_dock_scene(&scene, DipRect::new(0.0, 0.0, 320.0, 96.0));
    assert_eq!(scene.hovered_item(), Some(first_item));
    assert_eq!(scene.hover_strength(), 1.0);
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
fn magnification_spring_enters_smoothly_and_converges() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let first = item_center(&controller, DockItemId::new(1), surface)?;

    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Moved, first))?;

    assert!(!controller.animator().is_idle());
    assert!((controller.animator().position_x() - first.x).abs() < 0.01);
    assert_eq!(controller.animator().strength(), 0.0);

    controller.advance_animation(1.0 / 60.0);
    assert!(controller.animator().strength() > 0.0);
    assert!(controller.animator().strength() < 1.0);
    assert!(controller.animator().material_strength() > 0.0);
    assert!(controller.animator().material_strength() < controller.animator().strength());

    for _ in 0..180 {
        controller.advance_animation(1.0 / 120.0);
    }
    assert!(controller.animator().is_idle());
    assert!((controller.animator().strength() - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn magnification_spring_is_refresh_rate_independent() {
    // Given: the same in-flight pointer transition sampled on two monitor refresh rates.
    let mut seed = DockAnimator::new();
    seed.retarget(0.0, 1.0);
    seed.advance(0.05);
    seed.retarget(120.0, 1.0);
    let mut at_60_hz = seed;
    let mut at_144_hz = seed;

    // When: both animations advance through the same quarter second.
    for _ in 0..15 {
        at_60_hz.advance(1.0 / 60.0);
    }
    for _ in 0..36 {
        at_144_hz.advance(1.0 / 144.0);
    }

    // Then: monitor cadence does not alter the visible spring state.
    assert!((at_60_hz.position_x() - at_144_hz.position_x()).abs() < 0.000_1);
    assert!((at_60_hz.strength() - at_144_hz.strength()).abs() < 0.000_1);
    assert!(at_60_hz.position_x() <= 120.0);
    assert!(at_144_hz.position_x() <= 120.0);
}

#[test]
fn magnification_animator_rejects_non_finite_targets() {
    for position in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut animator = DockAnimator::new();
        animator.retarget(position, 1.0);
        assert!(animator.is_idle());
        assert!(animator.position_x().is_finite());
        assert_eq!(animator.strength(), 0.0);
    }

    let mut animator = DockAnimator::new();
    animator.retarget(42.0, f32::NAN);
    assert!(animator.is_idle());
    assert!(animator.position_x().is_finite());
    assert_eq!(animator.strength(), 0.0);
}

#[test]
fn magnification_spring_returns_to_rest_after_pointer_exit()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let first = item_center(&controller, DockItemId::new(1), surface)?;
    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Moved, first))?;
    for _ in 0..30 {
        controller.advance_animation(1.0 / 120.0);
    }

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Exited,
        DipPoint::new(-1.0, -1.0),
    ))?;
    assert!(!controller.animator().is_idle());
    for _ in 0..180 {
        controller.advance_animation(1.0 / 120.0);
    }

    assert!(controller.animator().is_idle());
    assert_eq!(controller.animator().strength(), 0.0);
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

    // Then: the synthetic separator never activates or becomes a persisted drag target.
    assert!(click.is_empty());
    assert!(drag.is_empty());
    assert_ne!(
        controller.state().dock_items()[0].id(),
        DockItemId::new(separator.id())
    );
    Ok(())
}

#[test]
fn dragging_a_running_unpinned_app_into_the_layout_pins_it_on_release()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 420.0, 96.0);
    controller.update_surface(surface);
    controller.sync_running_windows(&[ObservedWindow::new(
        WindowId::new(777),
        app("mspaint.exe")?,
        false,
        false,
    )])?;
    let running = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "mspaint.exe")
        .expect("running app should be discovered")
        .id();
    let running_center = item_center(&controller, running, surface)?;
    let mut before_first = item_center(&controller, DockItemId::new(1), surface)?;
    before_first.x -= 1.0;

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        running_center,
    ))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        before_first,
    ))?;
    assert_eq!(
        controller
            .state()
            .dock_items()
            .iter()
            .find(|item| item.id() == running)
            .expect("running item should remain present")
            .pin(),
        PinState::Unpinned
    );
    assert_eq!(controller.scene().items()[0].id(), running.value());

    let actions = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        before_first,
    ))?;

    assert!(actions.contains(&QueuedDockAction::Effect(Effect::PersistConfiguration)));
    assert_eq!(
        controller.state().dock_layout()[0],
        DockLayoutEntry::App(running)
    );
    assert_eq!(
        controller
            .state()
            .dock_items()
            .iter()
            .find(|item| item.id() == running)
            .expect("running item should remain present")
            .pin(),
        PinState::Pinned
    );
    Ok(())
}

#[test]
fn dragging_between_running_unpinned_apps_reorders_without_pinning()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 520.0, 96.0);
    controller.update_surface(surface);
    controller.sync_running_windows(&[
        ObservedWindow::new(WindowId::new(779), app("mspaint.exe")?, false, false),
        ObservedWindow::new(WindowId::new(780), app("terminal.exe")?, false, false),
    ])?;
    let paint = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "mspaint.exe")
        .expect("paint should be discovered")
        .id();
    let terminal = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "terminal.exe")
        .expect("terminal should be discovered")
        .id();
    let mut before_paint = item_center(&controller, paint, surface)?;
    before_paint.x -= 1.0;
    let terminal_center = item_center(&controller, terminal, surface)?;

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        terminal_center,
    ))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        before_paint,
    ))?;

    let preview = controller.scene();
    let separator_index = preview
        .items()
        .iter()
        .position(|item| item.kind() == DockItemVisualKind::Separator)
        .expect("running apps should remain behind the group separator");
    let terminal_index = preview
        .items()
        .iter()
        .position(|item| item.id() == terminal.value())
        .expect("dragged running app should remain visible");
    let paint_index = preview
        .items()
        .iter()
        .position(|item| item.id() == paint.value())
        .expect("target running app should remain visible");
    assert!(separator_index < terminal_index);
    assert!(terminal_index < paint_index);

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        before_paint,
    ))?;

    let terminal_item = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.id() == terminal)
        .expect("terminal should remain present");
    assert_eq!(terminal_item.pin(), PinState::Unpinned);
    assert!(
        !controller
            .state()
            .dock_layout()
            .contains(&DockLayoutEntry::App(terminal))
    );
    let running_order = controller
        .state()
        .dock_items()
        .iter()
        .filter(|item| item.pin() == PinState::Unpinned)
        .map(|item| item.id())
        .collect::<Vec<_>>();
    assert_eq!(running_order, vec![terminal, paint]);
    Ok(())
}

#[test]
fn dragging_a_running_pinned_app_into_the_running_group_unpins_and_positions_it()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 560.0, 96.0);
    controller.update_surface(surface);
    controller.sync_running_windows(&[
        ObservedWindow::new(WindowId::new(781), app("app.notepad")?, false, false),
        ObservedWindow::new(WindowId::new(782), app("terminal.exe")?, false, false),
        ObservedWindow::new(WindowId::new(783), app("mspaint.exe")?, false, false),
    ])?;
    let terminal = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "terminal.exe")
        .expect("terminal should be discovered")
        .id();
    let paint = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "mspaint.exe")
        .expect("paint should be discovered")
        .id();
    let pinned = DockItemId::new(1);
    let pinned_center = item_center(&controller, pinned, surface)?;
    let mut before_paint = item_center(&controller, paint, surface)?;
    before_paint.x -= 1.0;

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        pinned_center,
    ))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        before_paint,
    ))?;

    let preview = controller.scene();
    let separator_index = preview
        .items()
        .iter()
        .position(|item| item.kind() == DockItemVisualKind::Separator)
        .expect("running group separator should stay visible");
    let terminal_index = preview
        .items()
        .iter()
        .position(|item| item.id() == terminal.value())
        .expect("terminal should stay visible");
    let pinned_index = preview
        .items()
        .iter()
        .position(|item| item.id() == pinned.value())
        .expect("dragged app should stay visible");
    let paint_index = preview
        .items()
        .iter()
        .position(|item| item.id() == paint.value())
        .expect("paint should stay visible");
    assert!(separator_index < terminal_index);
    assert!(terminal_index < pinned_index);
    assert!(pinned_index < paint_index);

    let actions = controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        before_paint,
    ))?;

    assert!(actions.contains(&QueuedDockAction::Effect(Effect::PersistConfiguration)));
    assert!(
        !controller
            .state()
            .dock_layout()
            .contains(&DockLayoutEntry::App(pinned))
    );
    assert_eq!(
        controller
            .state()
            .dock_items()
            .iter()
            .find(|item| item.id() == pinned)
            .expect("dragged app should remain present")
            .pin(),
        PinState::Unpinned
    );
    let running_order = controller
        .state()
        .dock_items()
        .iter()
        .filter(|item| item.pin() == PinState::Unpinned)
        .map(|item| item.id())
        .collect::<Vec<_>>();
    assert_eq!(running_order, vec![terminal, pinned, paint]);
    Ok(())
}

#[test]
fn context_menu_reorders_running_unpinned_apps_without_pinning()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 520.0, 96.0);
    controller.update_surface(surface);
    controller.sync_running_windows(&[
        ObservedWindow::new(WindowId::new(784), app("terminal.exe")?, false, false),
        ObservedWindow::new(WindowId::new(785), app("mspaint.exe")?, false, false),
    ])?;
    let terminal = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "terminal.exe")
        .expect("terminal should be discovered")
        .id();
    let paint = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "mspaint.exe")
        .expect("paint should be discovered")
        .id();
    let paint_center = item_center(&controller, paint, surface)?;

    let items = controller.context_menu_items(paint_center);
    assert!(items.contains(&DockContextMenuItem::action(
        "Move Left",
        ContextMenuCommand::MoveLeft,
        true,
    )));
    let actions = controller.handle_context_menu(paint_center, ContextMenuCommand::MoveLeft)?;

    assert!(actions.contains(&QueuedDockAction::Effect(Effect::PersistConfiguration)));
    assert_eq!(
        controller
            .state()
            .dock_items()
            .iter()
            .filter(|item| item.pin() == PinState::Unpinned)
            .map(|item| item.id())
            .collect::<Vec<_>>(),
        vec![paint, terminal]
    );
    assert!(
        !controller
            .state()
            .dock_layout()
            .contains(&DockLayoutEntry::App(paint))
    );
    Ok(())
}

#[test]
fn release_recomputes_the_final_group_from_the_release_position()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 520.0, 96.0);
    controller.update_surface(surface);
    controller.sync_running_windows(&[ObservedWindow::new(
        WindowId::new(786),
        app("terminal.exe")?,
        false,
        false,
    )])?;
    let running = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "terminal.exe")
        .expect("terminal should be discovered")
        .id();
    let running_center = item_center(&controller, running, surface)?;
    let mut before_first = item_center(&controller, DockItemId::new(1), surface)?;
    before_first.x -= 1.0;
    let release_in_running_group = DipPoint::new(surface.width - 12.0, running_center.y);

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        running_center,
    ))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Dragged,
        before_first,
    ))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Released,
        release_in_running_group,
    ))?;

    assert_eq!(
        controller
            .state()
            .dock_items()
            .iter()
            .find(|item| item.id() == running)
            .expect("running app should remain present")
            .pin(),
        PinState::Unpinned
    );
    assert!(
        !controller
            .state()
            .dock_layout()
            .contains(&DockLayoutEntry::App(running))
    );
    Ok(())
}

#[test]
fn closing_a_running_app_during_drag_cancels_the_stale_session()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 420.0, 96.0);
    controller.update_surface(surface);
    controller.sync_running_windows(&[ObservedWindow::new(
        WindowId::new(787),
        app("terminal.exe")?,
        false,
        false,
    )])?;
    let running = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "terminal.exe")
        .expect("terminal should be discovered")
        .id();
    let running_center = item_center(&controller, running, surface)?;
    let first = item_center(&controller, DockItemId::new(1), surface)?;

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        running_center,
    ))?;
    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Dragged, first))?;
    controller.sync_running_windows(&[])?;
    let actions =
        controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Released, first))?;

    assert!(actions.is_empty());
    assert!(
        controller
            .state()
            .dock_items()
            .iter()
            .all(|item| item.id() != running)
    );
    Ok(())
}

#[test]
fn opening_an_app_during_drag_keeps_one_running_group_separator()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 520.0, 96.0);
    controller.update_surface(surface);
    let terminal_window =
        ObservedWindow::new(WindowId::new(788), app("terminal.exe")?, false, false);
    controller.sync_running_windows(std::slice::from_ref(&terminal_window))?;
    let terminal = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "terminal.exe")
        .expect("terminal should be discovered")
        .id();
    let terminal_center = item_center(&controller, terminal, surface)?;
    let first = item_center(&controller, DockItemId::new(1), surface)?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        terminal_center,
    ))?;
    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Dragged, first))?;

    controller.sync_running_windows(&[
        terminal_window,
        ObservedWindow::new(WindowId::new(789), app("mspaint.exe")?, false, false),
    ])?;

    assert_eq!(
        controller
            .scene()
            .items()
            .iter()
            .filter(|item| item.kind() == DockItemVisualKind::Separator)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn cancelling_an_unpinned_app_drag_keeps_it_in_the_running_group()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let surface = DipRect::new(0.0, 0.0, 420.0, 96.0);
    controller.update_surface(surface);
    controller.sync_running_windows(&[ObservedWindow::new(
        WindowId::new(778),
        app("mspaint.exe")?,
        false,
        false,
    )])?;
    let running = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "mspaint.exe")
        .expect("running app should be discovered")
        .id();
    let running_center = item_center(&controller, running, surface)?;
    let first = item_center(&controller, DockItemId::new(1), surface)?;

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Pressed,
        running_center,
    ))?;
    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Dragged, first))?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Cancelled,
        DipPoint::new(-1.0, -1.0),
    ))?;

    assert!(
        !controller
            .state()
            .dock_layout()
            .contains(&DockLayoutEntry::App(running))
    );
    assert_eq!(
        controller
            .state()
            .dock_items()
            .iter()
            .find(|item| item.id() == running)
            .expect("running item should remain present")
            .pin(),
        PinState::Unpinned
    );
    Ok(())
}

#[test]
fn keyboard_focus_activates_dock_items_without_pointer_input()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a dock with no pointer hover state.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 96.0));

    // When: keyboard navigation focuses the second app and activates it.
    assert!(controller.handle_key(DockKey::Next)?.is_empty());
    assert!(controller.handle_key(DockKey::Next)?.is_empty());
    let actions = controller.handle_key(DockKey::Activate)?;

    // Then: focus is visible in the scene and activation uses the dock reducer path.
    assert_eq!(controller.scene().focused_item(), Some(2));
    assert_eq!(
        actions,
        vec![QueuedDockAction::Launch("app.calculator".to_owned())]
    );

    // When: Escape is pressed.
    assert!(controller.handle_key(DockKey::Escape)?.is_empty());

    // Then: keyboard focus is cleared without changing app state.
    assert_eq!(controller.scene().focused_item(), None);
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

fn visual_center(
    controller: &DockController,
    id: u64,
    surface: DipRect,
) -> Result<DipPoint, Box<dyn std::error::Error>> {
    let layout = shell_renderer::layout_dock_scene(&controller.scene(), surface);
    let item = layout
        .items()
        .iter()
        .find(|item| item.id() == id)
        .ok_or("dock visual was not laid out")?;
    let bounds = item.bounds();
    Ok(DipPoint::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    ))
}
