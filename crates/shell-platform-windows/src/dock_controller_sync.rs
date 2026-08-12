#![deny(unsafe_code)]

use shell_core::{AppId, DockItem, DockItemId, RunningState, ShellEvent, WindowId};

use crate::dock_window_sync::stable_item_id;
use crate::{
    DockController, DockControllerError, ObservedWindow, PreviewAction, PreviewQueuedAction,
    PreviewWindowState, QueuedDockAction,
};

impl DockController {
    pub fn sync_running_windows(
        &mut self,
        observed: &[ObservedWindow],
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        self.sync_previews(observed);
        let mut actions = Vec::new();
        for window in observed {
            let matching = self.items_for_window(window);
            let item = if let Some(item) = self.preferred_matching_item(&matching) {
                for duplicate in matching
                    .iter()
                    .copied()
                    .filter(|candidate| *candidate != item)
                {
                    actions.extend(self.apply(ShellEvent::WindowClosed(duplicate))?);
                    if self.is_dock_item(duplicate) {
                        actions.extend(self.apply(ShellEvent::Unpin(duplicate))?);
                    }
                    self.launch_targets.remove(&duplicate);
                }
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
        let Some(app) = self.previews.get(&window).map(PreviewWindowState::app) else {
            return Ok(Vec::new());
        };
        let queued = match action {
            PreviewAction::Focus => PreviewQueuedAction::Focus {
                window,
                app: app.clone(),
            },
            PreviewAction::Close => PreviewQueuedAction::Close {
                window,
                app: app.clone(),
            },
        };
        Ok(vec![QueuedDockAction::Preview(queued)])
    }

    fn sync_previews(&mut self, observed: &[ObservedWindow]) {
        self.previews.clear();
        for window in observed {
            if let Some(preview) = window.preview() {
                self.previews.insert(
                    window.window(),
                    PreviewWindowState::from_observed(window, preview),
                );
            }
        }
    }

    #[must_use]
    pub fn has_preview_windows_for_item(&self, item: DockItemId) -> bool {
        let Some(app) = self
            .state
            .dock_items()
            .iter()
            .find(|entry| entry.id() == item)
            .map(DockItem::app)
        else {
            return false;
        };
        self.previews.values().any(|window| window.matches_app(app))
    }

    #[must_use]
    pub fn preview_windows_for_item(&self, item: DockItemId) -> Vec<&PreviewWindowState> {
        let Some(app) = self
            .state
            .dock_items()
            .iter()
            .find(|entry| entry.id() == item)
            .map(DockItem::app)
        else {
            return Vec::new();
        };
        let mut windows = self
            .previews
            .values()
            .filter(|window| window.matches_app(app))
            .collect::<Vec<_>>();
        windows.sort_by(|left, right| {
            right
                .foreground()
                .cmp(&left.foreground())
                .then_with(|| left.title().cmp(right.title()))
                .then_with(|| left.window().value().cmp(&right.window().value()))
        });
        windows
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

    fn items_for_window(&self, window: &ObservedWindow) -> Vec<DockItemId> {
        self.state
            .dock_items()
            .iter()
            .filter(|item| window.matches_app(item.app()))
            .map(DockItem::id)
            .collect()
    }

    fn preferred_matching_item(&self, matching: &[DockItemId]) -> Option<DockItemId> {
        matching
            .iter()
            .copied()
            .find(|id| {
                self.dock_item(*id)
                    .is_some_and(|item| item.pin() == shell_core::PinState::Pinned)
            })
            .or_else(|| matching.first().copied())
    }

    fn stable_item_for_app(&self, app: &AppId) -> DockItemId {
        let mut id = stable_item_id(app);
        while self.state.dock_items().iter().any(|item| item.id() == id)
            || self.separator_visual_id_exists(id.value())
        {
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
