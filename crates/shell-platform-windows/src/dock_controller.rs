#![deny(unsafe_code)]

use std::collections::HashMap;

use shell_core::{DockItemId, RunningState, ShellEvent, ShellState, WindowId, reduce};
use shell_renderer::{DipRect, DockScene, WindowPreviewVisual};

use crate::dock_launch::initial_launch_targets;
use crate::dock_visuals::visual_items;
use crate::{DockAnimator, DockControllerError, DockRuntimeConfig};

pub struct DockController {
    pub(crate) state: ShellState,
    pub(crate) config: DockRuntimeConfig,
    pub(crate) surface: DipRect,
    pub(crate) pressed_item: Option<DockItemId>,
    pub(crate) hovered_item: Option<DockItemId>,
    pub(crate) focused_item: Option<DockItemId>,
    pub(crate) visual_generation: u64,
    pub(crate) launch_targets: HashMap<DockItemId, String>,
    pub(crate) previews: HashMap<WindowId, crate::PreviewCapture>,
    pub(crate) animator: DockAnimator,
}

impl DockController {
    pub fn new(state: ShellState, config: DockRuntimeConfig) -> Result<Self, DockControllerError> {
        let launch_targets = initial_launch_targets(&state);
        let mut controller = Self {
            state,
            config,
            surface: DipRect::new(0.0, 0.0, 1.0, 1.0),
            pressed_item: None,
            hovered_item: None,
            focused_item: None,
            visual_generation: 0,
            launch_targets,
            previews: HashMap::new(),
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

    #[must_use]
    pub const fn visual_generation(&self) -> u64 {
        self.visual_generation
    }

    #[must_use]
    pub fn launch_target_for_item(&self, item: DockItemId) -> Option<&str> {
        self.launch_targets.get(&item).map(String::as_str)
    }

    pub const fn update_surface(&mut self, surface: DipRect) {
        self.surface = surface;
    }

    #[must_use]
    pub fn scene(&self) -> DockScene {
        DockScene::new(self.config.layout(), visual_items(&self.state))
            .with_hovered_item(self.hovered_item.map(DockItemId::value))
            .with_focused_item(self.focused_item.map(DockItemId::value))
            .with_window_previews(self.hovered_window_preview())
    }

    pub(crate) fn qa_trace_line(&self) -> String {
        let items = self
            .state
            .dock_items()
            .iter()
            .enumerate()
            .map(|(index, item)| {
                format!(
                    "{}:{}:{}:{}:{}",
                    index,
                    item.id().value(),
                    item.app().as_str(),
                    format!("{:?}", item.pin()).to_ascii_lowercase(),
                    running_state(item.running()),
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        format!(
            "DOCK_STATE hover={:?} items={}",
            self.hovered_item.map(DockItemId::value),
            items
        )
    }

    pub(crate) fn apply(
        &mut self,
        event: ShellEvent,
    ) -> Result<Vec<crate::QueuedDockAction>, DockControllerError> {
        let transition = reduce(&self.state, event)?;
        self.state = transition.state;
        Ok(transition
            .effects
            .into_iter()
            .map(crate::QueuedDockAction::Effect)
            .collect())
    }
}

impl DockController {
    fn hovered_window_preview(&self) -> Vec<WindowPreviewVisual> {
        let Some(hovered) = self.hovered_item else {
            return Vec::new();
        };
        let Some(window) = self
            .state
            .dock_items()
            .iter()
            .find(|item| item.id() == hovered)
            .and_then(|item| match item.running() {
                RunningState::Running { window, .. } => Some(*window),
                RunningState::Stopped => None,
            })
        else {
            return Vec::new();
        };
        match self.previews.get(&window).copied() {
            Some(crate::PreviewCapture::DwmThumbnail) => {
                vec![WindowPreviewVisual::available(window, hovered)]
            }
            Some(crate::PreviewCapture::Restricted(reason)) => {
                vec![WindowPreviewVisual::restricted(window, hovered, reason)]
            }
            None => Vec::new(),
        }
    }
}

fn running_state(running: &shell_core::RunningState) -> String {
    match running {
        shell_core::RunningState::Stopped => "stopped".to_owned(),
        shell_core::RunningState::Running {
            window,
            focused,
            minimized,
        } => format!(
            "running:{:?}:focused={}:minimized={}",
            window, focused, minimized
        ),
    }
}
