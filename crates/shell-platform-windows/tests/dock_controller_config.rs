use shell_core::{AppId, DockItem, DockItemId, ShellState};
use shell_platform_windows::{
    ContextMenuCommand, DockController, DockPhysicalPlacement, DockPointerPhase, DockPointerSample,
    DockRuntimeConfig,
};
use shell_renderer::{DipPoint, DipRect, DockAlignment, PhysicalRect};

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

#[test]
fn dock_physical_placement_collapses_hidden_autohide_to_reveal_strip() {
    // Given: a normal dock rectangle and autohide configured with a reveal strip.
    let normal = PhysicalRect::new(100, 800, 720, 96);
    let config = DockRuntimeConfig::default()
        .with_autohide(true)
        .with_reveal_zone_height(6.0);

    // When: hidden and revealed placements are computed.
    let hidden = DockPhysicalPlacement::from_visibility(normal, config, true, 1.5);
    let revealed = DockPhysicalPlacement::from_visibility(normal, config, false, 1.5);

    // Then: hidden keeps only the physical reveal zone on screen.
    assert_eq!(hidden.rect(), PhysicalRect::new(100, 887, 720, 9));
    assert_eq!(revealed.rect(), normal);
    assert!(hidden.is_hidden_strip());
    assert!(!revealed.is_hidden_strip());
}

#[test]
fn native_context_menu_ids_map_to_controller_commands() {
    // Given: documented native menu command ids.
    // When/Then: each selectable command maps to the pure controller command.
    assert_eq!(
        ContextMenuCommand::from_native_id(1),
        Some(ContextMenuCommand::Open)
    );
    assert_eq!(
        ContextMenuCommand::from_native_id(2),
        Some(ContextMenuCommand::Pin)
    );
    assert_eq!(
        ContextMenuCommand::from_native_id(3),
        Some(ContextMenuCommand::Unpin)
    );
    assert_eq!(
        ContextMenuCommand::from_native_id(4),
        Some(ContextMenuCommand::Quit)
    );
    assert_eq!(ContextMenuCommand::from_native_id(404), None);
}
