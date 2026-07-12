#![deny(unsafe_code)]

use shell_core::{AppId, DockItem, DockItemId, RunningState, ShellEvent};

use crate::dock_window_sync::stable_item_id;
use crate::{DockController, DockControllerError, ObservedWindow, QueuedDockAction};

impl DockController {
    pub fn sync_running_windows(
        &mut self,
        observed: &[ObservedWindow],
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let mut actions = Vec::new();
        for window in observed {
            let item = if let Some(item) = self.item_for_app(window.app()) {
                item
            } else {
                self.discover_running_window(window, &mut actions)?
            };
            actions.extend(self.apply(ShellEvent::WindowChanged {
                item,
                window: window.window(),
                focused: window.foreground(),
                minimized: window.minimized(),
            })?);
        }
        for item in self.stale_running_items(observed) {
            actions.extend(self.apply(ShellEvent::WindowClosed(item))?);
            if !self.is_dock_item(item) {
                self.launch_targets.remove(&item);
            }
        }
        Ok(actions)
    }

    fn discover_running_window(
        &mut self,
        window: &ObservedWindow,
        actions: &mut Vec<QueuedDockAction>,
    ) -> Result<DockItemId, DockControllerError> {
        let item = self.stable_item_for_app(window.app());
        let discovered = DockItem::running_unpinned(
            item,
            window.app().clone(),
            window.window(),
            window.foreground(),
            window.minimized(),
        );
        actions.extend(self.apply(ShellEvent::WindowDiscovered(discovered))?);
        self.launch_targets
            .insert(item, window.app().as_str().to_owned());
        Ok(item)
    }

    fn item_for_app(&self, app: &AppId) -> Option<DockItemId> {
        self.state
            .dock_items()
            .iter()
            .find(|item| item.app() == app)
            .map(DockItem::id)
    }

    fn stable_item_for_app(&self, app: &AppId) -> DockItemId {
        let mut id = stable_item_id(app);
        while self.state.dock_items().iter().any(|item| item.id() == id) {
            id = DockItemId::new(id.value().wrapping_add(1));
        }
        id
    }

    fn stale_running_items(&self, observed: &[ObservedWindow]) -> Vec<DockItemId> {
        self.state
            .dock_items()
            .iter()
            .filter_map(|item| match item.running() {
                RunningState::Running { window, .. }
                    if !observed.iter().any(|current| current.window() == *window) =>
                {
                    Some(item.id())
                }
                _ => None,
            })
            .collect()
    }
}
