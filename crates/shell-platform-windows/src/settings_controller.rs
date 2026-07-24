#![deny(unsafe_code)]

use std::error::Error;
use std::fmt;

use shell_config::{
    AppearanceSettings, ConfigError, ShellConfigV1, ThemeError, export_theme, import_theme,
};
use shell_core::{
    QuickControlKind, ShellEvent, ShellState, TopbarModule, TopbarModuleKind, TransitionError,
    reduce,
};
use shell_renderer::{QuickControlSettingsRow, SettingsRow, SettingsScene};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "settings editing is not yet wired to the native form"
    )
)]
pub enum SettingsEdit {
    DockItemSize(u16),
    DockSpacing(u16),
    Appearance(AppearanceSettings),
    TopbarVisibility {
        module: TopbarModuleKind,
        visible: bool,
    },
    TopbarReorder {
        module: TopbarModuleKind,
        before: Option<TopbarModuleKind>,
    },
    QuickControlVisibility {
        control: QuickControlKind,
        visible: bool,
    },
    QuickControlReorder {
        control: QuickControlKind,
        before: Option<QuickControlKind>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsKey {
    Next,
    Previous,
    Activate,
    Escape,
    MoveUp,
    MoveDown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsSection {
    Dock,
    Topbar,
    Modules,
    QuickControls,
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
    ApplyQuickSettings,
}

#[derive(Debug)]
pub enum SettingsError {
    Config(ConfigError),
    Theme(ThemeError),
    Transition(TransitionError),
}

pub struct SettingsController {
    committed: ShellConfigV1,
    draft: ShellConfigV1,
    preview: ShellConfigV1,
    focus: usize,
    active_section: Option<SettingsSection>,
    supported_quick_controls: Vec<QuickControlKind>,
}

#[cfg_attr(
    not(test),
    allow(dead_code, reason = "settings editing is an internal incremental seam")
)]
impl SettingsController {
    #[must_use]
    pub fn new(config: ShellConfigV1) -> Self {
        Self {
            committed: config.clone(),
            draft: config.clone(),
            preview: config,
            focus: 0,
            active_section: None,
            supported_quick_controls: Vec::new(),
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
        self.draft = apply_edit(&self.draft, edit)?;
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
        let length = if self.active_section == Some(SettingsSection::QuickControls) {
            self.quick_control_rows().len()
        } else {
            settings_rows().len()
        };
        self.focus = (self.focus + 1).min(length.saturating_sub(1));
    }

    pub fn focus_previous(&mut self) {
        self.focus = self.focus.saturating_sub(1);
    }

    pub fn handle_key(
        &mut self,
        key: SettingsKey,
    ) -> Result<Vec<QueuedSettingsAction>, SettingsError> {
        if self.active_section == Some(SettingsSection::QuickControls) {
            return self.handle_quick_controls_key(key);
        }
        match key {
            SettingsKey::Next => {
                self.focus_next();
                Ok(vec![QueuedSettingsAction::Redraw])
            }
            SettingsKey::Previous => {
                self.focus_previous();
                Ok(vec![QueuedSettingsAction::Redraw])
            }
            SettingsKey::Activate => {
                let section = section_for_focus(self.focus);
                if section == SettingsSection::QuickControls {
                    self.open_section(section);
                    Ok(vec![QueuedSettingsAction::Redraw])
                } else {
                    Ok(vec![QueuedSettingsAction::OpenSection(section)])
                }
            }
            SettingsKey::Escape => {
                self.cancel();
                Ok(vec![QueuedSettingsAction::Dismiss])
            }
            SettingsKey::MoveUp | SettingsKey::MoveDown => Ok(Vec::new()),
        }
    }

    #[must_use]
    pub fn scene(&self) -> SettingsScene {
        let scene = SettingsScene::new("Settings", settings_rows(), Some(self.focus));
        if self.active_section == Some(SettingsSection::QuickControls) {
            scene.with_quick_controls(self.quick_control_rows())
        } else {
            scene
        }
    }

    pub fn set_supported_quick_controls(
        &mut self,
        controls: impl IntoIterator<Item = QuickControlKind>,
    ) {
        self.supported_quick_controls = controls.into_iter().collect();
        self.focus = self
            .focus
            .min(self.quick_control_rows().len().saturating_sub(1));
    }

    pub fn open_section(&mut self, section: SettingsSection) {
        self.active_section = Some(section);
        self.focus = 0;
    }

    fn quick_control_rows(&self) -> Vec<QuickControlSettingsRow> {
        let filtered = self
            .draft
            .quick_settings()
            .controls()
            .iter()
            .filter(|placement| self.supported_quick_controls.contains(&placement.kind()))
            .copied()
            .collect::<Vec<_>>();
        let last = filtered.len().saturating_sub(1);
        filtered
            .into_iter()
            .enumerate()
            .map(|(index, placement)| {
                QuickControlSettingsRow::new(
                    placement.kind(),
                    quick_control_label(placement.kind()),
                    placement.visible(),
                    index > 0,
                    index < last,
                )
            })
            .collect()
    }

    fn handle_quick_controls_key(
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
            SettingsKey::Escape => {
                self.active_section = None;
                self.focus = 3;
                Ok(vec![QueuedSettingsAction::Redraw])
            }
            SettingsKey::Activate => {
                let Some(row) = self.quick_control_rows().get(self.focus).cloned() else {
                    return Ok(Vec::new());
                };
                self.edit(SettingsEdit::QuickControlVisibility {
                    control: row.kind(),
                    visible: !row.visible(),
                })?;
                Ok(vec![
                    QueuedSettingsAction::ApplyQuickSettings,
                    QueuedSettingsAction::Redraw,
                ])
            }
            SettingsKey::MoveUp | SettingsKey::MoveDown => {
                let rows = self.quick_control_rows();
                let Some(row) = rows.get(self.focus) else {
                    return Ok(Vec::new());
                };
                let target = match key {
                    SettingsKey::MoveUp if self.focus > 0 => Some(rows[self.focus - 1].kind()),
                    SettingsKey::MoveDown if self.focus + 1 < rows.len() => {
                        rows.get(self.focus + 2).map(QuickControlSettingsRow::kind)
                    }
                    _ => return Ok(Vec::new()),
                };
                let control = row.kind();
                self.edit(SettingsEdit::QuickControlReorder {
                    control,
                    before: target,
                })?;
                self.focus = if key == SettingsKey::MoveUp {
                    self.focus.saturating_sub(1)
                } else {
                    (self.focus + 1).min(rows.len().saturating_sub(1))
                };
                Ok(vec![
                    QueuedSettingsAction::ApplyQuickSettings,
                    QueuedSettingsAction::Redraw,
                ])
            }
        }
    }

