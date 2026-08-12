#![deny(unsafe_code)]

use std::error::Error;
use std::fmt;

use shell_core::{
    Popover, ShellEvent, ShellState, TopbarIntent, TopbarModuleKind, TransitionError, reduce,
};
use shell_renderer::{
    DipRect, TopbarDensity, TopbarModuleStatus, TopbarModuleVisual, TopbarScene,
    layout_topbar_scene,
};

use crate::{
    QueuedTopbarAction, TopbarKey, TopbarOverlayAnchor, TopbarPointerPhase, TopbarPointerSample,
    TopbarSnapshot,
};

pub struct TopbarController {
    state: ShellState,
    density: TopbarDensity,
    text_scale: f32,
    surface: DipRect,
    snapshot: TopbarSnapshot,
    pressed_target: Option<(TopbarOverlayAnchor, TopbarIntent)>,
    focused_module: Option<TopbarModuleKind>,
    hovered_module: Option<TopbarModuleKind>,
    active_module: Option<TopbarModuleKind>,
    visual_generation: u64,
    #[cfg(test)]
    resource_generation: u64,
}

impl TopbarController {
    pub fn new(state: ShellState, density: TopbarDensity) -> Result<Self, TopbarControllerError> {
        state.validate()?;
        Ok(Self {
            state,
            density,
            text_scale: 1.0,
            surface: DipRect::new(0.0, 0.0, 1.0, 1.0),
            snapshot: TopbarSnapshot::default(),
            pressed_target: None,
            focused_module: None,
            hovered_module: None,
            active_module: None,
            visual_generation: 0,
            #[cfg(test)]
            resource_generation: 0,
        })
    }

    #[must_use]
    #[expect(dead_code, reason = "retained for focused controller diagnostics")]
    pub const fn state(&self) -> &ShellState {
        &self.state
    }

    #[must_use]
    pub(crate) const fn snapshot(&self) -> &TopbarSnapshot {
        &self.snapshot
    }

    pub(crate) fn module_bounds(&self, kind: TopbarModuleKind) -> Option<DipRect> {
        layout_topbar_scene(&self.scene(), self.surface).module_bounds(kind)
    }

    #[must_use]
    pub const fn visual_generation(&self) -> u64 {
        self.visual_generation
    }

    #[must_use]
    #[cfg(test)]
    pub const fn resource_generation(&self) -> u64 {
        self.resource_generation
    }

    pub const fn update_surface(&mut self, surface: DipRect) {
        self.surface = surface;
    }

    pub const fn update_density(&mut self, density: TopbarDensity) {
        self.density = density;
    }

    pub fn update_text_scale(&mut self, text_scale: f32) {
        self.text_scale = text_scale.clamp(1.0, 2.5);
    }

    pub fn update_snapshot(&mut self, snapshot: TopbarSnapshot) {
        if self.snapshot != snapshot {
            self.snapshot = snapshot;
            self.visual_generation += 1;
        }
    }

    #[must_use]
    pub fn scene(&self) -> TopbarScene {
        TopbarScene::new(self.density, self.visible_modules())
            .with_focused_module(self.focused_module)
            .with_hovered_module(self.hovered_module)
            .with_pressed_module(self.pressed_module())
            .with_active_module(self.active_module)
            .with_text_scale(self.text_scale)
    }

    pub(crate) fn set_active_module(&mut self, active_module: Option<TopbarModuleKind>) {
        if self.active_module != active_module {
            self.active_module = active_module;
            self.visual_generation = self.visual_generation.wrapping_add(1);
        }
    }

    pub fn handle_pointer(
        &mut self,
        sample: TopbarPointerSample,
    ) -> Result<Vec<QueuedTopbarAction>, TopbarControllerError> {
        match sample.phase() {
            TopbarPointerPhase::Pressed => {
                self.pressed_target = self.hit_test(sample);
                Ok(vec![QueuedTopbarAction::RedrawTopbar])
            }
            TopbarPointerPhase::Released => {
                let target = self.hit_test(sample);
                let opens_target = target.is_some() && target == self.pressed_target;
                self.pressed_target = None;
                let mut actions = if opens_target {
                    self.open_target(target)?
                } else {
                    Vec::new()
                };
                actions.push(QueuedTopbarAction::RedrawTopbar);
                Ok(actions)
            }
            TopbarPointerPhase::Moved => {
                let hovered = self.hit_test(sample).and_then(|(anchor, _)| match anchor {
                    TopbarOverlayAnchor::Module(kind) => Some(kind),
                    TopbarOverlayAnchor::Overflow => None,
                });
                Ok(self.set_hovered_module(hovered))
            }
            TopbarPointerPhase::Exited => Ok(self.set_hovered_module(None)),
        }
    }

