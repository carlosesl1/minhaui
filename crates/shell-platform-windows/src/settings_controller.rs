#![deny(unsafe_code)]

use std::error::Error;
use std::fmt;

use shell_config::{
    AppearanceSettings, ConfigError, ShellConfigV1, ThemeError, export_theme, import_theme,
};
use shell_renderer::{SettingsRow, SettingsScene};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsEdit {
    DockItemSize(u16),
    DockSpacing(u16),
    Appearance(AppearanceSettings),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsKey {
    Next,
    Previous,
    Activate,
    Escape,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsSection {
    Dock,
    Topbar,
    Modules,
    Behavior,
    Appearance,
    Performance,
    Accessibility,
    Startup,
    Taskbar,
    AdvancedRecovery,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueuedSettingsAction {
    Redraw,
    OpenSection(SettingsSection),
    Dismiss,
}

#[derive(Debug)]
pub enum SettingsError {
    Config(ConfigError),
    Theme(ThemeError),
}

pub struct SettingsController {
    committed: ShellConfigV1,
    draft: ShellConfigV1,
    preview: ShellConfigV1,
    focus: usize,
}

impl SettingsController {
    #[must_use]
    pub fn new(config: ShellConfigV1) -> Self {
        Self {
            committed: config.clone(),
            draft: config.clone(),
            preview: config,
            focus: 0,
        }
    }

    #[must_use]
    pub const fn committed(&self) -> &ShellConfigV1 {
        &self.committed
    }

    #[must_use]
    pub const fn draft(&self) -> &ShellConfigV1 {
        &self.draft
    }

    #[must_use]
    pub const fn preview(&self) -> &ShellConfigV1 {
        &self.preview
    }

    pub fn edit(&mut self, edit: SettingsEdit) -> Result<(), SettingsError> {
        self.draft = apply_edit(&self.draft, edit);
        self.update_preview()
    }

    pub fn import_theme(&mut self, bytes: &[u8]) -> Result<(), SettingsError> {
        let theme = import_theme(bytes)?;
        self.edit(SettingsEdit::Appearance(
            self.draft.appearance().clone().with_theme(theme),
        ))
    }

    pub fn export_theme(&self) -> Result<Vec<u8>, SettingsError> {
        export_theme(self.preview.appearance().theme()).map_err(SettingsError::Theme)
    }

    pub fn apply(&mut self) -> Result<ShellConfigV1, SettingsError> {
        self.draft.validate()?;
        self.committed = self.draft.clone();
        self.preview = self.draft.clone();
        Ok(self.committed.clone())
    }

    pub fn cancel(&mut self) {
        self.draft = self.committed.clone();
        self.preview = self.committed.clone();
    }

    pub fn reset(&mut self) {
        self.draft = ShellConfigV1::default();
        if self.draft.validate().is_ok() {
            self.preview = self.draft.clone();
        }
    }

    pub fn focus_next(&mut self) {
        self.focus = (self.focus + 1).min(settings_rows().len().saturating_sub(1));
    }

    pub fn focus_previous(&mut self) {
        self.focus = self.focus.saturating_sub(1);
    }

    pub fn handle_key(
        &mut self,
        key: SettingsKey,
    ) -> Result<Vec<QueuedSettingsAction>, SettingsError> {
        match key {
            SettingsKey::Next => {
                self.focus_next();
                Ok(vec![QueuedSettingsAction::Redraw])
            }
            SettingsKey::Previous => {
                self.focus_previous();
                Ok(vec![QueuedSettingsAction::Redraw])
            }
            SettingsKey::Activate => Ok(vec![QueuedSettingsAction::OpenSection(
                section_for_focus(self.focus),
            )]),
            SettingsKey::Escape => {
                self.cancel();
                Ok(vec![QueuedSettingsAction::Dismiss])
            }
        }
    }

    #[must_use]
    pub fn scene(&self) -> SettingsScene {
        SettingsScene::new("Settings", settings_rows(), Some(self.focus))
    }

    fn update_preview(&mut self) -> Result<(), SettingsError> {
        self.draft.validate()?;
        self.preview = self.draft.clone();
        Ok(())
    }
}

fn apply_edit(config: &ShellConfigV1, edit: SettingsEdit) -> ShellConfigV1 {
    match edit {
        SettingsEdit::DockItemSize(value) => {
            config.with_dock(config.dock().clone().with_item_size(value))
        }
        SettingsEdit::DockSpacing(value) => {
            config.with_dock(config.dock().clone().with_spacing(value))
        }
        SettingsEdit::Appearance(appearance) => config.with_appearance(appearance),
    }
}

fn settings_rows() -> Vec<SettingsRow> {
    [
        (
            "Dock",
            "Size, spacing, alignment, magnification, animation, autohide",
        ),
        ("Topbar", "Density, order, and module visibility"),
        ("Modules", "Clock, network, volume, power, notifications"),
        ("Behavior", "Autohide, focus, launch, and confirmations"),
        (
            "Appearance",
            "Colors, opacity, blur, shadow, radius, themes",
        ),
        ("Performance", "Quality, battery, reduced effects"),
        (
            "Accessibility",
            "Reduced motion, high contrast, transparency",
        ),
        ("Startup", "Run at sign-in without accounts or network"),
        ("Taskbar", "Windows taskbar policy"),
        ("Advanced/Recovery", "Backups, diagnostics, reset"),
    ]
    .into_iter()
    .map(|(label, detail)| SettingsRow::new(label, detail, true))
    .collect()
}

const fn section_for_focus(focus: usize) -> SettingsSection {
    match focus {
        0 => SettingsSection::Dock,
        1 => SettingsSection::Topbar,
        2 => SettingsSection::Modules,
        3 => SettingsSection::Behavior,
        4 => SettingsSection::Appearance,
        5 => SettingsSection::Performance,
        6 => SettingsSection::Accessibility,
        7 => SettingsSection::Startup,
        8 => SettingsSection::Taskbar,
        _ => SettingsSection::AdvancedRecovery,
    }
}

impl From<ConfigError> for SettingsError {
    fn from(value: ConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<ThemeError> for SettingsError {
    fn from(value: ThemeError) -> Self {
        Self::Theme(value)
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Theme(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for SettingsError {}
