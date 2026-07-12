#![deny(unsafe_code)]

use shell_core::{DockItem, PinState, RunningState, ShellState};
use shell_renderer::{DockItemVisual, RunningIndicator};

const SEPARATOR_BASE: u64 = u64::MAX - 4096;

pub(crate) fn visual_items(state: &ShellState) -> Vec<DockItemVisual> {
    let mut visuals = Vec::with_capacity(state.dock_items().len() + 1);
    let mut inserted_separator = false;
    let has_pinned = state
        .dock_items()
        .iter()
        .any(|item| item.pin() == PinState::Pinned);
    for item in state.dock_items() {
        if !inserted_separator && has_pinned && item.pin() == PinState::Unpinned {
            visuals.push(DockItemVisual::separator(SEPARATOR_BASE));
            inserted_separator = true;
        }
        visuals.push(DockItemVisual::app(
            item.id().value(),
            visual_label(item),
            indicator(item),
        ));
    }
    visuals
}

fn indicator(item: &DockItem) -> RunningIndicator {
    match item.running() {
        RunningState::Stopped => RunningIndicator::Stopped,
        RunningState::Running {
            focused: true,
            minimized: false,
            ..
        } => RunningIndicator::Focused,
        RunningState::Running {
            minimized: true, ..
        } => RunningIndicator::Minimized,
        RunningState::Running { .. } => RunningIndicator::Running,
    }
}

fn visual_label(item: &DockItem) -> &str {
    match item.app().as_str() {
        "notepad.exe" | "app.notepad" => "Notes",
        "calc.exe" | "app.calculator" => "Calc",
        "explorer.exe" => "Files",
        value => value.strip_suffix(".exe").unwrap_or(value),
    }
}
