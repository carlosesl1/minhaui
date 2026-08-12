#![deny(unsafe_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use shell_core::{DockItem, DockItemId, DockLayoutEntry, PinState, RunningState, ShellState};
use shell_renderer::{DockIcon, DockItemVisual, RunningIndicator};

use crate::dock_icon_worker::{
    DockIconCandidate, DockIconResolutionRequest, DockIconResolutionResult,
};

#[derive(Default)]
pub(crate) struct DockIconSourceCache {
    entries: HashMap<DockItemId, CachedIconSource>,
    pending_request: Option<DockIconResolutionRequest>,
    latest_generation: u64,
}

struct CachedIconSource {
    fingerprint: String,
    icon: DockIcon,
    candidate: DockIconCandidate,
    state: IconResolutionState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IconResolutionState {
    Unresolved,
    Pending(u64),
    Resolved,
}

static NEXT_ICON_GENERATION: AtomicU64 = AtomicU64::new(1);

impl DockIconSourceCache {
    pub(crate) fn take_pending_request(&mut self) -> Option<DockIconResolutionRequest> {
        self.pending_request.take()
    }

    pub(crate) fn complete(&mut self, result: &DockIconResolutionResult) -> bool {
        if result.generation() != self.latest_generation {
            return false;
        }
        let mut changed = false;
        for resolved in result.icons() {
            let Some(cached) = self.entries.get_mut(&resolved.item()) else {
                continue;
            };
            if cached.fingerprint != resolved.fingerprint()
                || cached.state != IconResolutionState::Pending(result.generation())
            {
                continue;
            }
            let icon = resolved
                .source()
                .map_or(DockIcon::SystemFallback, |source| {
                    DockIcon::windows_executable(source)
                });
            changed |= cached.icon != icon;
            cached.icon = icon;
            cached.state = IconResolutionState::Resolved;
        }
        changed
    }

    pub(crate) fn abandon(&mut self, generation: u64) {
        if generation != self.latest_generation {
            return;
        }
        for entry in self.entries.values_mut() {
            if entry.state == IconResolutionState::Pending(generation) {
                entry.state = IconResolutionState::Resolved;
            }
        }
    }

