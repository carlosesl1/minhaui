#![deny(unsafe_code)]

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use shell_config::{ConfigStore, ShellConfigV1};
use shell_core::{DockItem, PinState, ShellState, TopbarModule, TopbarModuleKind};
use windows::core::Result;

static PROCESS_CONFIG: OnceLock<Mutex<ShellConfigV1>> = OnceLock::new();

pub(super) fn load_config() -> ShellConfigV1 {
    lock_config(process_config()).clone()
}

pub(super) fn state_from_config(config: &ShellConfigV1, defaults: ShellState) -> ShellState {
    configured_state(config, defaults).unwrap_or_default()
}

pub(super) fn persist_dock_state(state: &ShellState) -> Result<()> {
    let store = default_store()
        .ok_or_else(|| windows::core::Error::new(invalid_arg(), "LOCALAPPDATA is unavailable"))?;
    let mut current = lock_config(process_config());
    let config = config_with_dock_state(&current, state);
    store
        .persist(&config)
        .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
    *current = config;
    Ok(())
}

/// Persists user-facing settings without replacing a dock layout that changed
/// while the Settings window was open.
pub(super) fn persist_settings_config(config: &ShellConfigV1) -> Result<ShellConfigV1> {
    let store = default_store()
        .ok_or_else(|| windows::core::Error::new(invalid_arg(), "LOCALAPPDATA is unavailable"))?;
    let mut current = lock_config(process_config());
    let merged = merge_settings(&current, config);
    merged
        .validate()
        .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
    store
        .persist(&merged)
        .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
    *current = merged.clone();
    Ok(merged)
}

fn process_config() -> &'static Mutex<ShellConfigV1> {
    PROCESS_CONFIG.get_or_init(|| {
        let config = default_store()
            .map(|store| store.load().config().clone())
            .unwrap_or_default();
        Mutex::new(config)
    })
}

fn lock_config(mutex: &Mutex<ShellConfigV1>) -> MutexGuard<'_, ShellConfigV1> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            mutex.clear_poison();
            poisoned.into_inner()
        }
    }
}

fn merge_settings(current: &ShellConfigV1, edited: &ShellConfigV1) -> ShellConfigV1 {
    current
        .with_autohide(edited.autohide())
        .with_taskbar_policy(edited.taskbar_policy())
        .with_accessibility(edited.accessibility())
        .with_performance(edited.performance())
        .with_topbar_modules(edited.topbar_modules().to_vec())
        .with_dock(edited.dock().clone())
        .with_topbar(edited.topbar().clone())
        .with_quick_settings(edited.quick_settings().clone())
        .with_behavior(edited.behavior())
        .with_appearance(edited.appearance().clone())
        .with_advanced(edited.advanced())
}

fn configured_state(config: &ShellConfigV1, defaults: ShellState) -> Option<ShellState> {
    let layout = if config.dock_layout().is_empty() {
        None
    } else {
        Some(config.dock_layout().to_vec())
    };
    let mut state = defaults.with_dock_items(config.dock_items().to_vec());
    if let Some(layout) = layout {
        state = state.with_dock_layout(layout);
    }
    let mut topbar = Vec::with_capacity(9);
    topbar.push(TopbarModule::new(TopbarModuleKind::SystemMenu, true));
    topbar.push(TopbarModule::new(TopbarModuleKind::AppIdentity, true));
    topbar.push(TopbarModule::new(TopbarModuleKind::Search, true));
    let mut background_apps_inserted = false;
    for module in config
        .topbar_modules()
        .iter()
        .copied()
        .filter(|module| module.kind() != TopbarModuleKind::SystemMenu)
    {
        if module.kind() == TopbarModuleKind::Clock && !background_apps_inserted {
            topbar.push(TopbarModule::new(TopbarModuleKind::BackgroundApps, true));
            background_apps_inserted = true;
        }
        topbar.push(module);
    }
    if !background_apps_inserted {
        topbar.push(TopbarModule::new(TopbarModuleKind::BackgroundApps, true));
    }
    state = state.with_topbar_modules(topbar);
    state.validate().ok().map(|()| state)
}

fn config_with_dock_state(base: &ShellConfigV1, state: &ShellState) -> ShellConfigV1 {
    let items = state
        .dock_items()
        .iter()
        .filter(|item| item.pin() == PinState::Pinned)
        .map(|item| {
            let persisted = DockItem::pinned(item.id(), item.app().clone());
            match item.launch_target() {
                Some(target) => persisted.with_launch_target(target),
                None => persisted,
            }
        })
        .collect();
    base.clone()
        .with_dock_items(items)
        .with_dock_layout(state.dock_layout().to_vec())
}

