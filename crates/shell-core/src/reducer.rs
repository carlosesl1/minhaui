use crate::events::{applied, no_op};
use crate::{
    DockItem, DockItemId, DockVisibility, Effect, Monitor, NoOpReason, PerformancePreset, PinState,
    RunningState, ShellEvent, ShellState, TaskbarPolicy, TopbarModuleKind, Transition,
    TransitionError, TransitionOutcome, WindowId,
};

/// Reduces one event without performing I/O.
pub fn reduce(state: &ShellState, event: ShellEvent) -> Result<Transition, TransitionError> {
    state.validate().map_err(TransitionError::InvalidState)?;
    let mut next = state.clone();
    let (effects, outcome) = match event {
        ShellEvent::ActivateDockItem(id) => activate(&mut next, id)?,
        ShellEvent::WindowOpened { item, window } => window_opened(&mut next, item, window)?,
        ShellEvent::WindowChanged {
            item,
            window,
            focused,
            minimized,
        } => window_changed(&mut next, item, window, focused, minimized)?,
        ShellEvent::WindowDiscovered(item) => window_discovered(&mut next, item)?,
        ShellEvent::WindowClosed(id) => window_closed(&mut next, id)?,
        ShellEvent::Pin(item) => pin(&mut next, item)?,
        ShellEvent::Unpin(id) => unpin(&mut next, id),
        ShellEvent::ReorderDockItem { item, before } => reorder(&mut next, item, before)?,
        ShellEvent::OpenPopover(popover) => {
            if next.active_popover == Some(popover) {
                no_op(NoOpReason::AlreadyConfigured)
            } else {
                next.active_popover = Some(popover);
                applied(Vec::new())
            }
        }
        ShellEvent::DismissPopover => {
            if next.active_popover.take().is_some() {
                applied(Vec::new())
            } else {
                no_op(NoOpReason::NoActivePopover)
            }
        }
        ShellEvent::EnableAutohide(enabled) => set_autohide(&mut next, enabled),
        ShellEvent::HideDock => hide(&mut next),
        ShellEvent::RevealDock => reveal(&mut next),
        ShellEvent::DisplaysChanged(monitors) => displays_changed(&mut next, monitors)?,
        ShellEvent::SetTaskbarPolicy(policy) => set_taskbar(&mut next, policy),
        ShellEvent::SetTopbarVisibility { module, visible } => {
            set_topbar(&mut next, module, visible)?
        }
        ShellEvent::SetPerformance(preset) => {
            if next.performance == preset {
                no_op(NoOpReason::AlreadyConfigured)
            } else {
                next.performance = preset;
                applied(vec![Effect::PersistConfiguration])
            }
        }
        ShellEvent::EnterSafeMode => safe_mode(&mut next),
    };
    next.validate().map_err(TransitionError::InvalidState)?;
    Ok(Transition {
        state: next,
        effects,
        outcome,
    })
}

fn activate(
    state: &mut ShellState,
    id: DockItemId,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    let item = find_item(state, id)?;
    match &mut item.running {
        RunningState::Stopped => Ok(applied(vec![Effect::Launch(item.app.clone())])),
        RunningState::Running {
            window,
            focused,
            minimized,
        } => {
            if *focused && !*minimized {
                *focused = false;
                *minimized = true;
                Ok(applied(vec![Effect::MinimizeWindow(*window)]))
            } else {
                *focused = true;
                *minimized = false;
                Ok(applied(vec![Effect::FocusWindow(*window)]))
            }
        }
    }
}

fn window_opened(
    state: &mut ShellState,
    id: DockItemId,
    window: WindowId,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    let item = find_item(state, id)?;
    item.running = RunningState::Running {
        window,
        focused: false,
        minimized: false,
    };
    Ok(applied(Vec::new()))
}

fn window_discovered(
    state: &mut ShellState,
    item: DockItem,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    if state.dock_items.iter().any(|current| current.id == item.id) {
        return Err(TransitionError::DuplicateDockItem(item.id));
    }
    state.dock_items.push(item);
    Ok(applied(Vec::new()))
}

fn window_changed(
    state: &mut ShellState,
    id: DockItemId,
    window: WindowId,
    focused: bool,
    minimized: bool,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    let item = find_item(state, id)?;
    item.running = RunningState::Running {
        window,
        focused,
        minimized,
    };
    Ok(applied(Vec::new()))
}

fn window_closed(
    state: &mut ShellState,
    id: DockItemId,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    let index = item_index(state, id)?;
    if state.dock_items[index].pin == PinState::Pinned {
        state.dock_items[index].running = RunningState::Stopped;
    } else {
        state.dock_items.remove(index);
    }
    Ok(applied(Vec::new()))
}

fn pin(
    state: &mut ShellState,
    mut item: DockItem,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    if state.dock_items.iter().any(|current| current.id == item.id) {
        return Err(TransitionError::DuplicateDockItem(item.id));
    }
    item.pin = PinState::Pinned;
    state.dock_items.push(item);
    Ok(applied(vec![Effect::PersistConfiguration]))
}