    fn set_hovered_module(
        &mut self,
        hovered_module: Option<TopbarModuleKind>,
    ) -> Vec<QueuedTopbarAction> {
        if self.hovered_module == hovered_module {
            return Vec::new();
        }
        self.hovered_module = hovered_module;
        self.visual_generation = self.visual_generation.wrapping_add(1);
        vec![QueuedTopbarAction::RedrawTopbar]
    }

    pub fn handle_key(
        &mut self,
        key: TopbarKey,
    ) -> Result<Vec<QueuedTopbarAction>, TopbarControllerError> {
        match key {
            TopbarKey::Next => {
                self.focus_delta(1);
                Ok(vec![QueuedTopbarAction::RedrawTopbar])
            }
            TopbarKey::Previous => {
                self.focus_delta(-1);
                Ok(vec![QueuedTopbarAction::RedrawTopbar])
            }
            TopbarKey::Activate => self.open_focused(),
            TopbarKey::Escape => {
                self.set_focused_module(None);
                Ok(vec![QueuedTopbarAction::RedrawTopbar])
            }
        }
    }

    fn hit_test(&self, sample: TopbarPointerSample) -> Option<(TopbarOverlayAnchor, TopbarIntent)> {
        let layout = layout_topbar_scene(&self.scene(), self.surface);
        layout
            .item_at(sample.point())
            .and_then(|item| {
                item.intent()
                    .map(|intent| (TopbarOverlayAnchor::Module(item.kind()), intent))
            })
            .or_else(|| {
                layout
                    .hit_test(sample.point())
                    .map(|intent| (TopbarOverlayAnchor::Overflow, intent))
            })
    }

    fn open_target(
        &mut self,
        target: Option<(TopbarOverlayAnchor, TopbarIntent)>,
    ) -> Result<Vec<QueuedTopbarAction>, TopbarControllerError> {
        let Some(target) = target else {
            return Ok(Vec::new());
        };
        self.open_intent(target.1, target.0)
    }

    fn open_focused(&mut self) -> Result<Vec<QueuedTopbarAction>, TopbarControllerError> {
        let Some(focused) = self.focused_module else {
            return Ok(Vec::new());
        };
        let Some(intent) = self
            .visible_modules()
            .into_iter()
            .find(|module| module.kind() == focused)
            .and_then(|module| module.intent())
        else {
            return Ok(Vec::new());
        };
        self.open_intent(intent, TopbarOverlayAnchor::Module(focused))
    }

    fn open_intent(
        &mut self,
        intent: TopbarIntent,
        anchor: TopbarOverlayAnchor,
    ) -> Result<Vec<QueuedTopbarAction>, TopbarControllerError> {
        match intent {
            TopbarIntent::Popover(popover) => {
                let transition = reduce(&self.state, ShellEvent::OpenPopover(popover))?;
                self.state = transition.state;
                Ok(vec![QueuedTopbarAction::OpenPopover { popover, anchor }])
            }
            TopbarIntent::OpenSearch => Ok(vec![QueuedTopbarAction::OpenSearch]),
        }
    }

    fn focus_delta(&mut self, delta: isize) {
        let modules = self
            .visible_modules()
            .into_iter()
            .filter(|module| module.intent().is_some())
            .map(|module| module.kind())
            .collect::<Vec<_>>();
        if modules.is_empty() {
            self.set_focused_module(None);
            return;
        }
        let Some(current) = self.focused_module else {
            if delta < 0 {
                self.set_focused_module(modules.last().copied());
            } else {
                self.set_focused_module(Some(modules[0]));
            }
            return;
        };
        let position = modules
            .iter()
            .position(|module| *module == current)
            .unwrap_or(0);
        let next = position.saturating_add_signed(delta).min(modules.len() - 1);
        self.set_focused_module(Some(modules[next]));
    }

    fn set_focused_module(&mut self, focused_module: Option<TopbarModuleKind>) {
        if self.focused_module != focused_module {
            self.focused_module = focused_module;
            self.visual_generation = self.visual_generation.wrapping_add(1);
        }
    }

