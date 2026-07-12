#![deny(unsafe_code)]

use std::error::Error;
use std::fmt;

use shell_core::{Popover, ShellEvent, ShellState, TopbarModuleKind, TransitionError, reduce};
use shell_renderer::{
    DipRect, TopbarDensity, TopbarModuleStatus, TopbarModuleVisual, TopbarScene,
    layout_topbar_scene,
};

use crate::{
    PollBudget, QueuedTopbarAction, TopbarPointerPhase, TopbarPointerSample, TopbarSnapshot,
};

pub struct TopbarController {
    state: ShellState,
    density: TopbarDensity,
    surface: DipRect,
    snapshot: TopbarSnapshot,
    pressed_intent: Option<Popover>,
    visual_generation: u64,
    resource_generation: u64,
}

impl TopbarController {
    pub fn new(state: ShellState, density: TopbarDensity) -> Result<Self, TopbarControllerError> {
        state.validate()?;
        Ok(Self {
            state,
            density,
            surface: DipRect::new(0.0, 0.0, 1.0, 1.0),
            snapshot: TopbarSnapshot::default(),
            pressed_intent: None,
            visual_generation: 0,
            resource_generation: 0,
        })
    }

    #[must_use]
    pub const fn state(&self) -> &ShellState {
        &self.state
    }

    #[must_use]
    pub const fn snapshot(&self) -> &TopbarSnapshot {
        &self.snapshot
    }

    #[must_use]
    pub const fn visual_generation(&self) -> u64 {
        self.visual_generation
    }

    #[must_use]
    pub const fn resource_generation(&self) -> u64 {
        self.resource_generation
    }

    pub const fn update_surface(&mut self, surface: DipRect) {
        self.surface = surface;
    }

    pub const fn update_density(&mut self, density: TopbarDensity) {
        self.density = density;
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
    }

    pub fn handle_pointer(
        &mut self,
        sample: TopbarPointerSample,
    ) -> Result<Vec<QueuedTopbarAction>, TopbarControllerError> {
        match sample.phase() {
            TopbarPointerPhase::Pressed => {
                self.pressed_intent = self.hit_test(sample);
                Ok(Vec::new())
            }
            TopbarPointerPhase::Released => {
                let intent = self.hit_test(sample);
                if intent.is_some() && intent == self.pressed_intent {
                    self.pressed_intent = None;
                    self.open_intent(intent)
                } else {
                    self.pressed_intent = None;
                    Ok(Vec::new())
                }
            }
            TopbarPointerPhase::Moved | TopbarPointerPhase::Exited => Ok(Vec::new()),
        }
    }

    #[must_use]
    pub fn refresh_status(&mut self, budget: PollBudget, now_ms: u64) -> QueuedTopbarAction {
        if !budget.permits(now_ms) {
            return QueuedTopbarAction::PollDeferred;
        }
        QueuedTopbarAction::RedrawTopbar
    }

    fn hit_test(&self, sample: TopbarPointerSample) -> Option<Popover> {
        layout_topbar_scene(&self.scene(), self.surface).hit_test(sample.point())
    }

    fn open_intent(
        &mut self,
        intent: Option<Popover>,
    ) -> Result<Vec<QueuedTopbarAction>, TopbarControllerError> {
        let Some(popover) = intent else {
            return Ok(Vec::new());
        };
        let transition = reduce(&self.state, ShellEvent::OpenPopover(popover))?;
        self.state = transition.state;
        Ok(vec![QueuedTopbarAction::OpenPopover(popover)])
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
                "Menu",
                "Minha UI",
                TopbarModuleStatus::Neutral,
                Popover::SystemMenu,
            ),
            TopbarModuleKind::Clock => visual(
                kind,
                "Time",
                &self.snapshot.clock,
                TopbarModuleStatus::Neutral,
                Popover::Calendar,
            ),
            TopbarModuleKind::Network => visual(
                kind,
                "Net",
                &network_text(&self.snapshot.network),
                network_status(&self.snapshot.network),
                Popover::Network,
            ),
            TopbarModuleKind::Volume => visual(
                kind,
                "Vol",
                &format!("{}%", self.snapshot.volume_percent),
                TopbarModuleStatus::Neutral,
                Popover::Volume,
            ),
            TopbarModuleKind::Power => visual(
                kind,
                "Pwr",
                &self.snapshot.battery.label(),
                power_status(self.snapshot.battery),
                Popover::Power,
            ),
            TopbarModuleKind::Notifications => visual(
                kind,
                "Bell",
                notification_text(self.snapshot.notifications),
                TopbarModuleStatus::Neutral,
                Popover::Notifications,
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

fn notification_text(count: u16) -> &'static str {
    if count == 0 { "Clear" } else { "Attention" }
}

fn visual(
    kind: TopbarModuleKind,
    icon: &str,
    text: &str,
    status: TopbarModuleStatus,
    intent: Popover,
) -> TopbarModuleVisual {
    TopbarModuleVisual::new(kind, icon, text, status, Some(intent))
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
