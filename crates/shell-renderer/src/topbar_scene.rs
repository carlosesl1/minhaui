use shell_core::{TopbarIntent, TopbarModuleKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopbarDensity {
    Compact,
    Comfortable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopbarModuleStatus {
    Neutral,
    Good,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopbarModuleVisual {
    kind: TopbarModuleKind,
    icon: String,
    text: String,
    status: TopbarModuleStatus,
    intent: Option<TopbarIntent>,
}

impl TopbarModuleVisual {
    #[must_use]
    pub fn new(
        kind: TopbarModuleKind,
        icon: &str,
        text: &str,
        status: TopbarModuleStatus,
        intent: Option<TopbarIntent>,
    ) -> Self {
        Self {
            kind,
            icon: icon.to_owned(),
            text: text.to_owned(),
            status,
            intent,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> TopbarModuleKind {
        self.kind
    }

    #[must_use]
    pub fn icon(&self) -> &str {
        &self.icon
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn status(&self) -> TopbarModuleStatus {
        self.status
    }

    #[must_use]
    pub const fn intent(&self) -> Option<TopbarIntent> {
        self.intent
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopbarScene {
    density: TopbarDensity,
    modules: Vec<TopbarModuleVisual>,
    focused_module: Option<TopbarModuleKind>,
    hovered_module: Option<TopbarModuleKind>,
    pressed_module: Option<TopbarModuleKind>,
    active_module: Option<TopbarModuleKind>,
    text_scale: f32,
}

impl TopbarScene {
    #[must_use]
    pub const fn new(density: TopbarDensity, modules: Vec<TopbarModuleVisual>) -> Self {
        Self {
            density,
            modules,
            focused_module: None,
            hovered_module: None,
            pressed_module: None,
            active_module: None,
            text_scale: 1.0,
        }
    }

    #[must_use]
    pub const fn density(&self) -> TopbarDensity {
        self.density
    }

    #[must_use]
    pub fn modules(&self) -> &[TopbarModuleVisual] {
        &self.modules
    }

    #[must_use]
    pub const fn focused_module(&self) -> Option<TopbarModuleKind> {
        self.focused_module
    }

    #[must_use]
    pub const fn hovered_module(&self) -> Option<TopbarModuleKind> {
        self.hovered_module
    }

    #[must_use]
    pub const fn pressed_module(&self) -> Option<TopbarModuleKind> {
        self.pressed_module
    }

    #[must_use]
    pub const fn active_module(&self) -> Option<TopbarModuleKind> {
        self.active_module
    }

    #[must_use]
    pub const fn text_scale(&self) -> f32 {
        self.text_scale
    }

    #[must_use]
    pub const fn with_focused_module(mut self, focused_module: Option<TopbarModuleKind>) -> Self {
        self.focused_module = focused_module;
        self
    }

    #[must_use]
    pub const fn with_hovered_module(mut self, hovered_module: Option<TopbarModuleKind>) -> Self {
        self.hovered_module = hovered_module;
        self
    }

    #[must_use]
    pub const fn with_pressed_module(mut self, pressed_module: Option<TopbarModuleKind>) -> Self {
        self.pressed_module = pressed_module;
        self
    }

    #[must_use]
    pub const fn with_active_module(mut self, active_module: Option<TopbarModuleKind>) -> Self {
        self.active_module = active_module;
        self
    }

    #[must_use]
    pub fn with_text_scale(mut self, text_scale: f32) -> Self {
        self.text_scale = text_scale.clamp(1.0, 2.5);
        self
    }
}
