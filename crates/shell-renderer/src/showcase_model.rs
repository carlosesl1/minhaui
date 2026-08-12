#![deny(unsafe_code)]

use crate::Rgba8;

/// User-controlled visual preferences that are safe to apply to every native
/// surface. Values are normalized at the renderer boundary so malformed or
/// older configuration files cannot create invisible or unusable chrome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisualPreferences {
    opacity_percent: u8,
    corner_radius: u16,
}

impl VisualPreferences {
    #[must_use]
    pub const fn new(opacity_percent: u8, corner_radius: u16) -> Self {
        Self {
            opacity_percent: if opacity_percent < 60 {
                60
            } else if opacity_percent > 100 {
                100
            } else {
                opacity_percent
            },
            corner_radius: if corner_radius < 6 {
                6
            } else if corner_radius > 24 {
                24
            } else {
                corner_radius
            },
        }
    }

    #[must_use]
    pub const fn opacity_percent(self) -> u8 {
        self.opacity_percent
    }

    #[must_use]
    pub const fn corner_radius(self) -> u16 {
        self.corner_radius
    }

    pub(crate) const fn apply_background_alpha(self, mut color: Rgba8) -> Rgba8 {
        color.a = ((color.a as u16 * self.opacity_percent as u16 + 50) / 100) as u8;
        color
    }
}

