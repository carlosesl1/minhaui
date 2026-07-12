#![deny(unsafe_code)]

use shell_core::{AppId, DockItem, DockItemId, RunningState, ShellEvent, WindowId};

use crate::dock_window_sync::stable_item_id;
use crate::{
    DockController, DockControllerError, ObservedWindow, PreviewAction, PreviewQueuedAction,
    QueuedDockAction,
};

impl DockController {
    pub fn sync_running_windows(
        &mut self,
        observed: &[ObservedWindow],
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        self.sync_previews(observed);
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

    pub fn sync_running_windows_with_previews(
        &mut self,
        observed: &[ObservedWindow],
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        self.sync_running_windows(observed)
    }

    pub fn handle_preview_action(
        &self,
        window: WindowId,
        action: PreviewAction,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        if !self.previews.contains_key(&window) {
            return Ok(Vec::new());
        }
        let Some(app) = self.app_for_window(window) else {
            return Ok(Vec::new());
        };
        let queued = match action {
            PreviewAction::Focus => PreviewQueuedAction::Focus { window, app },
            PreviewAction::Close => PreviewQueuedAction::Close { window, app },
        };
        Ok(vec![QueuedDockAction::Preview(queued)])
    }

    fn sync_previews(&mut self, observed: &[ObservedWindow]) {
        self.previews.clear();
        for window in observed {
            if let Some(preview) = window.preview() {
                self.previews.insert(window.window(), preview);
            }
        }
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

    fn app_for_window(&self, target: WindowId) -> Option<AppId> {
        self.state
            .dock_items()
            .iter()
            .find_map(|item| match item.running() {
                RunningState::Running { window, .. } if *window == target => {
                    Some(item.app().clone())
                }
                RunningState::Stopped | RunningState::Running { .. } => None,
            })
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