    fn queue_unresolved(&mut self) {
        if !self
            .entries
            .values()
            .any(|entry| entry.state == IconResolutionState::Unresolved)
        {
            return;
        }
        self.latest_generation = NEXT_ICON_GENERATION.fetch_add(1, Ordering::Relaxed).max(1);
        let generation = self.latest_generation;
        let mut candidates = Vec::new();
        for entry in self.entries.values_mut() {
            if entry.state != IconResolutionState::Resolved {
                entry.state = IconResolutionState::Pending(generation);
                candidates.push(entry.candidate.clone());
            }
        }
        candidates.sort_by_key(|candidate| candidate.item().value());
        self.pending_request = Some(DockIconResolutionRequest::new(generation, candidates));
    }
}

pub(crate) fn visual_items(
    state: &ShellState,
    layout: &[DockLayoutEntry],
    launch_targets: &HashMap<DockItemId, String>,
    icon_sources: &mut DockIconSourceCache,
) -> Vec<DockItemVisual> {
    icon_sources
        .entries
        .retain(|id, _| state.dock_items().iter().any(|item| item.id() == *id));
    let visual_layout = visual_layout_entries(state, layout);
    let mut visuals = Vec::with_capacity(visual_layout.len());
    for entry in visual_layout {
        match entry {
            DockLayoutEntry::App(id) => {
                if let Some(item) = state.dock_items().iter().find(|item| item.id() == id) {
                    visuals.push(app_visual(item, launch_targets, icon_sources));
                }
            }
            DockLayoutEntry::Separator(id) => {
                visuals.push(DockItemVisual::separator(id.value()));
            }
        }
    }
    icon_sources.queue_unresolved();
    visuals
}

pub(crate) fn visual_layout_entries(
    state: &ShellState,
    layout: &[DockLayoutEntry],
) -> Vec<DockLayoutEntry> {
    let mut entries = layout.to_vec();
    let unpinned = state
        .dock_items()
        .iter()
        .filter(|item| {
            item.pin() == PinState::Unpinned && !layout.contains(&DockLayoutEntry::App(item.id()))
        })
        .map(|item| DockLayoutEntry::App(item.id()))
        .collect::<Vec<_>>();
    let has_running_separator = entries.iter().any(|entry| {
        matches!(entry, DockLayoutEntry::Separator(_)) && !state.dock_layout().contains(entry)
    });
    if !unpinned.is_empty() && !entries.is_empty() && !has_running_separator {
        entries.push(DockLayoutEntry::Separator(
            shell_core::DockSeparatorId::new(synthetic_separator_id(state, &entries)),
        ));
    }
    entries.extend(unpinned);
    entries
}

fn synthetic_separator_id(state: &ShellState, layout: &[DockLayoutEntry]) -> u64 {
    let mut candidate = u64::MAX;
    while state
        .dock_items()
        .iter()
        .any(|item| item.id().value() == candidate)
        || layout.iter().any(|entry| entry.visual_id() == candidate)
    {
        candidate = candidate.saturating_sub(1);
    }
    candidate
}

fn app_visual(
    item: &DockItem,
    launch_targets: &HashMap<DockItemId, String>,
    icon_sources: &mut DockIconSourceCache,
) -> DockItemVisual {
    DockItemVisual::app_with_icon(
        item.id().value(),
        visual_label(item),
        indicator(item),
        visual_icon(item, launch_targets, icon_sources),
    )
}

fn visual_icon(
    item: &DockItem,
    launch_targets: &HashMap<DockItemId, String>,
    icon_sources: &mut DockIconSourceCache,
) -> DockIcon {
    let window = match item.running() {
        RunningState::Running { window, .. } => Some(*window),
        RunningState::Stopped => None,
    };
    let launch_target = launch_targets.get(&item.id()).cloned();
    let fingerprint = icon_fingerprint(item, window, launch_target.as_deref());
    if let Some(cached) = icon_sources.entries.get(&item.id())
        && cached.fingerprint == fingerprint
    {
        return cached.icon.clone();
    }
    let candidate = DockIconCandidate::new(item.id(), fingerprint.clone(), window, launch_target);
    let icon = DockIcon::SystemFallback;
    icon_sources.entries.insert(
        item.id(),
        CachedIconSource {
            fingerprint,
            icon: icon.clone(),
            candidate,
            state: IconResolutionState::Unresolved,
        },
    );
    icon
}

fn icon_fingerprint(
    item: &DockItem,
    window: Option<shell_core::WindowId>,
    launch_target: Option<&str>,
) -> String {
    format!(
        "{}|{}|{}",
        item.app().as_str(),
        window.map_or(0, shell_core::WindowId::value),
        launch_target.unwrap_or_default()
    )
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use shell_core::{AppId, DockItem, DockItemId, ShellState, WindowId};
    use shell_renderer::DockIcon;

    use super::{DockIconSourceCache, IconResolutionState, icon_fingerprint, visual_items};
    use crate::dock_icon_worker::{DockIconResolutionResult, ResolvedDockIcon};

    #[test]
    fn stable_icon_cache_queues_one_batch_and_returns_a_fallback_immediately() {
        let item = DockItem::running_unpinned(
            DockItemId::new(7),
            AppId::parse("browser.exe").expect("valid app id"),
            WindowId::new(42),
            true,
            false,
        );
        let state = ShellState::default().with_dock_items(vec![item]);
        let mut cache = DockIconSourceCache::default();

        for _ in 0..3 {
            let visuals = visual_items(&state, state.dock_layout(), &HashMap::new(), &mut cache);
            assert_eq!(visuals.len(), 1);
            assert_eq!(visuals[0].icon(), &DockIcon::SystemFallback);
        }

        let request = cache
            .take_pending_request()
            .unwrap_or_else(|| panic!("first visual pass did not queue icon resolution"));
        assert!(request.generation() > 0);
        assert!(cache.take_pending_request().is_none());
        assert_eq!(
            cache
                .entries
                .get(&DockItemId::new(7))
                .map(|entry| entry.state),
            Some(IconResolutionState::Pending(request.generation()))
        );
    }

    #[test]
    fn current_completion_promotes_placeholder_and_stale_completion_is_ignored() {
        let item = DockItem::running_unpinned(
            DockItemId::new(7),
            AppId::parse("browser.exe").expect("valid app id"),
            WindowId::new(42),
            true,
            false,
        );
        let state = ShellState::default().with_dock_items(vec![item]);
        let mut cache = DockIconSourceCache::default();
        let _ = visual_items(&state, state.dock_layout(), &HashMap::new(), &mut cache);
        let request = cache
            .take_pending_request()
            .unwrap_or_else(|| panic!("icon request missing"));
        let fingerprint = cache
            .entries
            .get(&DockItemId::new(7))
            .map(|entry| entry.fingerprint.clone())
            .unwrap_or_else(|| panic!("cached item missing"));

        let stale = DockIconResolutionResult::new(
            request.generation() + 1,
            vec![ResolvedDockIcon::new(
                DockItemId::new(7),
                fingerprint.clone(),
                Some("wrong.exe".to_owned()),
            )],
        );
        assert!(!cache.complete(&stale));
        let current = DockIconResolutionResult::new(
            request.generation(),
            vec![ResolvedDockIcon::new(
                DockItemId::new(7),
                fingerprint,
                Some(r"C:\Apps\Browser\browser.exe".to_owned()),
            )],
        );
        assert!(cache.complete(&current));

        let visuals = visual_items(&state, state.dock_layout(), &HashMap::new(), &mut cache);
        assert_eq!(
            visuals[0].icon(),
            &DockIcon::windows_executable(r"C:\Apps\Browser\browser.exe")
        );
        assert!(cache.take_pending_request().is_none());
    }

    #[test]
    fn unavailable_worker_resolves_the_placeholder_without_requeueing() {
        let item = DockItem::pinned(
            DockItemId::new(7),
            AppId::parse("browser.exe").expect("valid app id"),
        );
        let state = ShellState::default().with_dock_items(vec![item]);
        let mut targets = HashMap::new();
        targets.insert(DockItemId::new(7), "browser.exe".to_owned());
        let mut cache = DockIconSourceCache::default();
        let _ = visual_items(&state, state.dock_layout(), &targets, &mut cache);
        let request = cache
            .take_pending_request()
            .unwrap_or_else(|| panic!("icon request missing"));

        cache.abandon(request.generation());
        let visuals = visual_items(&state, state.dock_layout(), &targets, &mut cache);

        assert_eq!(visuals[0].icon(), &DockIcon::SystemFallback);
        assert!(cache.take_pending_request().is_none());
    }

    #[test]
    fn icon_cache_fingerprint_tracks_app_window_and_launch_target() {
        let item = DockItem::running_unpinned(
            DockItemId::new(1),
            AppId::parse("vivaldi.exe").expect("valid app id"),
            WindowId::new(42),
            false,
            false,
        );
        let other_item = DockItem::running_unpinned(
            DockItemId::new(1),
            AppId::parse("other.exe").expect("valid app id"),
            WindowId::new(42),
            false,
            false,
        );
        let window = Some(WindowId::new(42));
        let target = Some("calculator-package");

        assert_ne!(
            icon_fingerprint(&item, window, target),
            icon_fingerprint(&item, None, target)
        );
        assert_ne!(
            icon_fingerprint(&item, window, target),
            icon_fingerprint(&other_item, window, target)
        );
        assert_ne!(
            icon_fingerprint(&item, window, target),
            icon_fingerprint(&item, window, Some("other"))
        );
    }
}
