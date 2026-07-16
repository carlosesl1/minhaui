use crate::{
    ContextMenuCommand, DockContextMenuController, DockContextMenuItem, PopoverKey,
    QueuedContextMenuAction,
};
use shell_renderer::{
    DipPoint, DipRect, context_menu_height_for_entries, context_menu_surface_width,
};

fn controller() -> DockContextMenuController {
    DockContextMenuController::new()
}

#[test]
fn pointer_hover_redraws_without_executing_and_click_executes_the_target_command() {
    let target = DipPoint::new(88.0, 42.0);
    let mut controller = controller();
    controller.open(
        target,
        vec![
            DockContextMenuItem::action("Open", ContextMenuCommand::Open, true),
            DockContextMenuItem::separator(),
            DockContextMenuItem::action("Quit Minha UI", ContextMenuCommand::Quit, true),
        ],
    );
    let scene = controller.scene().expect("context menu should be active");
    let surface = DipRect::new(
        0.0,
        0.0,
        context_menu_surface_width(),
        context_menu_height_for_entries(scene.entries()),
    );

    assert_eq!(
        controller.handle_pointer_move(DipPoint::new(40.0, 25.0), surface),
        vec![QueuedContextMenuAction::Redraw]
    );
    assert_eq!(
        controller.scene().and_then(|scene| scene.focused()),
        Some(0)
    );
    assert_eq!(
        controller.handle_pointer_click(DipPoint::new(40.0, 25.0), surface),
        vec![QueuedContextMenuAction::Execute {
            point: target,
            command: ContextMenuCommand::Open,
        }]
    );
}

#[test]
fn keyboard_navigation_skips_separators_and_disabled_rows() {
    let target = DipPoint::new(40.0, 40.0);
    let mut controller = controller();
    controller.open(
        target,
        vec![
            DockContextMenuItem::action("Open", ContextMenuCommand::Open, false),
            DockContextMenuItem::separator(),
            DockContextMenuItem::action("Keep in Dock", ContextMenuCommand::Pin, true),
            DockContextMenuItem::action("Quit Minha UI", ContextMenuCommand::Quit, true),
        ],
    );

    assert_eq!(
        controller.handle_key(PopoverKey::Next),
        vec![QueuedContextMenuAction::Redraw]
    );
    assert_eq!(
        controller.scene().and_then(|scene| scene.focused()),
        Some(2)
    );
    assert_eq!(
        controller.handle_key(PopoverKey::Activate),
        vec![QueuedContextMenuAction::Execute {
            point: target,
            command: ContextMenuCommand::Pin,
        }]
    );
    assert!(!controller.is_active());
}

#[test]
fn separator_click_and_escape_dismiss_the_context_menu() {
    let mut controller = controller();
    controller.open(
        DipPoint::new(0.0, 0.0),
        vec![
            DockContextMenuItem::action("Open", ContextMenuCommand::Open, true),
            DockContextMenuItem::separator(),
            DockContextMenuItem::action("Quit Minha UI", ContextMenuCommand::Quit, true),
        ],
    );
    let scene = controller.scene().expect("context menu should be active");
    let surface = DipRect::new(
        0.0,
        0.0,
        context_menu_surface_width(),
        context_menu_height_for_entries(scene.entries()),
    );

    assert_eq!(
        controller.handle_pointer_click(DipPoint::new(40.0, 52.0), surface),
        vec![QueuedContextMenuAction::Dismiss]
    );
    assert!(!controller.is_active());

    controller.open(
        DipPoint::new(0.0, 0.0),
        vec![DockContextMenuItem::action(
            "Quit Minha UI",
            ContextMenuCommand::Quit,
            true,
        )],
    );
    assert_eq!(
        controller.handle_key(PopoverKey::Escape),
        vec![QueuedContextMenuAction::Dismiss]
    );
    assert!(!controller.is_active());
}
