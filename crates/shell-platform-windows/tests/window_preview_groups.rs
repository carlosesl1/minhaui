use crate::{
    DockController, DockRuntimeConfig, ObservedWindow, PreviewAction, PreviewCapture,
    PreviewQueuedAction, QueuedDockAction,
};
use shell_core::{AppId, DockItem, DockItemId, ShellState, WindowId};

fn app(value: &str) -> Result<AppId, Box<dyn std::error::Error>> {
    Ok(AppId::parse(value)?)
}

fn state() -> Result<ShellState, Box<dyn std::error::Error>> {
    Ok(ShellState::default().with_dock_items(vec![DockItem::pinned(
        DockItemId::new(1),
        app("notepad.exe")?,
    )]))
}

fn observed_windows() -> Result<Vec<ObservedWindow>, Box<dyn std::error::Error>> {
    Ok(vec![
        ObservedWindow::new(WindowId::new(701), app("notepad.exe")?, false, false)
            .with_title("Alpha")
            .with_preview(PreviewCapture::dwm_thumbnail()),
        ObservedWindow::new(WindowId::new(702), app("notepad.exe")?, false, true)
            .with_title("Beta")
            .with_preview(PreviewCapture::dwm_thumbnail()),
        ObservedWindow::new(WindowId::new(703), app("notepad.exe")?, true, false)
            .with_title("Gamma")
            .with_preview(PreviewCapture::dwm_thumbnail()),
    ])
}

#[test]
fn preview_group_preserves_every_window_in_stable_order() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: one dock application owns three independently addressable windows.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.sync_running_windows_with_previews(&observed_windows()?)?;
    assert!(controller.has_preview_windows_for_item(DockItemId::new(1)));
    assert!(!controller.has_preview_windows_for_item(DockItemId::new(999)));

    // When: the application's preview group is requested.
    let group = controller.preview_windows_for_item(DockItemId::new(1));

    // Then: every window remains available with the focused window first.
    assert_eq!(
        group
            .iter()
            .map(|window| (window.window(), window.title()))
            .collect::<Vec<_>>(),
        vec![
            (WindowId::new(703), "Gamma"),
            (WindowId::new(701), "Alpha"),
            (WindowId::new(702), "Beta"),
        ]
    );
    Ok(())
}

#[test]
fn preview_action_targets_a_secondary_window() -> Result<(), Box<dyn std::error::Error>> {
    // Given: the synchronized preview group contains a secondary window.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.sync_running_windows_with_previews(&observed_windows()?)?;

    // When: close is requested for that secondary window.
    let close = controller.handle_preview_action(WindowId::new(702), PreviewAction::Close)?;

    // Then: the action retains the secondary window's application identity.
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
fn next_discovery_snapshot_removes_stale_preview_windows() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a synchronized preview group contains three windows.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.sync_running_windows_with_previews(&observed_windows()?)?;

    // When: discovery reports only one surviving window.
    controller.sync_running_windows_with_previews(&[ObservedWindow::new(
        WindowId::new(701),
        app("notepad.exe")?,
        true,
        false,
    )
    .with_title("Alpha")
    .with_preview(PreviewCapture::dwm_thumbnail())])?;

    // Then: stale windows disappear from both the group and action routing.
    assert_eq!(
        controller
            .preview_windows_for_item(DockItemId::new(1))
            .iter()
            .map(|window| window.window())
            .collect::<Vec<_>>(),
        vec![WindowId::new(701)]
    );
    assert!(
        controller
            .handle_preview_action(WindowId::new(702), PreviewAction::Close)?
            .is_empty()
    );
    Ok(())
}
