use crate::events::{applied, no_op};
use crate::reducer_dock::{
    activate, add_separator, pin, remove_separator, reorder, reorder_entry, unpin, window_changed,
    window_closed, window_discovered, window_opened,
};
use crate::{
    DockVisibility, Effect, Monitor, NoOpReason, PerformancePreset, ShellEvent, ShellState,
    TaskbarPolicy, TopbarModuleKind, Transition, TransitionError, TransitionOutcome,
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
        ShellEvent::AddDockSeparator { separator, before } => {
            add_separator(&mut next, separator, before)?
        }
        ShellEvent::RemoveDockSeparator(separator) => remove_separator(&mut next, separator)?,
        ShellEvent::ReorderDockEntry { entry, before } => reorder_entry(&mut next, entry, before)?,
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
