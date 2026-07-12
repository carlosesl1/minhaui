use shell_core::{
    AppId, DockItem, DockItemId, Effect, Monitor, MonitorId, NoOpReason, PinState, Popover,
    RunningState, ShellEvent, ShellState, TaskbarPolicy, TopbarModuleKind, TransitionError,
    TransitionOutcome, WindowId, reduce,
};

fn app(value: &str) -> Result<AppId, Box<dyn std::error::Error>> {
    Ok(AppId::parse(value)?)
}

fn item(id: u64, app_id: &str) -> Result<DockItem, Box<dyn std::error::Error>> {
    Ok(DockItem::pinned(DockItemId::new(id), app(app_id)?))
}

#[test]
fn defaults_are_safe_when_state_is_created() {
    // Given: no persisted state.
    // When: safe defaults are created.
    let state = ShellState::default();

    // Then: the shell is visible, non-invasive, and has a primary monitor.
    assert_eq!(state.taskbar_policy(), TaskbarPolicy::Off);
    assert!(state.dock().is_revealed());
    assert_eq!(
        state
            .monitors()
            .iter()
            .filter(|monitor| monitor.is_primary())
            .count(),
        1
    );
    assert!(state.validate().is_ok());
}

#[test]
fn launch_focus_minimize_sequence_emits_typed_intents() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a pinned application that is not running.
    let mut state = ShellState::default().with_dock_items(vec![item(7, "app.calculator")?]);

    // When: the user activates it, Windows reports a window, then it is activated twice.
    let launched = reduce(&state, ShellEvent::ActivateDockItem(DockItemId::new(7)))?;
    state = launched.state;
    let opened = reduce(
        &state,
        ShellEvent::WindowOpened {
            item: DockItemId::new(7),
            window: WindowId::new(90),
        },
    )?;
    state = opened.state;
    let focused = reduce(&state, ShellEvent::ActivateDockItem(DockItemId::new(7)))?;
    let minimized = reduce(
        &focused.state,
        ShellEvent::ActivateDockItem(DockItemId::new(7)),
    )?;

    // Then: each action is an intent and the reducer state tracks the window.
    assert_eq!(
        launched.effects,
        vec![Effect::Launch(app("app.calculator")?)]
    );
    assert_eq!(
        focused.effects,
        vec![Effect::FocusWindow(WindowId::new(90))]
    );
    assert_eq!(
        minimized.effects,
        vec![Effect::MinimizeWindow(WindowId::new(90))]
    );
    assert_eq!(
        minimized.state.dock_items()[0].running(),
        &RunningState::Running {
            window: WindowId::new(90),
            focused: false,
            minimized: true,
        }
    );
    Ok(())
}

#[test]
fn observed_window_sync_tracks_focus_and_minimized_state() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a pinned application that Windows has already surfaced.
    let state = ShellState::default().with_dock_items(vec![item(7, "app.calculator")?]);

    // When: platform discovery reports foreground and minimized states.
    let focused = reduce(
        &state,
        ShellEvent::WindowChanged {
            item: DockItemId::new(7),
            window: WindowId::new(90),
            focused: true,
            minimized: false,
        },
    )?;
    let minimized = reduce(
        &focused.state,
        ShellEvent::WindowChanged {
            item: DockItemId::new(7),
            window: WindowId::new(90),
            focused: false,
            minimized: true,
        },
    )?;

    // Then: the dock mirrors the external lifecycle without a launch click.
    assert_eq!(
        minimized.state.dock_items()[0].running(),
        &RunningState::Running {
            window: WindowId::new(90),
            focused: false,
            minimized: true,
        }
    );
    Ok(())
}

#[test]
fn running_unpinned_entries_can_be_pinned_without_losing_window_state()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: discovery created an unpinned running app.
    let state = ShellState::default();
    let running = DockItem::running_unpinned(
        DockItemId::new(44),
        app("app.external")?,
        WindowId::new(440),
        true,
        false,
    );

    // When: the user pins that running app.
    let pinned = reduce(&state, ShellEvent::Pin(running))?;

    // Then: pinning preserves the observed window association.
    let item = &pinned.state.dock_items()[0];
    assert_eq!(item.pin(), PinState::Pinned);
    assert_eq!(
        item.running(),
        &RunningState::Running {
            window: WindowId::new(440),
            focused: true,
            minimized: false,
        }
    );
    Ok(())
}

#[test]
fn pin_existing_unpinned_entry_updates_pin_without_duplicate()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a running unpinned entry is already present in state.
    let running = DockItem::running_unpinned(
        DockItemId::new(44),
        app("app.external")?,
        WindowId::new(440),
        false,
        true,
    );
    let state = ShellState::default().with_dock_items(vec![running.clone()]);

    // When: the user pins that exact discovered entry.
    let pinned = reduce(&state, ShellEvent::Pin(running))?;

    // Then: the item becomes pinned in place and keeps its window state.
    assert_eq!(pinned.state.dock_items().len(), 1);
    assert_eq!(pinned.state.dock_items()[0].id(), DockItemId::new(44));
    assert_eq!(pinned.state.dock_items()[0].pin(), PinState::Pinned);
    assert_eq!(
        pinned.state.dock_items()[0].running(),
        &RunningState::Running {
            window: WindowId::new(440),
            focused: false,
            minimized: true,
        }
    );
    assert_eq!(pinned.effects, vec![Effect::PersistConfiguration]);
    Ok(())
}

