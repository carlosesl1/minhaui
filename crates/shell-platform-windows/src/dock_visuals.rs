#![deny(unsafe_code)]

use std::collections::HashMap;

use shell_core::{DockItem, DockItemId, DockLayoutEntry, PinState, RunningState, ShellState};
use shell_renderer::{DockIcon, DockItemVisual, RunningIndicator};

#[derive(Default)]
pub(crate) struct DockIconSourceCache {
    entries: HashMap<DockItemId, CachedIconSource>,
}

struct CachedIconSource {
    fingerprint: String,
    icon: DockIcon,
}

trait IconSourceResolver {
    fn window_icon_source(&self, window: shell_core::WindowId) -> Option<String>;
    fn resolve_icon_source(&self, source: &str) -> Option<String>;
}

struct WindowsIconSourceResolver;

impl IconSourceResolver for WindowsIconSourceResolver {
    fn window_icon_source(&self, window: shell_core::WindowId) -> Option<String> {
        crate::win32_app_identity::window_icon_source(window)
    }

    fn resolve_icon_source(&self, source: &str) -> Option<String> {
        crate::win32_app_identity::resolve_icon_source(source)
    }
}

pub(crate) fn visual_items(
    state: &ShellState,
    layout: &[DockLayoutEntry],
    launch_targets: &HashMap<DockItemId, String>,
    icon_sources: &mut DockIconSourceCache,
) -> Vec<DockItemVisual> {
    visual_items_with_resolver(
        state,
        layout,
        launch_targets,
        icon_sources,
        &WindowsIconSourceResolver,
    )
}

fn visual_items_with_resolver(
    state: &ShellState,
    layout: &[DockLayoutEntry],
    launch_targets: &HashMap<DockItemId, String>,
    icon_sources: &mut DockIconSourceCache,
    resolver: &impl IconSourceResolver,
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
                    visuals.push(app_visual(item, launch_targets, icon_sources, resolver));
                }
            }
            DockLayoutEntry::Separator(id) => {
                visuals.push(DockItemVisual::separator(id.value()));
            }
        }
    }
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
    resolver: &impl IconSourceResolver,
) -> DockItemVisual {
    DockItemVisual::app_with_icon(
        item.id().value(),
        visual_label(item),
        indicator(item),
        visual_icon(item, launch_targets, icon_sources, resolver),
    )
}

fn visual_icon(
    item: &DockItem,
    launch_targets: &HashMap<DockItemId, String>,
    icon_sources: &mut DockIconSourceCache,
    resolver: &impl IconSourceResolver,
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
    let window_source = window.and_then(|window| resolver.window_icon_source(window));
    let source = preferred_icon_source(item.pin(), launch_target, window_source);
    let icon = source
        .as_deref()
        .and_then(|source| resolver.resolve_icon_source(source))
        .or(source)
        .map_or(DockIcon::SystemFallback, |source| {
            DockIcon::windows_executable(&source)
        });
    icon_sources.entries.insert(
        item.id(),
        CachedIconSource {
            fingerprint,
            icon: icon.clone(),
        },
    );
    icon
}

fn preferred_icon_source(
    _pin: PinState,
    launch_target: Option<String>,
    window_source: Option<String>,
) -> Option<String> {
    window_source.or(launch_target)
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
    use std::cell::Cell;
    use std::collections::HashMap;

    use shell_core::{AppId, DockItem, DockItemId, PinState, ShellState, WindowId};

    use super::{
        DockIconSourceCache, IconSourceResolver, icon_fingerprint, preferred_icon_source,
        visual_items_with_resolver,
    };

    struct CountingResolver {
        window_calls: Cell<usize>,
        canonical_calls: Cell<usize>,
    }

    impl IconSourceResolver for CountingResolver {
        fn window_icon_source(&self, _window: WindowId) -> Option<String> {
            self.window_calls.set(self.window_calls.get() + 1);
            Some(r"C:\Apps\Browser\browser.exe".to_owned())
        }

        fn resolve_icon_source(&self, source: &str) -> Option<String> {
            self.canonical_calls.set(self.canonical_calls.get() + 1);
            Some(source.to_owned())
        }
    }

    #[test]
    fn stable_icon_cache_skips_identity_and_package_resolution_on_later_frames() {
        let item = DockItem::running_unpinned(
            DockItemId::new(7),
            AppId::parse("browser.exe").expect("valid app id"),
            WindowId::new(42),
            true,
            false,
        );
        let state = ShellState::default().with_dock_items(vec![item]);
        let mut cache = DockIconSourceCache::default();
        let resolver = CountingResolver {
            window_calls: Cell::new(0),
            canonical_calls: Cell::new(0),
        };

        for _ in 0..3 {
            let visuals = visual_items_with_resolver(
                &state,
                state.dock_layout(),
                &HashMap::new(),
                &mut cache,
                &resolver,
            );
            assert_eq!(visuals.len(), 1);
        }

        assert_eq!(resolver.window_calls.get(), 1);
        assert_eq!(resolver.canonical_calls.get(), 1);
    }

    #[test]
    fn pinned_running_app_uses_its_canonical_window_icon_source() {
        let source = preferred_icon_source(
            PinState::Pinned,
            Some("vivaldi.exe".to_owned()),
            Some("C:\\Apps\\Vivaldi\\vivaldi.exe".to_owned()),
        );

        assert_eq!(source.as_deref(), Some("C:\\Apps\\Vivaldi\\vivaldi.exe"));
    }

    #[test]
    fn pinning_a_running_app_does_not_change_its_canonical_icon_source() {
        let launch_target = Some("vivaldi.exe".to_owned());
        let canonical_icon =
            Some("shell:AppsFolder\\\\Vivaldi.WMOW6CEHCLPDQ4UVKQKEJN7FEI".to_owned());

        let before = preferred_icon_source(
            PinState::Unpinned,
            launch_target.clone(),
            canonical_icon.clone(),
        );
        let after = preferred_icon_source(PinState::Pinned, launch_target, canonical_icon);

        assert_eq!(
            after, before,
            "pin transition replaced the branded app icon"
        );
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
