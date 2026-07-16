#![deny(unsafe_code)]

use std::path::PathBuf;

use shell_config::{ConfigSource, ConfigStore, ShellConfigV1};
use shell_core::{DockItem, PinState, ShellState};
use windows::core::Result;

pub(super) fn load_dock_state(defaults: ShellState) -> ShellState {
    let Some(store) = default_store() else {
        return defaults;
    };
    let loaded = store.load();
    if loaded.source() == ConfigSource::Defaults {
        return defaults;
    }
    state_from_config(loaded.config()).unwrap_or(defaults)
}

pub(super) fn persist_dock_state(state: &ShellState) -> Result<()> {
    let store = default_store()
        .ok_or_else(|| windows::core::Error::new(invalid_arg(), "LOCALAPPDATA is unavailable"))?;
    let config = config_with_dock_state(store.load().config(), state);
    store
        .persist(&config)
        .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))
}

fn state_from_config(config: &ShellConfigV1) -> Option<ShellState> {
    let layout = if config.dock_layout().is_empty() {
        None
    } else {
        Some(config.dock_layout().to_vec())
    };
    let mut state = ShellState::default().with_dock_items(config.dock_items().to_vec());
    if let Some(layout) = layout {
        state = state.with_dock_layout(layout);
    }
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

    use super::{config_with_dock_state, state_from_config};

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
        let restarted = state_from_config(&config).unwrap();

        assert_eq!(restarted.dock_layout(), layout);
        assert!(restarted.validate().is_ok());
    }

    #[test]
    fn an_explicit_empty_config_does_not_restore_sample_apps() {
        let restarted = state_from_config(&ShellConfigV1::default()).unwrap();

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
        let restarted = state_from_config(&config).unwrap();

        assert_eq!(restarted.dock_items()[0].launch_target(), Some(target));
        assert_eq!(
            initial_launch_targets(&restarted).get(&DockItemId::new(7)),
            Some(&target.to_owned())
        );
    }
}
