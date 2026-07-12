use shell_core::{Popover, TopbarModuleKind};

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
    intent: Option<Popover>,
}

impl TopbarModuleVisual {
    #[must_use]
    pub fn new(
        kind: TopbarModuleKind,
        icon: &str,
        text: &str,
        status: TopbarModuleStatus,
        intent: Option<Popover>,
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
    pub const fn intent(&self) -> Option<Popover> {
        self.intent
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopbarScene {
    density: TopbarDensity,
    modules: Vec<TopbarModuleVisual>,
    focused_module: Option<TopbarModuleKind>,
}

impl TopbarScene {
    #[must_use]
    pub const fn new(density: TopbarDensity, modules: Vec<TopbarModuleVisual>) -> Self {
        Self {
            density,
            modules,
            focused_module: None,
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
    pub const fn with_focused_module(mut self, focused_module: Option<TopbarModuleKind>) -> Self {
        self.focused_module = focused_module;
        self
    }
}
