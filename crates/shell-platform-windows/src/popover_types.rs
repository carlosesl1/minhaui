#![deny(unsafe_code)]

use shell_core::Popover;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopoverKey {
    Next,
    Previous,
    Activate,
    Escape,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PopoverAction {
    OpenSettings,
    SetVolume(u8),
    SelectAudioDevice(String),
    MediaPrevious,
    MediaPlayPause,
    MediaNext,
    ControlCenterToggle(&'static str),
    NetworkDetails,
    ConfirmSession(SessionAction),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionAction {
    Lock,
    Sleep,
    SignOut,
    Restart,
    ShutDown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueuedPopoverAction {
    Redraw,
    Dismiss,
    RequestConfirmation(SessionAction),
    TypedIntent(PopoverAction),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PopoverItem {
    label: String,
    detail: String,
    enabled: bool,
    action: Option<PopoverAction>,
}

impl PopoverItem {
    #[must_use]
    pub fn new(label: &str, detail: &str, enabled: bool, action: Option<PopoverAction>) -> Self {
        Self {
            label: label.to_owned(),
            detail: detail.to_owned(),
            enabled,
            action,
        }
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn action(&self) -> Option<&PopoverAction> {
        self.action.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PopoverLoadState {
    #[expect(
        dead_code,
        reason = "the controller supports asynchronous adapters although current adapters are synchronous"
    )]
    Loading,
    Ready(Vec<PopoverItem>),
    #[expect(
        dead_code,
        reason = "empty adapter results remain a distinct renderer state"
    )]
    Empty,
    Error(String),
    Offline(Vec<PopoverItem>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PopoverPayload {
    kind: Popover,
    state: PopoverLoadState,
}

impl PopoverPayload {
    #[must_use]
    pub const fn new(kind: Popover, state: PopoverLoadState) -> Self {
        Self { kind, state }
    }

    #[must_use]
    pub const fn kind(&self) -> Popover {
        self.kind
    }

    #[must_use]
    pub const fn state(&self) -> &PopoverLoadState {
        &self.state
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PopoverDataError {
    #[error("adapter failed: {0}")]
    Adapter(&'static str),
}

pub trait PopoverDataProvider {
    fn load(&self, kind: Popover) -> Result<PopoverPayload, PopoverDataError>;
}

pub trait WeatherProvider {
    fn forecast(&self) -> Result<WeatherItem, PopoverDataError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeatherItem {
    pub(crate) label: String,
    pub(crate) detail: String,
}

impl WeatherItem {
    #[must_use]
    #[expect(
        dead_code,
        reason = "constructor belongs to the opt-in WeatherProvider adapter seam"
    )]
    pub fn new(label: &str, detail: &str) -> Self {
        Self {
            label: label.to_owned(),
            detail: detail.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeatherAccess {
    Offline,
    #[expect(
        dead_code,
        reason = "online weather is explicitly opt-in and currently disabled by default"
    )]
    OptIn,
}
