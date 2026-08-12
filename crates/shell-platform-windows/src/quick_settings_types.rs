use shell_core::QuickControlKind;
use shell_renderer::{QuickSettingsMediaAction, QuickSettingsMediaSessionId};

use crate::ProjectionMode;
use crate::brightness_coordinator::BrightnessRequest;
use crate::media_session_types::MediaWorkerCommand;
use crate::night_light_coordinator::NightLightRequest;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AudioSessionId(u64);

impl AudioSessionId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AudioOutputId(u64);

impl AudioOutputId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioSessionSnapshot {
    id: AudioSessionId,
    label: String,
    detail: String,
    volume: u8,
    muted: bool,
    icon_source: Option<String>,
}

impl AudioSessionSnapshot {
    #[must_use]
    pub fn new(id: AudioSessionId, label: &str, detail: &str, volume: u8, muted: bool) -> Self {
        Self {
            id,
            label: label.to_owned(),
            detail: detail.to_owned(),
            volume: volume.min(100),
            muted,
            icon_source: None,
        }
    }
    #[must_use]
    pub fn with_icon_source(mut self, value: &str) -> Self {
        self.icon_source = (!value.is_empty()).then(|| value.to_owned());
        self
    }
    #[must_use]
    pub const fn id(&self) -> AudioSessionId {
        self.id
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
    pub const fn volume(&self) -> u8 {
        self.volume
    }
    #[must_use]
    pub const fn muted(&self) -> bool {
        self.muted
    }
    #[must_use]
    pub fn icon_source(&self) -> Option<&str> {
        self.icon_source.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioOutputSnapshot {
    id: AudioOutputId,
    label: String,
    detail: String,
    selected: bool,
}

impl AudioOutputSnapshot {
    #[must_use]
    pub fn new(id: AudioOutputId, label: &str, detail: &str, selected: bool) -> Self {
        Self {
            id,
            label: label.to_owned(),
            detail: detail.to_owned(),
            selected,
        }
    }
    #[must_use]
    pub const fn id(&self) -> AudioOutputId {
        self.id
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
    pub const fn selected(&self) -> bool {
        self.selected
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AudioPanelSnapshot {
    sessions: Vec<AudioSessionSnapshot>,
    outputs: Vec<AudioOutputSnapshot>,
    spatial_audio: bool,
}

impl AudioPanelSnapshot {
    #[must_use]
    pub fn new(
        sessions: Vec<AudioSessionSnapshot>,
        outputs: Vec<AudioOutputSnapshot>,
        spatial_audio: bool,
    ) -> Self {
        Self {
            sessions,
            outputs,
            spatial_audio,
        }
    }
    #[must_use]
    pub fn sessions(&self) -> &[AudioSessionSnapshot] {
        &self.sessions
    }
    #[must_use]
    pub fn outputs(&self) -> &[AudioOutputSnapshot] {
        &self.outputs
    }
    #[must_use]
    pub const fn spatial_audio(&self) -> bool {
        self.spatial_audio
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoNotDisturbMode {
    Off,
    PriorityOnly,
    AlarmsOnly,
}

impl DoNotDisturbMode {
    #[must_use]
    pub const fn value(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::PriorityOnly => 1,
            Self::AlarmsOnly => 2,
        }
    }

    #[must_use]
    pub const fn active(self) -> bool {
        !matches!(self, Self::Off)
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::PriorityOnly => "Priority only",
            Self::AlarmsOnly => "Alarms only",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickControlAvailability {
    Unsupported,
    Available { active: bool },
    RouteOnly { active: Option<bool> },
    TemporarilyUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuickControlCapability {
    kind: QuickControlKind,
    availability: QuickControlAvailability,
    label: String,
    detail: String,
    value: Option<u8>,
}

impl QuickControlCapability {
    #[must_use]
    pub fn new(
        kind: QuickControlKind,
        availability: QuickControlAvailability,
        label: &str,
        detail: &str,
        value: Option<u8>,
    ) -> Self {
        Self {
            kind,
            availability,
            label: label.to_owned(),
            detail: detail.to_owned(),
            value: value.map(|value| value.min(100)),
        }
    }
    #[must_use]
    pub const fn kind(&self) -> QuickControlKind {
        self.kind
    }
    #[must_use]
    pub const fn availability(&self) -> QuickControlAvailability {
        self.availability
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
    pub const fn value(&self) -> Option<u8> {
        self.value
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QuickSettingsCapabilities {
    controls: Vec<QuickControlCapability>,
}

impl QuickSettingsCapabilities {
    #[must_use]
    pub fn new(controls: Vec<QuickControlCapability>) -> Self {
        Self { controls }
    }
    #[must_use]
    pub fn controls(&self) -> &[QuickControlCapability] {
        &self.controls
    }
    #[must_use]
    pub fn get(&self, kind: QuickControlKind) -> Option<&QuickControlCapability> {
        self.controls.iter().find(|control| control.kind == kind)
    }
    pub fn replace(&mut self, control: QuickControlCapability) {
        if let Some(existing) = self
            .controls
            .iter_mut()
            .find(|existing| existing.kind == control.kind)
        {
            *existing = control;
        } else {
            self.controls.push(control);
        }
    }
    #[must_use]
    pub fn supported_kinds(&self) -> Vec<QuickControlKind> {
        self.controls
            .iter()
            .filter(|control| control.availability != QuickControlAvailability::Unsupported)
            .map(|control| control.kind)
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickSettingsIntent {
    Activate(QuickControlKind),
    SetValue { kind: QuickControlKind, value: u8 },
    SetDoNotDisturbMode(DoNotDisturbMode),
    SetProjectionMode(ProjectionMode),
    OpenMediaSessions,
    MediaAction(QuickSettingsMediaAction),
    SelectMediaSession(QuickSettingsMediaSessionId),
    SetAudioSessionVolume { id: AudioSessionId, value: u8 },
    SelectAudioOutput(AudioOutputId),
    OpenSoundSettings,
    EditControls,
    Dismiss,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickSettingsKey {
    Next,
    Previous,
    Activate,
    Increase,
    Decrease,
    Escape,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueuedQuickSettingsAction {
    Redraw,
    Reflow,
    Media(MediaWorkerCommand),
    NightLight(NightLightRequest),
    Brightness(BrightnessRequest),
    Intent(QuickSettingsIntent),
}
