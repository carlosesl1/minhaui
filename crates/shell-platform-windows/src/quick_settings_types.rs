use shell_core::QuickControlKind;
use shell_renderer::{QuickSettingsMediaAction, QuickSettingsMediaSessionId};

use crate::ProjectionMode;
use crate::media_session_types::MediaWorkerCommand;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickControlAvailability {
    Unsupported,
    Available { active: bool },
    RouteOnly,
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
    SetProjectionMode(ProjectionMode),
    OpenMediaSessions,
    MediaAction(QuickSettingsMediaAction),
    SelectMediaSession(QuickSettingsMediaSessionId),
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
    Intent(QuickSettingsIntent),
}
