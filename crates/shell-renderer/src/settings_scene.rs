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
}

impl SettingsScene {
    #[must_use]
    pub fn new(title: &str, rows: Vec<SettingsRow>, focused: Option<usize>) -> Self {
        Self {
            title: title.to_owned(),
            rows,
            focused,
        }
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
}
