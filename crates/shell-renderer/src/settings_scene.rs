#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsRow {
    label: String,
    detail: String,
    enabled: bool,
}

impl SettingsRow {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsScene {
    title: String,
    rows: Vec<SettingsRow>,
    focused: Option<usize>,
    quick_controls: Option<Vec<QuickControlSettingsRow>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickControlSettingsRow {
    kind: QuickControlKind,
    label: String,
    visible: bool,
    can_move_up: bool,
    can_move_down: bool,
}

impl QuickControlSettingsRow {
    #[must_use]
    pub fn new(
        kind: QuickControlKind,
        label: &str,
        visible: bool,
        can_move_up: bool,
        can_move_down: bool,
    ) -> Self {
        Self {
            kind,
            label: label.to_owned(),
            visible,
            can_move_up,
            can_move_down,
        }
    }
    #[must_use]
    pub const fn kind(&self) -> QuickControlKind {
        self.kind
    }
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }
    #[must_use]
    pub const fn can_move_up(&self) -> bool {
        self.can_move_up
    }
    #[must_use]
    pub const fn can_move_down(&self) -> bool {
        self.can_move_down
    }
}

impl SettingsScene {
    #[must_use]
    pub fn new(title: &str, rows: Vec<SettingsRow>, focused: Option<usize>) -> Self {
        Self {
            title: title.to_owned(),
            rows,
            focused,
            quick_controls: None,
        }
    }

    #[must_use]
    pub fn with_quick_controls(mut self, rows: Vec<QuickControlSettingsRow>) -> Self {
        self.quick_controls = Some(rows);
        self
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn rows(&self) -> &[SettingsRow] {
        &self.rows
    }

    #[must_use]
    pub const fn focused(&self) -> Option<usize> {
        self.focused
    }

    #[must_use]
    pub fn quick_controls(&self) -> Option<&[QuickControlSettingsRow]> {
        self.quick_controls.as_deref()
    }
}
use shell_core::QuickControlKind;