fn default_store() -> Option<ConfigStore> {
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    Some(ConfigStore::new(root.join("Minha UI").join("config.json")))
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}

#[cfg(test)]
mod tests {
    use shell_config::ShellConfigV1;
    use shell_core::{AppId, DockItem, DockItemId, DockLayoutEntry, DockSeparatorId, ShellState};

    use crate::dock_launch::initial_launch_targets;

    use super::{config_with_dock_state, configured_state, merge_settings};

    #[test]
    fn dock_order_and_separators_survive_config_roundtrip() {
        let first = DockItem::pinned(DockItemId::new(1), AppId::parse("first.exe").unwrap());
        let second = DockItem::pinned(DockItemId::new(2), AppId::parse("second.exe").unwrap());
        let layout = vec![
            DockLayoutEntry::App(second.id()),
            DockLayoutEntry::Separator(DockSeparatorId::new(90)),
            DockLayoutEntry::App(first.id()),
        ];
        let state = ShellState::default()
            .with_dock_items(vec![first, second])
            .with_dock_layout(layout.clone());

        let config = config_with_dock_state(&ShellConfigV1::default(), &state);
        let restarted = configured_state(&config, ShellState::default()).unwrap();

        assert_eq!(restarted.dock_layout(), layout);
        assert!(restarted.validate().is_ok());
    }

    #[test]
    fn an_explicit_empty_config_does_not_restore_sample_apps() {
        let restarted = configured_state(&ShellConfigV1::default(), ShellState::default()).unwrap();

        assert!(restarted.dock_items().is_empty());
        assert!(restarted.dock_layout().is_empty());
    }

    #[test]
    fn absolute_launch_target_survives_config_roundtrip() {
        let target = r"C:\Program Files\Portable Apps\Browser.exe";
        let item = DockItem::pinned(DockItemId::new(7), AppId::parse("browser.exe").unwrap())
            .with_launch_target(target);
        let state = ShellState::default().with_dock_items(vec![item]);

        let config = config_with_dock_state(&ShellConfigV1::default(), &state);
        let restarted = configured_state(&config, ShellState::default()).unwrap();

        assert_eq!(restarted.dock_items()[0].launch_target(), Some(target));
        assert_eq!(
            initial_launch_targets(&restarted).get(&DockItemId::new(7)),
            Some(&target.to_owned())
        );
    }

    #[test]
    fn settings_commit_preserves_a_newer_dock_layout() {
        let first = DockItem::pinned(DockItemId::new(1), AppId::parse("first.exe").unwrap());
        let latest = ShellConfigV1::default()
            .with_dock_items(vec![first])
            .with_dock_layout(vec![DockLayoutEntry::App(DockItemId::new(1))]);
        let edited = ShellConfigV1::default()
            .with_autohide(true)
            .with_dock(shell_config::DockSettings::default().with_item_size(64));

        let merged = merge_settings(&latest, &edited);

        assert_eq!(merged.dock_items(), latest.dock_items());
        assert_eq!(merged.dock_layout(), latest.dock_layout());
        assert!(merged.autohide());
        assert_eq!(merged.dock().item_size(), 64);
    }

    #[test]
    fn every_slot_receives_fixed_leading_modules_and_persisted_status_order() {
        let modules = vec![
            shell_core::TopbarModule::new(shell_core::TopbarModuleKind::SystemMenu, true),
            shell_core::TopbarModule::new(shell_core::TopbarModuleKind::Clock, true),
            shell_core::TopbarModule::new(shell_core::TopbarModuleKind::Network, false),
            shell_core::TopbarModule::new(shell_core::TopbarModuleKind::Volume, true),
            shell_core::TopbarModule::new(shell_core::TopbarModuleKind::Power, true),
            shell_core::TopbarModule::new(shell_core::TopbarModuleKind::Notifications, true),
        ];
        let config = ShellConfigV1::default().with_topbar_modules(modules);
        let state = configured_state(&config, ShellState::default()).unwrap();
        let kinds = state
            .topbar_modules()
            .iter()
            .map(|module| module.kind())
            .collect::<Vec<_>>();

        assert_eq!(
            &kinds[..3],
            &[
                shell_core::TopbarModuleKind::SystemMenu,
                shell_core::TopbarModuleKind::AppIdentity,
                shell_core::TopbarModuleKind::Search,
            ]
        );
        assert_eq!(kinds[3], shell_core::TopbarModuleKind::BackgroundApps);
        assert_eq!(kinds[4], shell_core::TopbarModuleKind::Clock);
        assert!(state.topbar_modules().iter().any(|module| module.kind()
            == shell_core::TopbarModuleKind::Network
            && !module.visible()));
    }
}