    fn pressed_module(&self) -> Option<TopbarModuleKind> {
        self.pressed_target
            .as_ref()
            .and_then(|(anchor, _)| match anchor {
                TopbarOverlayAnchor::Module(kind) => Some(*kind),
                TopbarOverlayAnchor::Overflow => None,
            })
    }

    fn visible_modules(&self) -> Vec<TopbarModuleVisual> {
        self.state
            .topbar_modules()
            .iter()
            .filter(|module| module.visible())
            .map(|module| self.visual_for(module.kind()))
            .collect()
    }

    fn visual_for(&self, kind: TopbarModuleKind) -> TopbarModuleVisual {
        match kind {
            TopbarModuleKind::SystemMenu => visual(
                kind,
                "\u{E700}",
                "Minha UI",
                TopbarModuleStatus::Neutral,
                Some(TopbarIntent::Popover(Popover::SystemMenu)),
            ),
            TopbarModuleKind::AppIdentity => visual(
                kind,
                "\u{E77B}",
                self.snapshot.app_label(),
                TopbarModuleStatus::Neutral,
                None,
            ),
            TopbarModuleKind::Search => visual(
                kind,
                "\u{E721}",
                "Search",
                TopbarModuleStatus::Neutral,
                Some(TopbarIntent::OpenSearch),
            ),
            TopbarModuleKind::Clock => visual(
                kind,
                "\u{E823}",
                &self.snapshot.clock,
                TopbarModuleStatus::Neutral,
                Some(TopbarIntent::Popover(Popover::Calendar)),
            ),
            TopbarModuleKind::Network => visual(
                kind,
                "\u{E701}",
                &network_text(&self.snapshot.network),
                network_status(&self.snapshot.network),
                Some(TopbarIntent::Popover(Popover::Network)),
            ),
            TopbarModuleKind::Volume => visual(
                kind,
                "\u{E767}",
                &format!("{}%", self.snapshot.volume_percent),
                TopbarModuleStatus::Neutral,
                Some(TopbarIntent::Popover(Popover::Volume)),
            ),
            TopbarModuleKind::Power => visual(
                kind,
                "\u{E83F}",
                &self.snapshot.battery.label(),
                power_status(self.snapshot.battery),
                Some(TopbarIntent::Popover(Popover::Power)),
            ),
            TopbarModuleKind::Notifications => visual(
                kind,
                "\u{E713}",
                "Controls",
                TopbarModuleStatus::Neutral,
                Some(TopbarIntent::Popover(Popover::QuickSettings)),
            ),
            TopbarModuleKind::BackgroundApps => visual(
                kind,
                "\u{E74A}",
                "Apps",
                TopbarModuleStatus::Neutral,
                Some(TopbarIntent::Popover(Popover::BackgroundApps)),
            ),
        }
    }
}

fn network_text(network: &crate::NetworkSnapshot) -> String {
    if network.is_online() {
        network.throughput_label().text()
    } else {
        network.label().to_owned()
    }
}

fn network_status(network: &crate::NetworkSnapshot) -> TopbarModuleStatus {
    if network.is_online() {
        TopbarModuleStatus::Good
    } else {
        TopbarModuleStatus::Warning
    }
}

const fn power_status(power: crate::PowerSnapshot) -> TopbarModuleStatus {
    match power.percent() {
        Some(percent) if percent <= 20 && !power.plugged_in() => TopbarModuleStatus::Warning,
        Some(_) | None => TopbarModuleStatus::Neutral,
    }
}

fn visual(
    kind: TopbarModuleKind,
    icon: &str,
    text: &str,
    status: TopbarModuleStatus,
    intent: Option<TopbarIntent>,
) -> TopbarModuleVisual {
    TopbarModuleVisual::new(kind, icon, text, status, intent)
}

#[derive(Debug)]
pub enum TopbarControllerError {
    State(shell_core::StateError),
    Transition(TransitionError),
}

impl fmt::Display for TopbarControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::State(error) => write!(formatter, "{error}"),
            Self::Transition(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for TopbarControllerError {}

impl From<shell_core::StateError> for TopbarControllerError {
    fn from(value: shell_core::StateError) -> Self {
        Self::State(value)
    }
}

impl From<TransitionError> for TopbarControllerError {
    fn from(value: TransitionError) -> Self {
        Self::Transition(value)
    }
}
