use crate::{
    ContextMenuCommand, DockController, DockPointerPhase, DockPointerSample, DockRuntimeConfig,
    ObservedWindow, PreviewCapture, QueuedDockAction,
};
use shell_core::{
    AppId, DockItem, DockItemId, Effect, PinState, RunningState, ShellState, WindowId,
};
use shell_renderer::{DipPoint, DipRect};

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
    let surface = DipRect::new(0.0, 0.0, 320.0, 96.0);
    controller.update_surface(surface);
    let point = item_center(&controller, DockItemId::new(1), surface)?;
    let generation = controller.visual_generation();

    // When: the user presses and releases the first item.
    controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Pressed, point))?;
    assert_eq!(controller.scene().pressed_item(), Some(1));
    assert!(controller.visual_generation() > generation);
    let actions =
        controller.handle_pointer(DockPointerSample::new(DockPointerPhase::Released, point))?;

    // Then: launch is queued immediately and animation time remains external.
    assert_eq!(
        actions,
        vec![QueuedDockAction::Launch("app.notepad".into())]
    );
    assert_eq!(controller.scene().pressed_item(), None);
    assert!(controller.animator().is_idle());
    Ok(())
}

#[test]
fn context_menu_pin_persists_discovered_unpinned_app() -> Result<(), Box<dyn std::error::Error>> {
    // Given: window sync has added an unpinned running app.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    controller.update_surface(DipRect::new(0.0, 0.0, 360.0, 96.0));
    controller.sync_running_windows(&[ObservedWindow::new(
        WindowId::new(777),
        app("mspaint.exe")?,
        false,
        false,
    )])?;
    let synced_id = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "mspaint.exe")
        .expect("synced app should be visible")
        .id();
    let point = item_center(&controller, synced_id, DipRect::new(0.0, 0.0, 360.0, 96.0))?;

    // When: the native context-menu Pin command is selected on that item.
    let actions = controller.handle_context_menu(point, ContextMenuCommand::Pin)?;

    // Then: the item is retained as pinned and configuration persistence is queued.
    assert_eq!(
        actions,
        vec![QueuedDockAction::Effect(Effect::PersistConfiguration)]
    );
    let pinned = controller
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "mspaint.exe")
        .expect("synced app should remain present");
    assert_eq!(pinned.pin(), PinState::Pinned);
    Ok(())
}

#[test]
fn sync_running_windows_tracks_pinned_focus_minimize_and_close()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a stopped pinned app and an observed platform window.
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let observed = ObservedWindow::new(WindowId::new(500), app("app.notepad")?, true, false);

    // When: discovery sync runs, then the observed window disappears.
    let opened = controller.sync_running_windows(&[observed])?;
    let closed = controller.sync_running_windows(&[])?;

    // Then: state mirrors the Win32 lifecycle without launch/focus clicks.
    assert!(opened.is_empty());
    assert!(closed.is_empty());
    assert_eq!(
        controller.state().dock_items()[0].running(),
        &RunningState::Stopped
    );
    Ok(())
}

#[test]
fn calculator_alias_reuses_the_pinned_item_and_its_preview_group()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DockController::new(state()?, DockRuntimeConfig::default())?;
    let observed = ObservedWindow::new(WindowId::new(501), app("calc.exe")?, true, false)
        .with_preview(PreviewCapture::dwm_thumbnail());

    controller.sync_running_windows(&[observed])?;

    assert_eq!(controller.state().dock_items().len(), 2);
    assert!(matches!(
        controller.state().dock_items()[1].running(),
        RunningState::Running { window, .. } if *window == WindowId::new(501)
    ));
    assert_eq!(
        controller
            .preview_windows_for_item(DockItemId::new(2))
            .len(),
        1
    );
    Ok(())
}

