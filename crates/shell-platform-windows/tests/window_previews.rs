use shell_core::{AppId, DockItem, DockItemId, RunningState, ShellState, WindowId};
use shell_platform_windows::{
    ContextMenuCommand, DockController, DockPointerPhase, DockPointerSample, DockRuntimeConfig,
    ObservedWindow, PreviewAction, PreviewCapture, PreviewQueuedAction, PreviewUnavailableReason,
    QueuedDockAction,
};
use shell_renderer::{DipPoint, DipRect, WindowPreviewVisual};

fn app(value: &str) -> Result<AppId, Box<dyn std::error::Error>> {
    Ok(AppId::parse(value)?)
}

fn state() -> Result<ShellState, Box<dyn std::error::Error>> {
    Ok(ShellState::default().with_dock_items(vec![DockItem::pinned(
        DockItemId::new(1),
        app("notepad.exe")?,
    )]))
}

#[test]
fn hover_exposes_window_preview_without_changing_running_state()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a running pinned app with an available DWM thumbnail.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 260.0, 96.0));
    controller.sync_running_windows_with_previews(&[ObservedWindow::new(
        WindowId::new(700),
        app("notepad.exe")?,
        true,
        false,
    )
    .with_preview(PreviewCapture::dwm_thumbnail())])?;

    // When: the pointer hovers the running item.
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Moved,
        DipPoint::new(130.0, 48.0),
    ))?;

    // Then: the scene exposes one preview and keeps the dock item focused.
    assert_eq!(
        controller.scene().window_previews(),
        &[WindowPreviewVisual::available(
            WindowId::new(700),
            DockItemId::new(1)
        )]
    );
    assert_eq!(
        controller.state().dock_items()[0].running(),
        &RunningState::Running {
            window: WindowId::new(700),
            focused: true,
            minimized: false
        }
    );
    Ok(())
}

#[test]
fn preview_degrades_explicitly_when_capture_is_restricted() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a running app whose window cannot be captured by DWM.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 260.0, 96.0));
    controller.sync_running_windows_with_previews(&[ObservedWindow::new(
        WindowId::new(701),
        app("notepad.exe")?,
        false,
        false,
    )
    .with_preview(PreviewCapture::restricted(
        PreviewUnavailableReason::CaptureRestricted,
    ))])?;

    // When: the pointer hovers the running item.
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Moved,
        DipPoint::new(130.0, 48.0),
    ))?;

    // Then: the scene carries the explicit restriction instead of an invented bitmap.
    assert_eq!(
        controller.scene().window_previews(),
        &[WindowPreviewVisual::restricted(
            WindowId::new(701),
            DockItemId::new(1),
            PreviewUnavailableReason::CaptureRestricted
        )]
    );
    Ok(())
}

#[test]
fn preview_actions_queue_focus_and_close_effects_without_blocking()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a running app with a preview target.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.sync_running_windows_with_previews(&[ObservedWindow::new(
        WindowId::new(702),
        app("notepad.exe")?,
        false,
        false,
    )
    .with_preview(PreviewCapture::dwm_thumbnail())])?;

    // When: preview focus and close commands are selected.
    let focus = controller.handle_preview_action(WindowId::new(702), PreviewAction::Focus)?;
    let close = controller.handle_preview_action(WindowId::new(702), PreviewAction::Close)?;

    // Then: both commands become queued platform actions without synchronous waits.
    assert_eq!(
        focus,
        vec![QueuedDockAction::Preview(PreviewQueuedAction::Focus {
            window: WindowId::new(702),
            app: app("notepad.exe")?
        })]
    );
    assert_eq!(
        close,
        vec![QueuedDockAction::Preview(PreviewQueuedAction::Close {
            window: WindowId::new(702),
            app: app("notepad.exe")?
        })]
    );
    Ok(())
}

#[test]
fn preview_context_commands_route_to_hovered_window_actions()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a hovered running app whose preview has a native context command.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 260.0, 96.0));
    controller.sync_running_windows_with_previews(&[ObservedWindow::new(
        WindowId::new(703),
        app("notepad.exe")?,
        false,
        false,
    )
    .with_preview(PreviewCapture::dwm_thumbnail())])?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Moved,
        DipPoint::new(130.0, 48.0),
    ))?;

    // When: native menu command IDs select preview focus and close.
    let focus = controller
        .handle_context_menu(DipPoint::new(130.0, 48.0), ContextMenuCommand::PreviewFocus)?;
    let close = controller
        .handle_context_menu(DipPoint::new(130.0, 48.0), ContextMenuCommand::PreviewClose)?;

    // Then: the same safe preview platform actions are emitted through input routing.
    assert_eq!(
        focus,
        vec![QueuedDockAction::Preview(PreviewQueuedAction::Focus {
            window: WindowId::new(703),
            app: app("notepad.exe")?
        })]
    );
    assert_eq!(
        close,
        vec![QueuedDockAction::Preview(PreviewQueuedAction::Close {
            window: WindowId::new(703),
            app: app("notepad.exe")?
        })]
    );
    Ok(())
}
