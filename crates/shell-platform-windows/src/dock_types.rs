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
}

impl DockRuntimeConfig {
    #[must_use]
    pub const fn new(alignment: DockAlignment) -> Self {
        Self {
            layout: DockLayoutConfig::new(alignment),
            autohide: false,
            reveal_zone_height: 8.0,
        }
    }

    #[must_use]
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
    pub const fn with_autohide(mut self, enabled: bool) -> Self {
        self.autohide = enabled;
        self
    }

    #[must_use]
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
pub enum ContextMenuCommand {
    Open,
    Unpin,
    Pin,
    PreviewFocus,
    PreviewClose,
    Quit,
}

impl ContextMenuCommand {
    #[must_use]
    pub const fn from_native_id(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::Open),
            2 => Some(Self::Pin),
            3 => Some(Self::Unpin),
            4 => Some(Self::Quit),
            5 => Some(Self::PreviewFocus),
            6 => Some(Self::PreviewClose),
            _ => None,
        }
    }

    #[must_use]
    pub const fn native_id(self) -> u16 {
        match self {
            Self::Open => 1,
            Self::Pin => 2,
            Self::Unpin => 3,
            Self::Quit => 4,
            Self::PreviewFocus => 5,
            Self::PreviewClose => 6,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DockAnimator {
    active_until_ms: u64,
}

impl DockAnimator {
    #[must_use]
    pub const fn new() -> Self {
        Self { active_until_ms: 0 }
    }

    #[must_use]
    pub const fn is_idle(self) -> bool {
        self.active_until_ms == 0
    }

    pub const fn start(&mut self, now_ms: u64, duration_ms: u64) {
        self.active_until_ms = now_ms + duration_ms;
    }

    pub const fn advance(&mut self, now_ms: u64) {
        if now_ms >= self.active_until_ms {
            self.active_until_ms = 0;
        }
    }
}

impl Default for DockAnimator {
    fn default() -> Self {
        Self::new()
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
