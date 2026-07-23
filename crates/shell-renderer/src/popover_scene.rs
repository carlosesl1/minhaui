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
    icon_source: Option<String>,
    enabled: bool,
}

impl PopoverRow {
    #[must_use]
    pub fn new(label: &str, detail: &str, enabled: bool) -> Self {
        Self {
            label: label.to_owned(),
            detail: detail.to_owned(),
            icon_source: None,
            enabled,
        }
    }

    #[must_use]
    pub fn with_icon_source(mut self, source: Option<String>) -> Self {
        self.icon_source = source;
        self
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
    pub fn icon_source(&self) -> Option<&str> {
        self.icon_source.as_deref()
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
    scroll_offset: usize,
    status_text: String,
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
            scroll_offset: 0,
            status_text: default_status_text(state).to_owned(),
        }
    }

    #[must_use]
    pub fn with_scroll_offset(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    #[must_use]
    pub fn with_status_text(mut self, status: &str) -> Self {
        self.status_text = status.to_owned();
        self
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

    #[must_use]
    pub const fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    #[must_use]
    pub fn status_text(&self) -> &str {
        &self.status_text
    }
}

const fn default_status_text(state: PopoverContentState) -> &'static str {
    match state {
        PopoverContentState::Loading => "Loading",
        PopoverContentState::Ready => "Ready",
        PopoverContentState::Empty => "Empty",
        PopoverContentState::Error => "Unavailable",
        PopoverContentState::Offline => "Offline",
    }
}
