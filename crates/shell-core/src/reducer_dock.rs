use crate::events::{applied, no_op};
use crate::{
    DockItem, DockItemId, DockLayoutEntry, DockSeparatorId, Effect, NoOpReason, PinState,
    RunningState, ShellState, TransitionError, TransitionOutcome, WindowId,
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
    if let Some(item) = state.dock_items.last()
        && item.pin == PinState::Pinned
    {
        state.dock_layout.push(DockLayoutEntry::App(item.id));
    }
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
        let removed = state.dock_items.remove(index);
        remove_app_from_layout(state, removed.id);
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
        if !state
            .dock_layout
            .contains(&DockLayoutEntry::App(existing.id))
        {
            state.dock_layout.push(DockLayoutEntry::App(existing.id));
        }
        return Ok(applied(vec![Effect::PersistConfiguration]));
    }
    item.pin = PinState::Pinned;
    state.dock_layout.push(DockLayoutEntry::App(item.id));
    state.dock_items.push(item);
    Ok(applied(vec![Effect::PersistConfiguration]))
}

pub(crate) fn unpin(state: &mut ShellState, id: DockItemId) -> (Vec<Effect>, TransitionOutcome) {
    let Some(index) = state.dock_items.iter().position(|item| item.id == id) else {
        return no_op(NoOpReason::ItemNotPresent);
    };
    if matches!(state.dock_items[index].running, RunningState::Stopped) {
        let removed = state.dock_items.remove(index);
        remove_app_from_layout(state, removed.id);
    } else {
        state.dock_items[index].pin = PinState::Unpinned;
        remove_app_from_layout(state, id);
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
    if state.dock_layout.contains(&DockLayoutEntry::App(id)) {
        let before = before.map(DockLayoutEntry::App);
        move_layout_entry(state, DockLayoutEntry::App(id), before)?;
    }
    Ok(applied(vec![Effect::PersistConfiguration]))
}

pub(crate) fn add_separator(
    state: &mut ShellState,
    separator: DockSeparatorId,
    before: Option<DockLayoutEntry>,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    let entry = DockLayoutEntry::Separator(separator);
    if state.dock_layout.contains(&entry) {
        return Err(TransitionError::DuplicateDockSeparator(separator));
    }
    let target = layout_target_index(state, before)?;
    state.dock_layout.insert(target, entry);
    Ok(applied(vec![Effect::PersistConfiguration]))
}

pub(crate) fn remove_separator(
    state: &mut ShellState,
    separator: DockSeparatorId,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    let entry = DockLayoutEntry::Separator(separator);
    let index = state
        .dock_layout
        .iter()
        .position(|current| *current == entry)
        .ok_or(TransitionError::UnknownDockLayoutEntry(entry))?;
    state.dock_layout.remove(index);
    Ok(applied(vec![Effect::PersistConfiguration]))
}

pub(crate) fn reorder_entry(
    state: &mut ShellState,
    entry: DockLayoutEntry,
    before: Option<DockLayoutEntry>,
) -> Result<(Vec<Effect>, TransitionOutcome), TransitionError> {
    if before == Some(entry) {
        return Ok(no_op(NoOpReason::AlreadyConfigured));
    }
    move_layout_entry(state, entry, before)?;
    sync_pinned_item_order(state);
    Ok(applied(vec![Effect::PersistConfiguration]))
}

fn move_layout_entry(
    state: &mut ShellState,
    entry: DockLayoutEntry,
    before: Option<DockLayoutEntry>,
) -> Result<(), TransitionError> {
    let source = state
        .dock_layout
        .iter()
        .position(|current| *current == entry)
        .ok_or(TransitionError::UnknownDockLayoutEntry(entry))?;
    let entry = state.dock_layout.remove(source);
    let target = layout_target_index(state, before)?;
    state.dock_layout.insert(target, entry);
    Ok(())
}

fn layout_target_index(
    state: &ShellState,
    before: Option<DockLayoutEntry>,
) -> Result<usize, TransitionError> {
    before.map_or(Ok(state.dock_layout.len()), |target| {
        state
            .dock_layout
            .iter()
            .position(|current| *current == target)
            .ok_or(TransitionError::UnknownDockLayoutEntry(target))
    })
}

fn sync_pinned_item_order(state: &mut ShellState) {
    let ranks = state
        .dock_layout
        .iter()
        .enumerate()
        .filter_map(|(rank, entry)| match entry {
            DockLayoutEntry::App(id) => Some((*id, rank)),
            DockLayoutEntry::Separator(_) => None,
        })
        .collect::<std::collections::HashMap<_, _>>();
    state
        .dock_items
        .sort_by_key(|item| ranks.get(&item.id).copied().unwrap_or(usize::MAX));
}

fn remove_app_from_layout(state: &mut ShellState, id: DockItemId) {
    state
        .dock_layout
        .retain(|entry| *entry != DockLayoutEntry::App(id));
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
