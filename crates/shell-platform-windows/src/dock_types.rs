#![deny(unsafe_code)]

use std::error::Error;
use std::fmt;

use shell_core::{AppIdError, Effect, TransitionError};
use shell_renderer::{DipPoint, DockAlignment, DockLayoutConfig};

use crate::PreviewQueuedAction;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockRuntimeConfig {
    layout: DockLayoutConfig,
    autohide: bool,
    reveal_zone_height: f32,
    animation_ms: u16,
}

impl DockRuntimeConfig {
    const INDICATOR_AND_BOTTOM_RESERVE_DIP: f32 = 11.0;

    #[must_use]
    pub const fn new(alignment: DockAlignment) -> Self {
        Self {
            layout: DockLayoutConfig::new(alignment),
            autohide: false,
            reveal_zone_height: 8.0,
            animation_ms: 140,
        }
    }

    #[must_use]
    #[cfg(test)]
    pub const fn alignment(self) -> DockAlignment {
        self.layout.alignment()
    }

    #[must_use]
    pub const fn layout(self) -> DockLayoutConfig {
        self.layout
    }

    #[must_use]
    pub const fn autohide(self) -> bool {
        self.autohide
    }

    #[must_use]
    pub const fn reveal_zone_height(self) -> f32 {
        self.reveal_zone_height
    }

    /// User-selected duration used to scale Dock spring and reorder motion.
    /// Zero disables nonessential Dock animation while preserving interaction.
    #[must_use]
    pub const fn animation_ms(self) -> u16 {
        self.animation_ms
    }

    #[must_use]
    pub const fn with_item_size(mut self, value: f32) -> Self {
        self.layout = self.layout.with_item_size(value);
        self
    }

    #[must_use]
    pub const fn with_spacing(mut self, value: f32) -> Self {
        self.layout = self.layout.with_spacing(value);
        self
    }

    #[must_use]
    pub const fn with_padding(mut self, value: f32) -> Self {
        self.layout = self.layout.with_padding(value);
        self
    }

    #[must_use]
    pub const fn with_magnified_item_size(mut self, value: f32) -> Self {
        self.layout = self.layout.with_magnified_item_size(value);
        self
    }

    #[must_use]
    pub const fn dock_height_dip(self) -> f32 {
        self.layout.padding() + self.layout.item_size() + Self::INDICATOR_AND_BOTTOM_RESERVE_DIP
    }

    #[must_use]
    pub const fn with_autohide(mut self, enabled: bool) -> Self {
        self.autohide = enabled;
        self
    }

    #[must_use]
    pub const fn with_animation_ms(mut self, value: u16) -> Self {
        self.animation_ms = value;
        self
    }

    #[must_use]
    #[cfg(test)]
    pub const fn with_reveal_zone_height(mut self, value: f32) -> Self {
        self.reveal_zone_height = value;
        self
    }
}

