use serde::{Deserialize, Serialize};

/// Stable identity for a control exposed by the adaptive Quick Settings panel.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickControlKind {
    Wifi,
    Bluetooth,
    NearbySharing,
    Focus,
    Multitasking,
    Projection,
    Brightness,
    DarkMode,
    NightLight,
    Volume,
    Battery,
    EnergySaver,
}

impl QuickControlKind {
    /// Complete V1 order used to append controls missing from older documents.
    pub const ALL: [Self; 12] = [
        Self::Wifi,
        Self::Bluetooth,
        Self::NearbySharing,
        Self::Focus,
        Self::Multitasking,
        Self::Projection,
        Self::Brightness,
        Self::DarkMode,
        Self::NightLight,
        Self::Volume,
        Self::Battery,
        Self::EnergySaver,
    ];
}

/// Persists one control's relative order and visibility independently of hardware presence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuickControlPlacement {
    kind: QuickControlKind,
    visible: bool,
}

impl QuickControlPlacement {
    #[must_use]
    pub const fn new(kind: QuickControlKind, visible: bool) -> Self {
        Self { kind, visible }
    }

    #[must_use]
    pub const fn kind(self) -> QuickControlKind {
        self.kind
    }

    #[must_use]
    pub const fn visible(self) -> bool {
        self.visible
    }
}
