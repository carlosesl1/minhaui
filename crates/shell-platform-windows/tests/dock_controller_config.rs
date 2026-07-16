use crate::{
    ContextMenuCommand, DockController, DockPhysicalPlacement, DockPointerPhase, DockPointerSample,
    DockRuntimeConfig, resolve_dock_visibility,
};
use shell_core::{AppId, DockItem, DockItemId, ShellState};
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
        DipPoint::new(300.0, 4.0),
    ))?;

    // Then: reveal is handled as state, not by blocking animation.
    assert!(controller.state().dock().is_revealed());
    assert!(reveal.is_empty());
    assert_eq!(controller.config().alignment(), DockAlignment::Right);
    Ok(())
}

#[test]
fn dock_physical_placement_moves_full_surface_below_the_reveal_strip() {
    // Given: a normal dock rectangle and autohide configured with a reveal strip.
    let normal = PhysicalRect::new(100, 800, 720, 96);
    let config = DockRuntimeConfig::default()
        .with_autohide(true)
        .with_reveal_zone_height(6.0);

    // When: hidden and revealed placements are computed.
    let hidden = DockPhysicalPlacement::from_visibility(normal, config, true, 1.5);
    let revealed = DockPhysicalPlacement::from_visibility(normal, config, false, 1.5);

    // Then: only the reveal zone remains inside the monitor, but the HWND keeps
    // its complete surface size so showing it never resizes the compositor.
    assert_eq!(hidden.rect(), PhysicalRect::new(100, 887, 720, 96));
    assert_eq!(revealed.rect(), normal);
    assert!(hidden.is_hidden_strip());
    assert!(!revealed.is_hidden_strip());
}

#[test]
fn fullscreen_autohide_can_reveal_even_when_user_autohide_is_disabled() {
    let normal = PhysicalRect::new(100, 800, 720, 96);
    let config = DockRuntimeConfig::default().with_autohide(false);

    let (effective_config, hidden) = resolve_dock_visibility(config, false, true);
    let placement = DockPhysicalPlacement::from_visibility(normal, effective_config, hidden, 1.0);
    let (_, revealed) = resolve_dock_visibility(config, true, true);

    assert!(effective_config.autohide());
    assert!(hidden);
    assert!(!revealed);
    assert!(placement.is_hidden_strip());
    assert_eq!(placement.rect(), PhysicalRect::new(100, 888, 720, 96));
}

#[test]
fn fullscreen_autohide_uses_the_bottom_hot_edge_when_preference_is_disabled()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller =
        DockController::new(state()?, DockRuntimeConfig::default().with_autohide(false))?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 55.0));

    controller.set_fullscreen_autohide(true)?;
    assert!(!controller.state().dock().is_revealed());

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Moved,
        DipPoint::new(160.0, 4.0),
    ))?;
    assert!(controller.state().dock().is_revealed());

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Exited,
        DipPoint::new(-1.0, -1.0),
    ))?;
    assert!(!controller.state().dock().is_revealed());

    controller.set_fullscreen_autohide(false)?;
    assert!(controller.state().dock().is_revealed());
    Ok(())
}

#[test]
fn context_overlay_hold_keeps_autohide_dock_revealed_until_release()
-> Result<(), Box<dyn std::error::Error>> {
    let config = DockRuntimeConfig::default().with_autohide(true);
    let mut controller = DockController::new(state()?, config)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 96.0));

    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Exited,
        DipPoint::new(-1.0, -1.0),
    ))?;
    assert!(!controller.state().dock().is_revealed());

    controller.hold_revealed_for_overlay()?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Exited,
        DipPoint::new(-1.0, -1.0),
    ))?;
    assert!(controller.state().dock().is_revealed());

    controller.release_overlay_hold()?;
    assert!(!controller.state().dock().is_revealed());
    Ok(())
}

#[test]
fn releasing_overlay_hold_while_pointer_is_back_on_dock_keeps_it_revealed()
-> Result<(), Box<dyn std::error::Error>> {
    let config = DockRuntimeConfig::default().with_autohide(true);
    let mut controller = DockController::new(state()?, config)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 320.0, 96.0));
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Exited,
        DipPoint::new(-1.0, -1.0),
    ))?;

    controller.hold_revealed_for_overlay()?;
    controller.handle_pointer(DockPointerSample::new(
        DockPointerPhase::Moved,
        DipPoint::new(160.0, 94.0),
    ))?;
    controller.release_overlay_hold()?;

    assert!(controller.state().dock().is_revealed());
    Ok(())
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