impl Default for VisualPreferences {
    fn default() -> Self {
        Self::new(94, 18)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub enum ShowcaseState {
    Rest,
    Hover,
    Pressed,
    Active,
    FocusVisible,
    Unavailable,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub enum ShowcasePrimitive {
    Button,
    Slider,
    DeviceRow,
    CalendarCell,
    Popover,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub struct ShowcaseItem {
    kind: ShowcasePrimitive,
    state: ShowcaseState,
    label: &'static str,
}

#[cfg(test)]
impl ShowcaseItem {
    #[must_use]
    pub const fn kind(self) -> ShowcasePrimitive {
        self.kind
    }

    #[must_use]
    pub const fn state(self) -> ShowcaseState {
        self.state
    }

    #[must_use]
    #[expect(
        dead_code,
        reason = "showcase labels remain metadata for future diagnostics"
    )]
    pub const fn label(self) -> &'static str {
        self.label
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShowcaseTokens {
    pub surface_base: Rgba8,
    pub surface_raised: Rgba8,
    pub surface_hover: Rgba8,
    pub surface_pressed: Rgba8,
    pub surface_selected: Rgba8,
    pub dock_luminance: Rgba8,
    pub dock_veil: Rgba8,
    pub dock_reflection: Rgba8,
    pub dock_depth: Rgba8,
    pub dock_inset_edge: Rgba8,
    pub topbar_tint: Rgba8,
    pub text_primary: Rgba8,
    pub text_secondary: Rgba8,
    pub text_disabled: Rgba8,
    pub accent: Rgba8,
    pub focus_outer: Rgba8,
    pub rim_outer: Rgba8,
    pub rim_inner: Rgba8,
    pub warning: Rgba8,
    pub error: Rgba8,
    pub dock_radius: f32,
    pub control_radius: f32,
    pub popover_radius: f32,
}

impl ShowcaseTokens {
    #[must_use]
    pub const fn obsidian_glass() -> Self {
        Self {
            surface_base: Rgba8::new(0x20, 0x22, 0x26, 0xD2),
            surface_raised: Rgba8::new(0xFF, 0xFF, 0xFF, 0x0A),
            surface_hover: Rgba8::new(0xFF, 0xFF, 0xFF, 0x12),
            surface_pressed: Rgba8::new(0xFF, 0xFF, 0xFF, 0x1F),
            surface_selected: Rgba8::new(0x2D, 0x7D, 0xFF, 0x2E),
            dock_luminance: Rgba8::new(0x4D, 0x4D, 0x4D, 0x4D),
            dock_veil: Rgba8::new(0x1A, 0x1A, 0x1A, 0x1A),
            dock_reflection: Rgba8::new(0xFF, 0xFF, 0xFF, 0x14),
            dock_depth: Rgba8::new(0x16, 0x16, 0x16, 0x18),
            dock_inset_edge: Rgba8::new(0x80, 0x80, 0x80, 0xFF),
            topbar_tint: Rgba8::new(0x11, 0x15, 0x1B, 0x8F),
            text_primary: Rgba8::new(0xF5, 0xF7, 0xFA, 0xFF),
            text_secondary: Rgba8::new(0xB8, 0xC0, 0xCC, 0xFF),
            text_disabled: Rgba8::new(0x68, 0x71, 0x7D, 0xFF),
            accent: Rgba8::new(0x4C, 0x9A, 0xFF, 0xFF),
            focus_outer: Rgba8::new(0x9D, 0xCA, 0xFF, 0xFF),
            rim_outer: Rgba8::new(0x00, 0x00, 0x00, 0x28),
            rim_inner: Rgba8::new(0xFF, 0xFF, 0xFF, 0x24),
            warning: Rgba8::new(0xF2, 0xB8, 0x4B, 0xFF),
            error: Rgba8::new(0xFF, 0x73, 0x73, 0xFF),
            dock_radius: 15.0,
            control_radius: 6.0,
            popover_radius: 12.0,
        }
    }

    #[must_use]
    pub const fn solid_fallback() -> Self {
        let mut tokens = Self::obsidian_glass();
        tokens.surface_base = Rgba8::new(0x1C, 0x21, 0x29, 0xFF);
        tokens.dock_luminance = Rgba8::new(0x25, 0x2A, 0x32, 0xFF);
        tokens.dock_veil = Rgba8::new(0x00, 0x00, 0x00, 0x00);
        tokens.topbar_tint = Rgba8::new(0x1C, 0x21, 0x29, 0xFF);
        tokens
    }

    #[must_use]
    pub(crate) fn with_preferences(
        mut self,
        preferences: VisualPreferences,
        solid_material: bool,
    ) -> Self {
        if !solid_material {
            self.surface_base = preferences.apply_background_alpha(self.surface_base);
            self.surface_raised = preferences.apply_background_alpha(self.surface_raised);
            self.surface_hover = preferences.apply_background_alpha(self.surface_hover);
            self.surface_pressed = preferences.apply_background_alpha(self.surface_pressed);
            self.surface_selected = preferences.apply_background_alpha(self.surface_selected);
            self.dock_luminance = preferences.apply_background_alpha(self.dock_luminance);
            self.dock_veil = preferences.apply_background_alpha(self.dock_veil);
            self.dock_reflection = preferences.apply_background_alpha(self.dock_reflection);
            self.topbar_tint = preferences.apply_background_alpha(self.topbar_tint);
        }
        let radius = preferences.corner_radius as f32;
        self.dock_radius = radius;
        self.popover_radius = radius;
        self.control_radius = (radius / 3.0).clamp(4.0, 8.0);
        self
    }
}

#[cfg(test)]
mod visual_preferences_tests {
    use super::{ShowcaseTokens, VisualPreferences};

    #[test]
    fn preferences_are_bounded_before_the_native_renderer_uses_them() {
        assert_eq!(VisualPreferences::new(0, 0), VisualPreferences::new(60, 6));
        assert_eq!(
            VisualPreferences::new(u8::MAX, u16::MAX),
            VisualPreferences::new(100, 24)
        );
    }

    #[test]
    fn transparent_tokens_apply_opacity_without_fading_text() {
        let base = ShowcaseTokens::obsidian_glass();
        let customized = base.with_preferences(VisualPreferences::new(60, 22), false);

        assert!(customized.surface_base.a < base.surface_base.a);
        assert_eq!(customized.text_primary, base.text_primary);
        assert_eq!(customized.dock_radius, 22.0);
        assert_eq!(customized.popover_radius, 22.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockInsetShadow {
    offset_y: f32,
    blur: f32,
    spread: f32,
    color: Rgba8,
}

impl DockInsetShadow {
    #[must_use]
    pub const fn new(offset_y: f32, blur: f32, spread: f32, color: Rgba8) -> Self {
        Self {
            offset_y,
            blur,
            spread,
            color,
        }
    }

    #[must_use]
    pub const fn offset_y(self) -> f32 {
        self.offset_y
    }

    #[must_use]
    pub const fn blur(self) -> f32 {
        self.blur
    }

    #[must_use]
    pub const fn spread(self) -> f32 {
        self.spread
    }

    #[must_use]
    pub const fn color(self) -> Rgba8 {
        self.color
    }
}

#[must_use]
pub const fn dock_inset_shadows() -> [DockInsetShadow; 4] {
    [
        DockInsetShadow::new(1.25, 2.0, -1.5, Rgba8::new(0xC0, 0xC0, 0xC0, 0x24)),
        DockInsetShadow::new(-1.25, 2.0, -1.5, Rgba8::new(0xC0, 0xC0, 0xC0, 0x24)),
        DockInsetShadow::new(8.0, 16.0, -14.0, Rgba8::new(0x16, 0x16, 0x16, 0x30)),
        DockInsetShadow::new(-8.0, 16.0, -14.0, Rgba8::new(0x16, 0x16, 0x16, 0x30)),
    ]
}

#[must_use]
#[cfg(test)]
pub const fn showcase_primitives() -> [ShowcaseItem; 9] {
    [
        ShowcaseItem {
            kind: ShowcasePrimitive::Button,
            state: ShowcaseState::Rest,
            label: "Rest",
        },
        ShowcaseItem {
            kind: ShowcasePrimitive::Button,
            state: ShowcaseState::Hover,
            label: "Hover",
        },
        ShowcaseItem {
            kind: ShowcasePrimitive::Button,
            state: ShowcaseState::Pressed,
            label: "Pressed",
        },
        ShowcaseItem {
            kind: ShowcasePrimitive::Button,
            state: ShowcaseState::Active,
            label: "Active",
        },
        ShowcaseItem {
            kind: ShowcasePrimitive::Button,
            state: ShowcaseState::FocusVisible,
            label: "Focus",
        },
        ShowcaseItem {
            kind: ShowcasePrimitive::Slider,
            state: ShowcaseState::Unavailable,
            label: "Unavailable",
        },
        ShowcaseItem {
            kind: ShowcasePrimitive::DeviceRow,
            state: ShowcaseState::Error,
            label: "Device error",
        },
        ShowcaseItem {
            kind: ShowcasePrimitive::CalendarCell,
            state: ShowcaseState::Active,
            label: "Calendar 11",
        },
        ShowcaseItem {
            kind: ShowcasePrimitive::Popover,
            state: ShowcaseState::Rest,
            label: "Popover",
        },
    ]
}
