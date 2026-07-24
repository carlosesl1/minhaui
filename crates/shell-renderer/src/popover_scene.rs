use shell_core::Popover;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopoverContentState {
    Loading,
    Ready,
    Empty,
    Error,
    Offline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopoverLayoutStyle {
    Compact,
    SystemPanel,
    BalancedApps,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PopoverRow {
    label: String,
    detail: String,
    icon_source: Option<String>,
    icon_glyph: Option<String>,
    enabled: bool,
    section_start: bool,
}

impl PopoverRow {
    #[must_use]
    pub fn new(label: &str, detail: &str, enabled: bool) -> Self {
        Self {
            label: label.to_owned(),
            detail: detail.to_owned(),
            icon_source: None,
            icon_glyph: None,
            enabled,
            section_start: false,
        }
    }

    #[must_use]
    pub fn with_icon_source(mut self, source: Option<String>) -> Self {
        self.icon_source = source;
        self
    }

    #[must_use]
    pub fn with_icon_glyph(mut self, glyph: &str) -> Self {
        self.icon_glyph = Some(glyph.to_owned());
        self
    }

    #[must_use]
    pub const fn with_section_start(mut self) -> Self {
        self.section_start = true;
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
    pub fn icon_glyph(&self) -> Option<&str> {
        self.icon_glyph.as_deref()
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn section_start(&self) -> bool {
        self.section_start
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
    layout_style: PopoverLayoutStyle,
    anchor_x: Option<f32>,
    header_detail: Option<String>,
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
            layout_style: PopoverLayoutStyle::Compact,
            anchor_x: None,
            header_detail: None,
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
    pub const fn with_layout_style(mut self, style: PopoverLayoutStyle) -> Self {
        self.layout_style = style;
        self
    }

    #[must_use]
    pub fn with_header_detail(mut self, detail: &str) -> Self {
        self.header_detail = Some(detail.to_owned());
        self
    }

    #[must_use]
    pub fn with_anchor_x(mut self, anchor_x: f32) -> Self {
        self.anchor_x = anchor_x.is_finite().then_some(anchor_x);
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

    #[must_use]
    pub const fn layout_style(&self) -> PopoverLayoutStyle {
        self.layout_style
    }

    #[must_use]
    pub const fn anchor_x(&self) -> Option<f32> {
        self.anchor_x
    }

    #[must_use]
    pub fn header_detail(&self) -> Option<&str> {
        self.header_detail.as_deref()
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
