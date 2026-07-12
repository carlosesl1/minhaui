#![deny(unsafe_code)]

use shell_core::{DockItem, DockItemId, Effect, ShellEvent, reduce};
use shell_renderer::{DipPoint, DockLayout, layout_dock_scene};

use crate::dock_launch::dropped_launch_target;
use crate::{
    ContextMenuCommand, DockController, DockControllerError, DockPointerPhase, DockPointerSample,
    PreviewAction, QueuedDockAction,
};

impl DockController {
    pub fn handle_pointer(
        &mut self,
        sample: DockPointerSample,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        match sample.phase {
            DockPointerPhase::Pressed => {
                self.pressed_item = self.hit_app(sample.point);
                Ok(Vec::new())
            }
            DockPointerPhase::Released => self.release(sample.point),
            DockPointerPhase::Dragged => self.drag(sample.point),
            DockPointerPhase::Exited => {
                self.set_hovered_item(None);
                if self.config.autohide() {
                    self.apply(ShellEvent::HideDock)
                } else {
                    Ok(Vec::new())
                }
            }
            DockPointerPhase::Moved => {
                self.set_hovered_item(self.hit_app(sample.point));
                self.reveal_if_needed(sample.point)
            }
        }
    }

    pub fn handle_context_menu(
        &mut self,
        point: DipPoint,
        command: ContextMenuCommand,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let id = self.hit_app(point);
        match command {
            ContextMenuCommand::Open => Ok(id.map_or_else(Vec::new, |item| {
                self.window_for_item(item).map_or_else(Vec::new, |window| {
                    vec![QueuedDockAction::Effect(Effect::FocusWindow(window))]
                })
            })),
            ContextMenuCommand::Unpin => {
                id.map_or(Ok(Vec::new()), |item| self.apply(ShellEvent::Unpin(item)))
            }
            ContextMenuCommand::Pin => id.map_or(Ok(Vec::new()), |item| {
                self.dock_item(item)
                    .cloned()
                    .map_or(Ok(Vec::new()), |entry| self.apply(ShellEvent::Pin(entry)))
            }),
            ContextMenuCommand::PreviewFocus => {
                self.handle_preview_action_at(point, PreviewAction::Focus)
            }
            ContextMenuCommand::PreviewClose => {
                self.handle_preview_action_at(point, PreviewAction::Close)
            }
            ContextMenuCommand::Quit => Ok(vec![QueuedDockAction::Quit]),
        }
    }

    pub fn handle_drop(
        &mut self,
        _point: DipPoint,
        path: &str,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let (app, launch_target) = dropped_launch_target(path)?;
        let id = self.next_item_id();
        let actions = self.apply(ShellEvent::Pin(DockItem::pinned(id, app)))?;
        self.launch_targets.insert(id, launch_target);
        Ok(actions)
    }

    fn release(&mut self, point: DipPoint) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let released = self.hit_app(point);
        let pressed = self.pressed_item.take();
        if let Some(released_item) = released
            && pressed == Some(released_item)
        {
            self.activate(released_item)
        } else {
            Ok(Vec::new())
        }
    }

    fn drag(&mut self, point: DipPoint) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let Some(item) = self.pressed_item else {
            return Ok(Vec::new());
        };
        let before = self.before_item(point, item);
        self.pressed_item = Some(item);
        self.apply(ShellEvent::ReorderDockItem { item, before })
    }

    fn reveal_if_needed(
        &mut self,
        point: DipPoint,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let threshold = self.surface.y + self.surface.height - self.config.reveal_zone_height();
        if self.config.autohide() && point.y >= threshold {
            self.apply(ShellEvent::RevealDock)
        } else {
            Ok(Vec::new())
        }
    }

    pub(crate) fn layout(&self) -> DockLayout {
        layout_dock_scene(&self.scene(), self.surface)
    }

    fn before_item(&self, point: DipPoint, moving: DockItemId) -> Option<DockItemId> {
        self.layout()
            .items()
            .iter()
            .filter(|item| item.id() != moving.value())
            .filter(|item| self.is_dock_item(DockItemId::new(item.id())))
            .find(|item| point.x < item.bounds().x + item.bounds().width / 2.0)
            .map(|item| DockItemId::new(item.id()))
    }

    pub(crate) fn next_item_id(&self) -> DockItemId {
        let value = self
            .state
            .dock_items()
            .iter()
            .map(|item| item.id().value())
            .max()
            .map_or(1, |value| value + 1);
        DockItemId::new(value)
    }

    fn window_for_item(&self, id: DockItemId) -> Option<shell_core::WindowId> {
        self.state
            .dock_items()
            .iter()
            .find(|item| item.id() == id)
            .and_then(|item| match item.running() {
                shell_core::RunningState::Running { window, .. } => Some(*window),
                shell_core::RunningState::Stopped => None,
            })
    }

    fn handle_preview_action_at(
        &self,
        point: DipPoint,
        action: PreviewAction,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let Some(item) = self.hit_app(point) else {
            return Ok(Vec::new());
        };
        self.window_for_item(item).map_or(Ok(Vec::new()), |window| {
            self.handle_preview_action(window, action)
        })
    }

    pub(crate) fn dock_item(&self, id: DockItemId) -> Option<&DockItem> {
        self.state.dock_items().iter().find(|item| item.id() == id)
    }

    pub(crate) fn is_dock_item(&self, id: DockItemId) -> bool {
        self.dock_item(id).is_some()
    }

    fn hit_app(&self, point: DipPoint) -> Option<DockItemId> {
        self.layout()
            .hit_test(point)
            .map(DockItemId::new)
            .filter(|id| self.is_dock_item(*id))
    }

    fn set_hovered_item(&mut self, hovered_item: Option<DockItemId>) {
        if self.hovered_item != hovered_item {
            self.hovered_item = hovered_item;
            self.visual_generation = self.visual_generation.wrapping_add(1);
        }
    }

    fn activate(&mut self, item: DockItemId) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let transition = reduce(&self.state, ShellEvent::ActivateDockItem(item))?;
        self.state = transition.state;
        Ok(transition
            .effects
            .into_iter()
            .map(|effect| self.map_effect(effect, Some(item)))
            .collect())
    }

    fn map_effect(&self, effect: Effect, launch_item: Option<DockItemId>) -> QueuedDockAction {
        match effect {
            Effect::Launch(app) => {
                let target = launch_item
                    .and_then(|item| self.launch_targets.get(&item).cloned())
                    .unwrap_or_else(|| app.as_str().to_owned());
                QueuedDockAction::Launch(target)
            }
            effect => QueuedDockAction::Effect(effect),
        }
    }
}