impl Default for DockRuntimeConfig {
    fn default() -> Self {
        Self::new(DockAlignment::Center)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DockPointerPhase {
    Moved,
    Pressed,
    Released,
    Dragged,
    Cancelled,
    Exited,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockPointerSample {
    pub(crate) phase: DockPointerPhase,
    pub(crate) point: DipPoint,
}

impl DockPointerSample {
    #[must_use]
    pub const fn new(phase: DockPointerPhase, point: DipPoint) -> Self {
        Self { phase, point }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DockKey {
    Next,
    Previous,
    Preview,
    Activate,
    Escape,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextMenuCommand {
    Open,
    Unpin,
    Pin,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "reserved for the legacy native preview menu")
    )]
    PreviewFocus,
    PreviewClose,
    AddSeparator,
    RemoveSeparator,
    MoveLeft,
    MoveRight,
    OpenTaskManager,
    Quit,
}

impl ContextMenuCommand {
    #[must_use]
    #[cfg(test)]
    pub const fn from_native_id(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::Open),
            2 => Some(Self::Pin),
            3 => Some(Self::Unpin),
            4 => Some(Self::Quit),
            5 => Some(Self::PreviewFocus),
            6 => Some(Self::PreviewClose),
            7 => Some(Self::AddSeparator),
            8 => Some(Self::RemoveSeparator),
            9 => Some(Self::MoveLeft),
            10 => Some(Self::MoveRight),
            11 => Some(Self::OpenTaskManager),
            _ => None,
        }
    }

    #[must_use]
    #[expect(
        dead_code,
        reason = "retained for the isolated Win32 native-menu adapter"
    )]
    pub const fn native_id(self) -> u16 {
        match self {
            Self::Open => 1,
            Self::Pin => 2,
            Self::Unpin => 3,
            Self::Quit => 4,
            Self::PreviewFocus => 5,
            Self::PreviewClose => 6,
            Self::AddSeparator => 7,
            Self::RemoveSeparator => 8,
            Self::MoveLeft => 9,
            Self::MoveRight => 10,
            Self::OpenTaskManager => 11,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueuedDockAction {
    Launch(String),
    Effect(Effect),
    Preview(PreviewQueuedAction),
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockAnimator {
    position: ScalarSpring,
    strength: ScalarSpring,
    material_strength: ScalarSpring,
    initialized: bool,
    active: bool,
}

impl DockAnimator {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            position: ScalarSpring::new(0.0),
            strength: ScalarSpring::new(0.0),
            material_strength: ScalarSpring::new(0.0),
            initialized: false,
            active: false,
        }
    }

    #[must_use]
    pub const fn is_idle(self) -> bool {
        !self.active
    }

    #[must_use]
    pub const fn position_x(self) -> f32 {
        self.position.value
    }

    #[must_use]
    pub const fn strength(self) -> f32 {
        self.strength.value
    }

    #[must_use]
    pub const fn material_strength(self) -> f32 {
        self.material_strength.value
    }

    pub fn retarget(&mut self, position_x: f32, strength: f32) {
        if !position_x.is_finite() || !strength.is_finite() {
            *self = Self::new();
            return;
        }
        let strength = strength.clamp(0.0, 1.0);
        if !self.initialized
            || (self.strength.value <= STRENGTH_EPSILON && self.strength.target <= STRENGTH_EPSILON)
        {
            self.position.snap(position_x);
            self.initialized = true;
        } else {
            self.position.target = position_x;
        }
        self.strength.target = strength;
        self.material_strength.target = strength;
        self.active = !self.is_settled();
    }

    pub fn retarget_strength(&mut self, strength: f32) {
        let strength = if strength.is_finite() {
            strength.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.strength.target = strength;
        self.material_strength.target = strength;
        self.active = !self.is_settled();
    }

    pub fn advance(&mut self, delta_seconds: f32) -> bool {
        if !self.is_finite() {
            *self = Self::new();
            return true;
        }
        if !self.active || !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return false;
        }
        let previous_position = self.position.value;
        let previous_strength = self.strength.value;
        let previous_material_strength = self.material_strength.value;
        let step = delta_seconds.min(MAX_FRAME_SECONDS);
        self.position.integrate(step, POSITION_SPRING_FREQUENCY_HZ);
        self.strength.integrate(step, STRENGTH_SPRING_FREQUENCY_HZ);
        self.material_strength
            .integrate(step, MATERIAL_SPRING_FREQUENCY_HZ);
        self.settle_components();
        self.active = !self.is_settled();
        (self.position.value - previous_position).abs() > f32::EPSILON
            || (self.strength.value - previous_strength).abs() > f32::EPSILON
            || (self.material_strength.value - previous_material_strength).abs() > f32::EPSILON
    }

    pub fn snap_to_target(&mut self) -> bool {
        let changed = (self.position.value - self.position.target).abs() > f32::EPSILON
            || (self.strength.value - self.strength.target).abs() > f32::EPSILON
            || (self.material_strength.value - self.material_strength.target).abs() > f32::EPSILON;
        self.position.snap(self.position.target);
        self.strength.snap(self.strength.target);
        self.material_strength.snap(self.material_strength.target);
        self.active = false;
        changed
    }

    fn settle_components(&mut self) {
        if self
            .position
            .is_settled(POSITION_EPSILON, POSITION_VELOCITY_EPSILON)
        {
            self.position.snap(self.position.target);
        }
        if self
            .strength
            .is_settled(STRENGTH_EPSILON, STRENGTH_VELOCITY_EPSILON)
        {
            self.strength.snap(self.strength.target);
        }
        if self
            .material_strength
            .is_settled(STRENGTH_EPSILON, STRENGTH_VELOCITY_EPSILON)
        {
            self.material_strength.snap(self.material_strength.target);
        }
    }

    fn is_settled(self) -> bool {
        self.position
            .is_settled(POSITION_EPSILON, POSITION_VELOCITY_EPSILON)
            && self
                .strength
                .is_settled(STRENGTH_EPSILON, STRENGTH_VELOCITY_EPSILON)
            && self
                .material_strength
                .is_settled(STRENGTH_EPSILON, STRENGTH_VELOCITY_EPSILON)
    }

    fn is_finite(self) -> bool {
        self.position.is_finite() && self.strength.is_finite() && self.material_strength.is_finite()
    }
}

impl Default for DockAnimator {
    fn default() -> Self {
        Self::new()
    }
}

const POSITION_SPRING_FREQUENCY_HZ: f32 = 10.0;
const STRENGTH_SPRING_FREQUENCY_HZ: f32 = 8.5;
const MATERIAL_SPRING_FREQUENCY_HZ: f32 = 7.5;
const MAX_FRAME_SECONDS: f32 = 1.0 / 20.0;
const POSITION_EPSILON: f32 = 0.01;
const POSITION_VELOCITY_EPSILON: f32 = 0.05;
const STRENGTH_EPSILON: f32 = 0.0005;
const STRENGTH_VELOCITY_EPSILON: f32 = 0.0025;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScalarSpring {
    value: f32,
    target: f32,
    velocity: f32,
}

impl ScalarSpring {
    const fn new(value: f32) -> Self {
        Self {
            value,
            target: value,
            velocity: 0.0,
        }
    }

    fn integrate(&mut self, delta_seconds: f32, frequency_hz: f32) {
        let omega = std::f32::consts::TAU * frequency_hz;
        let displacement = self.value - self.target;
        let decay = (-omega * delta_seconds).exp();
        let impulse = (self.velocity + omega * displacement) * delta_seconds;
        self.value = self.target + (displacement + impulse) * decay;
        self.velocity = (self.velocity - omega * impulse) * decay;
    }

    fn snap(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
    }

    fn is_settled(self, value_epsilon: f32, velocity_epsilon: f32) -> bool {
        (self.target - self.value).abs() <= value_epsilon && self.velocity.abs() <= velocity_epsilon
    }

    fn is_finite(self) -> bool {
        self.value.is_finite() && self.target.is_finite() && self.velocity.is_finite()
    }
}

#[derive(Debug)]
pub enum DockControllerError {
    Transition(TransitionError),
    AppId(AppIdError),
    MissingFileName,
}

impl fmt::Display for DockControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transition(error) => write!(formatter, "{error}"),
            Self::AppId(error) => write!(formatter, "{error}"),
            Self::MissingFileName => formatter.write_str("dropped path must include a file name"),
        }
    }
}

impl Error for DockControllerError {}

impl From<TransitionError> for DockControllerError {
    fn from(value: TransitionError) -> Self {
        Self::Transition(value)
    }
}

impl From<AppIdError> for DockControllerError {
    fn from(value: AppIdError) -> Self {
        Self::AppId(value)
    }
}
