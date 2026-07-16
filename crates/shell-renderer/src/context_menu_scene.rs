#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextMenuEntry {
    Action { label: String, enabled: bool },
    Separator,
}

impl ContextMenuEntry {
    #[must_use]
    pub fn action(label: &str, enabled: bool) -> Self {
        Self::Action {
            label: label.to_owned(),
            enabled,
        }
    }

    #[must_use]
    pub const fn separator() -> Self {
        Self::Separator
    }

    #[must_use]
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Action { label, .. } => Some(label),
            Self::Separator => None,
        }
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        matches!(self, Self::Action { enabled: true, .. })
    }

    #[must_use]
    pub const fn is_separator(&self) -> bool {
        matches!(self, Self::Separator)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextMenuScene {
    entries: Vec<ContextMenuEntry>,
    focused: Option<usize>,
}

impl ContextMenuScene {
    #[must_use]
    pub fn new(entries: Vec<ContextMenuEntry>, focused: Option<usize>) -> Self {
        Self { entries, focused }
    }

    #[must_use]
    pub fn entries(&self) -> &[ContextMenuEntry] {
        &self.entries
    }

    #[must_use]
    pub const fn focused(&self) -> Option<usize> {
        self.focused
    }
}
