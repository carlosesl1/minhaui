use serde::{Deserialize, Serialize};

use crate::ThemePayload;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockAlignmentPreference {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DockSettings {
    item_size: u16,
    spacing: u16,
    alignment: DockAlignmentPreference,
    magnification: u16,
    animation_ms: u16,
}

impl Default for DockSettings {
    fn default() -> Self {
        Self {
            item_size: 36,
            spacing: 9,
            alignment: DockAlignmentPreference::Center,
            magnification: 122,
            animation_ms: 140,
        }
    }
}

impl DockSettings {
    #[must_use]
    pub const fn item_size(&self) -> u16 {
        self.item_size
    }

    #[must_use]
    pub const fn spacing(&self) -> u16 {
        self.spacing
    }

    #[must_use]
    pub const fn alignment(&self) -> DockAlignmentPreference {
        self.alignment
    }

    #[must_use]
    pub const fn magnification(&self) -> u16 {
        self.magnification
    }

    #[must_use]
    pub const fn animation_ms(&self) -> u16 {
        self.animation_ms
    }

    #[must_use]
    pub const fn with_item_size(mut self, value: u16) -> Self {
        self.item_size = value;
        self
    }

    #[must_use]
    pub const fn with_spacing(mut self, value: u16) -> Self {
        self.spacing = value;
        self
    }

    #[must_use]
    pub const fn with_alignment(mut self, value: DockAlignmentPreference) -> Self {
        self.alignment = value;
        self
    }

    #[must_use]
    pub const fn with_magnification(mut self, value: u16) -> Self {
        self.magnification = value;
        self
    }

    #[must_use]
    pub const fn with_animation_ms(mut self, value: u16) -> Self {
        self.animation_ms = value;
        self
    }

    pub(crate) const fn validate(&self) -> bool {
        matches!(self.item_size, 36..=72)
            && matches!(self.spacing, 4..=20)
            && matches!(self.magnification, 100..=140)
            && matches!(self.animation_ms, 0..=280)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopbarDensityPreference {
    Compact,
    #[default]
    Comfortable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TopbarSettings {
    density: TopbarDensityPreference,
}

impl Default for TopbarSettings {
    fn default() -> Self {
        Self {
            density: TopbarDensityPreference::Comfortable,
        }
    }
}

impl TopbarSettings {
    #[must_use]
    pub const fn density(&self) -> TopbarDensityPreference {
        self.density
    }

    #[must_use]
    pub const fn with_density(mut self, value: TopbarDensityPreference) -> Self {
        self.density = value;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppearanceSettings {
    theme: ThemePayload,
    opacity: u8,
    blur: u8,
    shadow: u8,
    radius: u16,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: ThemePayload::default(),
            opacity: 94,
            blur: 16,
            shadow: 2,
            radius: 18,
        }
    }
}

impl AppearanceSettings {
    #[must_use]
    pub const fn theme(&self) -> &ThemePayload {
        &self.theme
    }

    #[must_use]
    pub const fn blur(&self) -> u8 {
        self.blur
    }

    #[must_use]
    pub const fn opacity(&self) -> u8 {
        self.opacity
    }

    #[must_use]
    pub const fn shadow(&self) -> u8 {
        self.shadow
    }

    #[must_use]
    pub const fn radius(&self) -> u16 {
        self.radius
    }

    #[must_use]
    pub fn with_theme(mut self, theme: ThemePayload) -> Self {
        self.theme = theme;
        self
    }

    #[must_use]
    pub const fn with_blur(mut self, value: u8) -> Self {
        self.blur = value;
        self
    }

    #[must_use]
    pub const fn with_opacity(mut self, value: u8) -> Self {
        self.opacity = value;
        self
    }

    #[must_use]
    pub const fn with_shadow(mut self, value: u8) -> Self {
        self.shadow = value;
        self
    }

    #[must_use]
    pub const fn with_radius(mut self, value: u16) -> Self {
        self.radius = value;
        self
    }

    pub(crate) fn validate(&self) -> bool {
        self.theme.validate().is_ok()
            && matches!(self.opacity, 60..=100)
            && self.blur <= 32
            && self.shadow <= 3
            && matches!(self.radius, 6..=24)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BehaviorSettings {
    startup: bool,
    reduced_motion: bool,
}

impl BehaviorSettings {
    #[must_use]
    pub const fn startup(self) -> bool {
        self.startup
    }

    #[must_use]
    pub const fn reduced_motion(self) -> bool {
        self.reduced_motion
    }

    #[must_use]
    pub const fn with_startup(mut self, value: bool) -> Self {
        self.startup = value;
        self
    }

    #[must_use]
    pub const fn with_reduced_motion(mut self, value: bool) -> Self {
        self.reduced_motion = value;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdvancedSettings {
    recovery_enabled: bool,
    diagnostics_enabled: bool,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            recovery_enabled: true,
            diagnostics_enabled: false,
        }
    }
}

impl AdvancedSettings {
    #[must_use]
    pub const fn recovery_enabled(self) -> bool {
        self.recovery_enabled
    }

    #[must_use]
    pub const fn diagnostics_enabled(self) -> bool {
        self.diagnostics_enabled
    }

    #[must_use]
    pub const fn with_recovery_enabled(mut self, value: bool) -> Self {
        self.recovery_enabled = value;
        self
    }

    #[must_use]
    pub const fn with_diagnostics_enabled(mut self, value: bool) -> Self {
        self.diagnostics_enabled = value;
        self
    }
}
