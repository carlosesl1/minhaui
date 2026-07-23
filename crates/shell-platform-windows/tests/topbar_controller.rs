use crate::{
    ObservedWindow, QueuedTopbarAction, TopbarController, TopbarKey, TopbarPointerPhase,
    TopbarPointerSample, TopbarSnapshot, foreground_app_label,
};
use shell_core::{AppId, Popover, ShellState, TopbarModuleKind, WindowId};
use shell_renderer::{DipPoint, DipRect, TopbarDensity};

#[test]
fn topbar_click_opens_typed_module_intent_without_rebuilding_resources()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a topbar controller with visible base modules.
    let mut controller = TopbarController::new(ShellState::default(), TopbarDensity::Comfortable)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 900.0, 40.0));
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
        vec![QueuedTopbarAction::OpenPopover {
            popover: Popover::Network,
            anchor: crate::TopbarOverlayAnchor::Module(TopbarModuleKind::Network),
        }]
    );
    assert_eq!(controller.resource_generation(), 0);
    Ok(())
}

#[test]
fn background_apps_module_opens_its_typed_popover() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = TopbarController::new(ShellState::default(), TopbarDensity::Compact)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 900.0, 40.0));
    let point = module_center(&controller, TopbarModuleKind::BackgroundApps)?;

    controller.handle_pointer(TopbarPointerSample::new(TopbarPointerPhase::Pressed, point))?;

    assert_eq!(
        controller.handle_pointer(TopbarPointerSample::new(
            TopbarPointerPhase::Released,
            point,
        ))?,
        vec![QueuedTopbarAction::OpenPopover {
            popover: Popover::BackgroundApps,
            anchor: crate::TopbarOverlayAnchor::Module(TopbarModuleKind::BackgroundApps),
        }]
    );
    Ok(())
}

#[test]
fn topbar_keyboard_focus_opens_visible_modules_without_pointer_input()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a topbar controller with keyboard focus on no module yet.
    let mut controller = TopbarController::new(ShellState::default(), TopbarDensity::Comfortable)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 900.0, 40.0));
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
        vec![QueuedTopbarAction::OpenPopover {
            popover: Popover::Network,
            anchor: crate::TopbarOverlayAnchor::Module(TopbarModuleKind::Network),
        }]
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
fn search_dispatches_directly_and_active_app_is_informational()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = TopbarController::new(ShellState::default(), TopbarDensity::Comfortable)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 900.0, 40.0));
    controller
        .update_snapshot(TopbarSnapshot::privacy_safe_fixture().with_app_label("File Explorer"));

    let scene = controller.scene();
    let app = scene
        .modules()
        .iter()
        .find(|module| module.kind() == TopbarModuleKind::AppIdentity)
        .expect("active app module");
    assert_eq!(app.text(), "File Explorer");
    assert!(app.intent().is_none());

    let search = module_center(&controller, TopbarModuleKind::Search)?;
    controller.handle_pointer(TopbarPointerSample::new(
        TopbarPointerPhase::Pressed,
        search,
    ))?;
    assert_eq!(
        controller.handle_pointer(TopbarPointerSample::new(
            TopbarPointerPhase::Released,
            search,
        ))?,
        vec![QueuedTopbarAction::OpenSearch]
    );
    Ok(())
}

#[test]
fn foreground_identity_uses_cached_observation_without_window_title_data()
-> Result<(), Box<dyn std::error::Error>> {
    let background = ObservedWindow::new(
        WindowId::new(1),
        AppId::parse("private-document-editor.exe")?,
        false,
        false,
    )
    .with_title("Secret plan.txt");
    let foreground =
        ObservedWindow::new(WindowId::new(2), AppId::parse("explorer.exe")?, true, false)
            .with_title("C:\\Users\\Carlos\\Private");

    assert_eq!(foreground_app_label(&[background, foreground]), "explorer");
    assert_eq!(foreground_app_label(&[]), "Desktop");
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
        DipRect::new(0.0, 0.0, 900.0, 40.0),
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