fn unpin(state: &mut ShellState, id: DockItemId) -> (Vec<Effect>, TransitionOutcome) {
    let Some(index) = state.dock_items.iter().position(|item| item.id == id) else {
        return no_op(NoOpReason::ItemNotPresent);
    };
    if matches!(state.dock_items[index].running, RunningState::Stopped) {
        state.dock_items.remove(index);
    } else {
        state.dock_items[index].pin = PinState::Unpinned;
    }
    applied(vec![Effect::PersistConfiguration])
}

fn reorder(
    state: &mut ShellState,
    id: DockItemId,
    before: Option<DockItemId>,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    let source = item_index(state, id)?;
    if before == Some(id) {
        return Ok(no_op(NoOpReason::AlreadyConfigured));
    }
    let item = state.dock_items.remove(source);
    let target = match before {
        Some(target_id) => state
            .dock_items
            .iter()
            .position(|current| current.id == target_id)
            .ok_or(TransitionError::UnknownDockItem(target_id))?,
        None => state.dock_items.len(),
    };
    state.dock_items.insert(target, item);
    Ok(applied(vec![Effect::PersistConfiguration]))
}

fn set_autohide(state: &mut ShellState, enabled: bool) -> (Vec<Effect>, TransitionOutcome) {
    if state.dock.enabled == enabled {
        return no_op(NoOpReason::AlreadyConfigured);
    }
    state.dock.enabled = enabled;
    if !enabled {
        state.dock.visibility = DockVisibility::Revealed;
    }
    applied(vec![Effect::PersistConfiguration])
}

fn hide(state: &mut ShellState) -> (Vec<Effect>, TransitionOutcome) {
    if !state.dock.enabled {
        return no_op(NoOpReason::AlreadyConfigured);
    }
    if state.dock.visibility == DockVisibility::Hidden {
        return no_op(NoOpReason::AlreadyHidden);
    }
    state.dock.visibility = DockVisibility::Hidden;
    applied(Vec::new())
}

fn reveal(state: &mut ShellState) -> (Vec<Effect>, TransitionOutcome) {
    if state.dock.visibility == DockVisibility::Revealed {
        return no_op(NoOpReason::AlreadyRevealed);
    }
    state.dock.visibility = DockVisibility::Revealed;
    applied(Vec::new())
}

fn displays_changed(
    state: &mut ShellState,
    monitors: Vec<Monitor>,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    if monitors.is_empty() {
        return Err(TransitionError::NoMonitors);
    }
    if monitors.iter().filter(|monitor| monitor.primary).count() != 1 {
        return Err(TransitionError::InvalidMonitorTopology);
    }
    let active = monitors
        .iter()
        .find(|monitor| monitor.primary)
        .map(|monitor| monitor.id)
        .ok_or(TransitionError::InvalidMonitorTopology)?;
    state.monitors = monitors;
    state.active_monitor = active;
    Ok(applied(vec![Effect::RebuildSurfaces]))
}

fn set_taskbar(state: &mut ShellState, policy: TaskbarPolicy) -> (Vec<Effect>, TransitionOutcome) {
    if state.taskbar_policy == policy {
        return no_op(NoOpReason::AlreadyConfigured);
    }
    state.taskbar_policy = policy;
    applied(vec![
        Effect::ApplyTaskbarPolicy(policy),
        Effect::PersistConfiguration,
    ])
}

fn set_topbar(
    state: &mut ShellState,
    kind: TopbarModuleKind,
    visible: bool,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    let module = state
        .topbar
        .iter_mut()
        .find(|module| module.kind == kind)
        .ok_or(TransitionError::UnknownTopbarModule(kind))?;
    if module.visible == visible {
        return Ok(no_op(NoOpReason::AlreadyConfigured));
    }
    module.visible = visible;
    Ok(applied(vec![Effect::PersistConfiguration]))
}

fn safe_mode(state: &mut ShellState) -> (Vec<Effect>, TransitionOutcome) {
    if state.safe_mode {
        return no_op(NoOpReason::AlreadyConfigured);
    }
    state.safe_mode = true;
    state.taskbar_policy = TaskbarPolicy::Off;
    state.accessibility.reduced_motion = true;
    state.accessibility.transparency = false;
    state.performance = PerformancePreset::BatterySaver;
    state.dock.visibility = DockVisibility::Revealed;
    state.active_popover = None;
    applied(vec![
        Effect::ApplyTaskbarPolicy(TaskbarPolicy::Off),
        Effect::RebuildSurfaces,
    ])
}

fn find_item(state: &mut ShellState, id: DockItemId) -> Result<&mut DockItem, TransitionError> {
    state
        .dock_items
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or(TransitionError::UnknownDockItem(id))
}

fn item_index(state: &ShellState, id: DockItemId) -> Result<usize, TransitionError> {
    state
        .dock_items
        .iter()
        .position(|item| item.id == id)
        .ok_or(TransitionError::UnknownDockItem(id))
}
