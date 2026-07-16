use crate::{
    ContextMenuCommand, DockContextMenuItem, DockController, DockRuntimeConfig, QueuedDockAction,
};
use shell_core::{AppId, DockItem, DockItemId, ShellState};
use shell_renderer::{DipPoint, DipRect};

fn controller() -> Result<DockController, Box<dyn std::error::Error>> {
    let state = ShellState::default().with_dock_items(vec![DockItem::pinned(
        DockItemId::new(1),
        AppId::parse("app.notepad")?,
    )]);
    let mut controller = DockController::new(state, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 96.0));
    Ok(controller)
}

#[test]
fn dock_context_menu_exposes_task_manager_action() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a dock controller and a point outside its application items.
    let controller = controller()?;

    // When: the global dock context menu is composed.
    let items = controller.context_menu_items(DipPoint::new(0.0, 0.0));

    // Then: Task Manager is available before the quit action.
    let task_manager = DockContextMenuItem::action(
        "Abrir Gerenciador de Tarefas",
        ContextMenuCommand::OpenTaskManager,
        true,
    );
    let task_manager_index = items
        .iter()
        .position(|item| item == &task_manager)
        .ok_or("Task Manager action is missing")?;
    let quit_index = items
        .iter()
        .position(|item| item.command() == Some(ContextMenuCommand::Quit))
        .ok_or("quit action is missing")?;
    assert!(task_manager_index < quit_index);
    Ok(())
}

#[test]
fn task_manager_context_command_queues_windows_task_manager()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a dock controller and its global Task Manager command.
    let mut controller = controller()?;

    // When: the command is executed away from any application item.
    let actions = controller
        .handle_context_menu(DipPoint::new(0.0, 0.0), ContextMenuCommand::OpenTaskManager)?;

    // Then: the existing launch adapter receives the Windows executable.
    assert_eq!(
        actions,
        vec![QueuedDockAction::Launch("taskmgr.exe".to_owned())]
    );
    Ok(())
}
