#![deny(unsafe_code)]

use std::path::Path;

use shell_core::{AppId, DockItem, DockItemId, Effect, ShellEvent, ShellState, reduce};
use shell_renderer::{
    DipPoint, DipRect, DockItemVisual, DockLayout, DockScene, RunningIndicator, layout_dock_scene,
};

use crate::{
    ContextMenuCommand, DockAnimator, DockControllerError, DockPointerPhase, DockPointerSample,
    DockRuntimeConfig, QueuedDockAction,
};

pub struct DockController {
    state: ShellState,
    config: DockRuntimeConfig,
    surface: DipRect,
    pressed_item: Option<DockItemId>,
    animator: DockAnimator,
}

impl DockController {
    pub fn new(state: ShellState, config: DockRuntimeConfig) -> Result<Self, DockControllerError> {
        let mut controller = Self {
            state,
            config,
            surface: DipRect::new(0.0, 0.0, 1.0, 1.0),
            pressed_item: None,
            animator: DockAnimator::new(),
        };
        if config.autohide() {
            controller.apply(ShellEvent::EnableAutohide(true))?;
        }
        Ok(controller)
    }

    #[must_use]
    pub const fn state(&self) -> &ShellState {
        &self.state
    }

    #[must_use]
    pub const fn config(&self) -> DockRuntimeConfig {
        self.config
    }

    #[must_use]
    pub const fn animator(&self) -> DockAnimator {
        self.animator
    }

    pub const fn update_surface(&mut self, surface: DipRect) {
        self.surface = surface;
    }

    pub fn handle_pointer(
        &mut self,
        sample: DockPointerSample,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        match sample.phase {
            DockPointerPhase::Pressed => {
                self.pressed_item = self.layout().hit_test(sample.point).map(DockItemId::new);
                Ok(Vec::new())
            }
            DockPointerPhase::Released => self.release(sample.point),
            DockPointerPhase::Dragged => self.drag(sample.point),
            DockPointerPhase::Exited => {
                if self.config.autohide() {
                    self.apply(ShellEvent::HideDock)
                } else {
                    Ok(Vec::new())
                }
            }
            DockPointerPhase::Moved => self.reveal_if_needed(sample.point),
        }
    }

    pub fn handle_context_menu(
        &mut self,
        point: DipPoint,
        command: ContextMenuCommand,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let id = self.layout().hit_test(point).map(DockItemId::new);
        match command {
            ContextMenuCommand::Open => Ok(id.map_or_else(Vec::new, |item| {
                self.window_for_item(item).map_or_else(Vec::new, |window| {
                    vec![QueuedDockAction::Effect(Effect::FocusWindow(window))]
                })
            })),
            ContextMenuCommand::Unpin => {
                id.map_or(Ok(Vec::new()), |item| self.apply(ShellEvent::Unpin(item)))
            }
            ContextMenuCommand::Pin => Ok(Vec::new()),
            ContextMenuCommand::Quit => Ok(vec![QueuedDockAction::Quit]),
        }
    }

    pub fn handle_drop(
        &mut self,
        _point: DipPoint,
        path: &str,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let file_name = Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(DockControllerError::MissingFileName)?;
        let app = AppId::parse(file_name)?;
        let id = self.next_item_id();
        self.apply(ShellEvent::Pin(DockItem::pinned(id, app)))
    }

    #[must_use]
    pub fn scene(&self) -> DockScene {
        DockScene::new(self.config.layout(), self.visual_items())
    }

    fn release(&mut self, point: DipPoint) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let released = self.layout().hit_test(point).map(DockItemId::new);
        let pressed = self.pressed_item.take();
        if let Some(released_item) = released
            && pressed == Some(released_item)
        {
            self.apply(ShellEvent::ActivateDockItem(released_item))
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

    fn apply(&mut self, event: ShellEvent) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let transition = reduce(&self.state, event)?;
        self.state = transition.state;
        Ok(transition
            .effects
            .into_iter()
            .map(QueuedDockAction::Effect)
            .collect())
    }

    fn layout(&self) -> DockLayout {
        layout_dock_scene(&self.scene(), self.surface)
    }

    fn before_item(&self, point: DipPoint, moving: DockItemId) -> Option<DockItemId> {
        self.layout()
            .items()
            .iter()
            .filter(|item| item.id() != moving.value())
            .find(|item| point.x < item.bounds().x + item.bounds().width / 2.0)
            .map(|item| DockItemId::new(item.id()))
    }

    fn visual_items(&self) -> Vec<DockItemVisual> {
        self.state
            .dock_items()
            .iter()
            .map(|item| DockItemVisual::app(item.id().value(), visual_label(item), indicator(item)))
            .collect()
    }

    fn next_item_id(&self) -> DockItemId {
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
}

fn indicator(item: &DockItem) -> RunningIndicator {
    match item.running() {
        shell_core::RunningState::Stopped => RunningIndicator::Stopped,
        shell_core::RunningState::Running {
            focused: true,
            minimized: false,
            ..
        } => RunningIndicator::Focused,
        shell_core::RunningState::Running {
            minimized: true, ..
        } => RunningIndicator::Minimized,
        shell_core::RunningState::Running { .. } => RunningIndicator::Running,
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
