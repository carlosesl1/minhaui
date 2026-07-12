use crate::events::{applied, no_op};
use crate::{
    DockItem, DockItemId, Effect, NoOpReason, PinState, RunningState, ShellState, TransitionError,
    TransitionOutcome, WindowId,
};

pub(crate) fn activate(
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

pub(crate) fn window_opened(
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

pub(crate) fn window_discovered(
    state: &mut ShellState,
    item: DockItem,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    if state.dock_items.iter().any(|current| current.id == item.id) {
        return Err(TransitionError::DuplicateDockItem(item.id));
    }
    state.dock_items.push(item);
    Ok(applied(Vec::new()))
}

pub(crate) fn window_changed(
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

pub(crate) fn window_closed(
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

pub(crate) fn pin(
    state: &mut ShellState,
    mut item: DockItem,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    if let Some(existing) = state
        .dock_items
        .iter_mut()
        .find(|current| current.id == item.id)
    {
        if existing.app != item.app {
            return Err(TransitionError::DuplicateDockItem(item.id));
        }
        if existing.pin == PinState::Pinned {
            return Ok(no_op(NoOpReason::AlreadyConfigured));
        }
        existing.pin = PinState::Pinned;
        return Ok(applied(vec![Effect::PersistConfiguration]));
    }
    item.pin = PinState::Pinned;
    state.dock_items.push(item);
    Ok(applied(vec![Effect::PersistConfiguration]))
}

pub(crate) fn unpin(state: &mut ShellState, id: DockItemId) -> (Vec<Effect>, TransitionOutcome) {
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

pub(crate) fn reorder(
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