    fn update_preview(&mut self) -> Result<(), SettingsError> {
        self.draft.validate()?;
        self.preview = self.draft.clone();
        Ok(())
    }
}

fn apply_edit(config: &ShellConfigV1, edit: SettingsEdit) -> Result<ShellConfigV1, SettingsError> {
    let edited = match edit {
        SettingsEdit::DockItemSize(value) => {
            config.with_dock(config.dock().clone().with_item_size(value))
        }
        SettingsEdit::DockSpacing(value) => {
            config.with_dock(config.dock().clone().with_spacing(value))
        }
        SettingsEdit::Appearance(appearance) => config.with_appearance(appearance),
        SettingsEdit::TopbarVisibility { module, visible } => {
            apply_topbar_event(config, ShellEvent::SetTopbarVisibility { module, visible })?
        }
        SettingsEdit::TopbarReorder { module, before } => {
            apply_topbar_event(config, ShellEvent::ReorderTopbarModule { module, before })?
        }
        SettingsEdit::QuickControlVisibility { control, visible } => {
            config.with_quick_settings(config.quick_settings().with_visibility(control, visible))
        }
        SettingsEdit::QuickControlReorder { control, before } => {
            config.with_quick_settings(config.quick_settings().reordered(control, before))
        }
    };
    Ok(edited)
}

fn apply_topbar_event(
    config: &ShellConfigV1,
    event: ShellEvent,
) -> Result<ShellConfigV1, SettingsError> {
    let mut modules = vec![
        TopbarModule::new(TopbarModuleKind::SystemMenu, true),
        TopbarModule::new(TopbarModuleKind::AppIdentity, true),
        TopbarModule::new(TopbarModuleKind::Search, true),
    ];
    let mut background_apps_inserted = false;
    for module in config
        .topbar_modules()
        .iter()
        .copied()
        .filter(|module| module.kind() != TopbarModuleKind::SystemMenu)
    {
        if module.kind() == TopbarModuleKind::Clock && !background_apps_inserted {
            modules.push(TopbarModule::new(TopbarModuleKind::BackgroundApps, true));
            background_apps_inserted = true;
        }
        modules.push(module);
    }
    if !background_apps_inserted {
        modules.push(TopbarModule::new(TopbarModuleKind::BackgroundApps, true));
    }
    let state = ShellState::default().with_topbar_modules(modules);
    let transition = reduce(&state, event)?;
    let persisted = transition
        .state
        .topbar_modules()
        .iter()
        .copied()
        .filter(|module| module.kind().is_v1_persisted())
        .collect();
    Ok(config.clone().with_topbar_modules(persisted))
}

fn settings_rows() -> Vec<SettingsRow> {
    [
        (
            "Dock",
            "Size, spacing, alignment, magnification, animation, autohide",
        ),
        ("Topbar", "Density, order, and module visibility"),
        ("Modules", "Clock, network, volume, power, notifications"),
        (
            "Quick Controls",
            "Show, hide, and reorder compatible controls",
        ),
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
        3 => SettingsSection::QuickControls,
        4 => SettingsSection::Behavior,
        5 => SettingsSection::Appearance,
        6 => SettingsSection::Performance,
        7 => SettingsSection::Accessibility,
        8 => SettingsSection::Startup,
        9 => SettingsSection::Taskbar,
        _ => SettingsSection::AdvancedRecovery,
    }
}

const fn quick_control_label(kind: QuickControlKind) -> &'static str {
    match kind {
        QuickControlKind::Wifi => "Wi-Fi",
        QuickControlKind::Bluetooth => "Bluetooth",
        QuickControlKind::NearbySharing => "Nearby sharing",
        QuickControlKind::Focus => "Do not disturb",
        QuickControlKind::Multitasking => "Snap windows",
        QuickControlKind::Projection => "Projection",
        QuickControlKind::Brightness => "Brightness",
        QuickControlKind::DarkMode => "Dark mode",
        QuickControlKind::NightLight => "Night light",
        QuickControlKind::Volume => "Sound",
        QuickControlKind::Battery => "Battery",
        QuickControlKind::EnergySaver => "Energy saver",
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

impl From<TransitionError> for SettingsError {
    fn from(value: TransitionError) -> Self {
        Self::Transition(value)
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Theme(error) => write!(formatter, "{error}"),
            Self::Transition(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for SettingsError {}