#[test]
fn explicit_aumid_reuses_the_existing_executable_alias() -> Result<(), Box<dyn std::error::Error>> {
    let vivaldi = DockItem::pinned(DockItemId::new(41), app("vivaldi.exe")?);
    let mut controller = DockController::new(
        ShellState::default().with_dock_items(vec![vivaldi]),
        DockRuntimeConfig::default(),
    )?;
    let observed = ObservedWindow::new(
        WindowId::new(502),
        app("Vivaldi.WMOW6CEHCLPDQ4UVKQKEJN7FEI")?,
        true,
        false,
    )
    .with_aliases(vec![app("vivaldi.exe")?])
    .with_preview(PreviewCapture::dwm_thumbnail());

    controller.sync_running_windows(&[observed])?;

    assert_eq!(controller.state().dock_items().len(), 1);
    assert!(matches!(
        controller.state().dock_items()[0].running(),
        RunningState::Running { window, .. } if *window == WindowId::new(502)
    ));
    assert_eq!(
        controller
            .preview_windows_for_item(DockItemId::new(41))
            .len(),
        1
    );
    Ok(())
}

#[test]
fn sync_removes_a_legacy_duplicate_created_from_aumid_and_executable_aliases()
-> Result<(), Box<dyn std::error::Error>> {
    let executable = DockItem::pinned(DockItemId::new(41), app("vivaldi.exe")?);
    let aumid = DockItem::pinned(
        DockItemId::new(42),
        app("Vivaldi.WMOW6CEHCLPDQ4UVKQKEJN7FEI")?,
    );
    let mut controller = DockController::new(
        ShellState::default().with_dock_items(vec![executable, aumid]),
        DockRuntimeConfig::default(),
    )?;
    let observed = ObservedWindow::new(
        WindowId::new(503),
        app("Vivaldi.WMOW6CEHCLPDQ4UVKQKEJN7FEI")?,
        true,
        false,
    )
    .with_aliases(vec![app("vivaldi.exe")?]);

    let actions = controller.sync_running_windows(&[observed])?;

    assert_eq!(controller.state().dock_items().len(), 1);
    assert_eq!(
        controller.state().dock_items()[0].app().as_str(),
        "vivaldi.exe"
    );
    assert!(actions.contains(&QueuedDockAction::Effect(Effect::PersistConfiguration)));
    Ok(())
}

#[test]
fn sync_preserves_pinned_alias_when_unpinned_primary_appears_first()
-> Result<(), Box<dyn std::error::Error>> {
    let aumid = app("Vivaldi.WMOW6CEHCLPDQ4UVKQKEJN7FEI")?;
    let unpinned = DockItem::running_unpinned(
        DockItemId::new(42),
        aumid.clone(),
        WindowId::new(503),
        false,
        false,
    );
    let pinned = DockItem::pinned(DockItemId::new(41), app("vivaldi.exe")?);
    let mut controller = DockController::new(
        ShellState::default().with_dock_items(vec![unpinned, pinned]),
        DockRuntimeConfig::default(),
    )?;
    let observed = ObservedWindow::new(WindowId::new(503), aumid, true, false)
        .with_aliases(vec![app("vivaldi.exe")?]);

    controller.sync_running_windows(&[observed])?;

    assert_eq!(controller.state().dock_items().len(), 1);
    let survivor = &controller.state().dock_items()[0];
    assert_eq!(survivor.id(), DockItemId::new(41));
    assert_eq!(survivor.app().as_str(), "vivaldi.exe");
    assert_eq!(survivor.pin(), PinState::Pinned);
    assert!(matches!(
        survivor.running(),
        RunningState::Running { window, .. } if *window == WindowId::new(503)
    ));
    Ok(())
}

#[test]
fn sync_running_windows_adds_unpinned_apps_with_stable_identity()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: an external running app that is not already pinned.
    let mut first = DockController::new(state()?, DockRuntimeConfig::default())?;
    let mut second = DockController::new(state()?, DockRuntimeConfig::default())?;
    let observed = ObservedWindow::new(WindowId::new(777), app("mspaint.exe")?, false, false);

    // When: two controller instances sync the same app.
    first.sync_running_windows(std::slice::from_ref(&observed))?;
    second.sync_running_windows(&[observed])?;

    // Then: the unpinned item identity is stable across instances.
    let first_id = first
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "mspaint.exe")
        .map(|item| item.id());
    let second_id = second
        .state()
        .dock_items()
        .iter()
        .find(|item| item.app().as_str() == "mspaint.exe")
        .map(|item| item.id());
    assert_eq!(first_id, second_id);
    assert!(first_id.is_some());
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
