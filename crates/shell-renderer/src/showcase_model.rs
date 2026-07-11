#![deny(unsafe_code)]

use crate::Rgba8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
pub enum ShowcasePrimitive {
    Button,
    Slider,
    DeviceRow,
    CalendarCell,
    Popover,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShowcaseItem {
    kind: ShowcasePrimitive,
    state: ShowcaseState,
    label: &'static str,
}

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
    pub text_primary: Rgba8,
    pub text_secondary: Rgba8,
    pub text_disabled: Rgba8,
    pub accent: Rgba8,
    pub focus_outer: Rgba8,
    pub error: Rgba8,
    pub dock_radius: f32,
    pub control_radius: f32,
    pub popover_radius: f32,
}

impl ShowcaseTokens {
    #[must_use]
    pub const fn obsidian_glass() -> Self {
        Self {
            surface_base: Rgba8::new(0x11, 0x15, 0x1B, 0xEF),
            surface_raised: Rgba8::new(0x18, 0x1D, 0x24, 0xF2),
            surface_hover: Rgba8::new(0xFF, 0xFF, 0xFF, 0x12),
            surface_pressed: Rgba8::new(0xFF, 0xFF, 0xFF, 0x1F),
            surface_selected: Rgba8::new(0x2D, 0x7D, 0xFF, 0x2E),
            text_primary: Rgba8::new(0xF5, 0xF7, 0xFA, 0xFF),
            text_secondary: Rgba8::new(0xB8, 0xC0, 0xCC, 0xFF),
            text_disabled: Rgba8::new(0x68, 0x71, 0x7D, 0xFF),
            accent: Rgba8::new(0x4C, 0x9A, 0xFF, 0xFF),
            focus_outer: Rgba8::new(0x9D, 0xCA, 0xFF, 0xFF),
            error: Rgba8::new(0xFF, 0x73, 0x73, 0xFF),
            dock_radius: 18.0,
            control_radius: 6.0,
            popover_radius: 14.0,
        }
    }
}

#[must_use]
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