#[test]
fn pin_reorder_unpin_sequence_preserves_unique_order() -> Result<(), Box<dyn std::error::Error>> {
    // Given: two pinned applications.
    let state =
        ShellState::default().with_dock_items(vec![item(1, "app.one")?, item(2, "app.two")?]);

    // When: the second is moved before the first and the first is unpinned.
    let reordered = reduce(
        &state,
        ShellEvent::ReorderDockItem {
            item: DockItemId::new(2),
            before: Some(DockItemId::new(1)),
        },
    )?;
    let unpinned = reduce(&reordered.state, ShellEvent::Unpin(DockItemId::new(1)))?;

    // Then: order is deterministic and stopped unpinned items leave the dock.
    assert_eq!(reordered.state.dock_items()[0].id(), DockItemId::new(2));
    assert_eq!(unpinned.state.dock_items().len(), 1);
    assert!(unpinned.state.validate().is_ok());
    assert_eq!(unpinned.effects, vec![Effect::PersistConfiguration]);
    Ok(())
}

#[test]
fn opening_a_popover_replaces_the_previous_popover() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a calendar popover is open.
    let state = reduce(
        &ShellState::default(),
        ShellEvent::OpenPopover(Popover::Calendar),
    )?
    .state;

    // When: volume is opened.
    let transition = reduce(&state, ShellEvent::OpenPopover(Popover::Volume))?;

    // Then: exactly the volume popover remains active.
    assert_eq!(transition.state.active_popover(), Some(Popover::Volume));
    Ok(())
}

#[test]
fn topbar_system_menu_and_visibility_emit_typed_intents() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: the default ordered top bar.
    let state = ShellState::default();

    // When: the system menu entry point is opened and the network module is hidden.
    let opened = reduce(&state, ShellEvent::OpenPopover(Popover::SystemMenu))?;
    let hidden = reduce(
        &opened.state,
        ShellEvent::SetTopbarVisibility {
            module: TopbarModuleKind::Network,
            visible: false,
        },
    )?;

    // Then: the reducer tracks the typed system menu intent and persists visibility.
    assert_eq!(opened.state.active_popover(), Some(Popover::SystemMenu));
    assert_eq!(hidden.effects, vec![Effect::PersistConfiguration]);
    assert!(
        hidden
            .state
            .topbar_modules()
            .iter()
            .any(|module| module.kind() == TopbarModuleKind::Network && !module.visible())
    );
    Ok(())
}

#[test]
fn repeated_hide_is_an_explicit_noop() -> Result<(), Box<dyn std::error::Error>> {
    // Given: autohide is enabled and the dock is already hidden.
    let state = reduce(&ShellState::default(), ShellEvent::EnableAutohide(true))?.state;
    let hidden = reduce(&state, ShellEvent::HideDock)?.state;

    // When: another hide event arrives.
    let transition = reduce(&hidden, ShellEvent::HideDock)?;

    // Then: the reducer names the no-op.
    assert_eq!(
        transition.outcome,
        TransitionOutcome::NoOp(NoOpReason::AlreadyHidden)
    );
    assert!(transition.effects.is_empty());
    Ok(())
}

#[test]
fn empty_display_change_is_rejected_without_mutating_state() {
    // Given: a valid state.
    let state = ShellState::default();

    // When: the platform reports an impossible empty display topology.
    let result = reduce(&state, ShellEvent::DisplaysChanged(Vec::new()));

    // Then: the transition is explicitly rejected.
    assert_eq!(result, Err(TransitionError::NoMonitors));
    assert!(state.validate().is_ok());
}

#[test]
fn display_change_and_safe_mode_choose_a_primary_surface() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a normal shell state.
    let state = ShellState::default();
    let monitors = vec![
        Monitor::new(MonitorId::new(2), false),
        Monitor::new(MonitorId::new(3), true),
    ];

    // When: displays change and safe mode is entered.
    let changed = reduce(&state, ShellEvent::DisplaysChanged(monitors))?;
    let safe = reduce(&changed.state, ShellEvent::EnterSafeMode)?;

    // Then: surfaces rebuild for monitor 3 and safe mode disables invasive behavior.
    assert_eq!(changed.state.active_monitor(), MonitorId::new(3));
    assert!(changed.effects.contains(&Effect::RebuildSurfaces));
    assert!(safe.state.safe_mode());
    assert_eq!(safe.state.taskbar_policy(), TaskbarPolicy::Off);
    assert!(safe.state.accessibility().reduced_motion());
    Ok(())
}

#[test]
fn unknown_dock_item_is_an_error() {
    // Given: an empty dock.
    let state = ShellState::default();

    // When: an unknown item is activated.
    let result = reduce(&state, ShellEvent::ActivateDockItem(DockItemId::new(404)));

    // Then: the reducer returns a typed error.
    assert_eq!(
        result,
        Err(TransitionError::UnknownDockItem(DockItemId::new(404)))
    );
}
