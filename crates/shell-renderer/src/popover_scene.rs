use shell_core::Popover;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopoverContentState {
    Loading,
    Ready,
    Empty,
    Error,
    Offline,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PopoverRow {
    label: String,
    detail: String,
    enabled: bool,
}

impl PopoverRow {
    #[must_use]
    pub fn new(label: &str, detail: &str, enabled: bool) -> Self {
        Self {
            label: label.to_owned(),
            detail: detail.to_owned(),
            enabled,
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct PopoverScene {
    kind: Popover,
    title: String,
    state: PopoverContentState,
    rows: Vec<PopoverRow>,
    focused: Option<usize>,
}

impl PopoverScene {
    #[must_use]
    pub fn new(
        kind: Popover,
        title: &str,
        state: PopoverContentState,
        rows: Vec<PopoverRow>,
        focused: Option<usize>,
    ) -> Self {
        Self {
            kind,
            title: title.to_owned(),
            state,
            rows,
            focused,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> Popover {
        self.kind
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub const fn state(&self) -> PopoverContentState {
        self.state
    }

    #[must_use]
    pub fn rows(&self) -> &[PopoverRow] {
        &self.rows
    }

    #[must_use]
    pub const fn focused(&self) -> Option<usize> {
        self.focused
    }
}
