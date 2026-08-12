use shell_core::QuickControlKind;

/// Stable identity for one destination in the Settings navigation model.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SettingsSectionId(u64);

impl SettingsSectionId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Stable identity for one control in the active Settings section.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SettingsControlId(u64);

impl SettingsControlId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// One destination in the Settings navigation rail or compact navigation page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsNavigationItem {
    section: SettingsSectionId,
    label: String,
    detail: String,
    icon_glyph: Option<String>,
    enabled: bool,
    modified: bool,
}

impl SettingsNavigationItem {
    #[must_use]
    pub fn new(section: SettingsSectionId, label: &str, detail: &str, enabled: bool) -> Self {
        Self {
            section,
            label: label.to_owned(),
            detail: detail.to_owned(),
            icon_glyph: None,
            enabled,
            modified: false,
        }
    }

    #[must_use]
    pub fn with_icon_glyph(mut self, glyph: &str) -> Self {
        self.icon_glyph = Some(glyph.to_owned());
        self
    }

    #[must_use]
    pub const fn with_modified(mut self, modified: bool) -> Self {
        self.modified = modified;
        self
    }

    #[must_use]
    pub const fn section(&self) -> SettingsSectionId {
        self.section
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
    pub fn icon_glyph(&self) -> Option<&str> {
        self.icon_glyph.as_deref()
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn modified(&self) -> bool {
        self.modified
    }
}

/// Presentation contract for the value affordance of a Settings control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsControlKind {
    Toggle { checked: bool },
    Slider { position: u8 },
    Choice,
    Text,
    Path,
    Shortcut,
    Action,
    ReadOnly,
    Reorder,
}

/// One control row in the active Settings section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsControl {
    id: SettingsControlId,
    label: String,
    detail: String,
    value: String,
    kind: SettingsControlKind,
    enabled: bool,
    modified: bool,
}

impl SettingsControl {
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "settings scene construction keeps all renderer state explicit"
    )]
    pub fn new(
        id: SettingsControlId,
        label: &str,
        detail: &str,
        value: &str,
        kind: SettingsControlKind,
        enabled: bool,
        modified: bool,
    ) -> Self {
        let kind = match kind {
            SettingsControlKind::Slider { position } => SettingsControlKind::Slider {
                position: position.min(100),
            },
            other => other,
        };
        Self {
            id,
            label: label.to_owned(),
            detail: detail.to_owned(),
            value: value.to_owned(),
            kind,
            enabled,
            modified,
        }
    }

    #[must_use]
    pub const fn id(&self) -> SettingsControlId {
        self.id
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
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub const fn kind(&self) -> SettingsControlKind {
        self.kind
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn modified(&self) -> bool {
        self.modified
    }
}

/// Keyboard focus target rendered by Settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsFocus {
    Navigation(SettingsSectionId),
    Control(SettingsControlId),
    Back,
    Apply,
    Cancel,
    Reset,
}

/// Renderer-owned Settings state. It contains no persistence or platform policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsScene {
    title: String,
    navigation: Vec<SettingsNavigationItem>,
    active_section: Option<SettingsSectionId>,
    controls: Vec<SettingsControl>,
    focus: Option<SettingsFocus>,
    dirty: bool,
    status_text: String,
    navigation_scroll_offset: usize,
    control_scroll_offset: usize,
    // Compatibility fields keep the current incremental controller compiling
    // while it migrates to the richer renderer contract above.
    rows: Vec<SettingsRow>,
    focused: Option<usize>,
    quick_controls: Option<Vec<QuickControlSettingsRow>>,
}

impl SettingsScene {
    /// Legacy constructor retained for the existing native Settings controller.
    #[must_use]
    pub fn new(title: &str, rows: Vec<SettingsRow>, focused: Option<usize>) -> Self {
        let navigation = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                SettingsNavigationItem::new(
                    SettingsSectionId::new(index_id(index)),
                    row.label(),
                    row.detail(),
                    row.enabled(),
                )
            })
            .collect::<Vec<_>>();
        let focus = focused
            .and_then(|index| navigation.get(index))
            .map(|item| SettingsFocus::Navigation(item.section()));
        Self {
            title: title.to_owned(),
            navigation,
            active_section: None,
            controls: Vec::new(),
            focus,
            dirty: false,
            status_text: String::new(),
            navigation_scroll_offset: 0,
            control_scroll_offset: 0,
            rows,
            focused,
            quick_controls: None,
        }
    }

    /// Creates the new Settings scene from a typed navigation model.
    #[must_use]
    pub fn from_navigation(title: &str, navigation: Vec<SettingsNavigationItem>) -> Self {
        let rows = navigation
            .iter()
            .map(|item| SettingsRow::new(item.label(), item.detail(), item.enabled()))
            .collect();
        Self {
            title: title.to_owned(),
            navigation,
            active_section: None,
            controls: Vec::new(),
            focus: None,
            dirty: false,
            status_text: String::new(),
            navigation_scroll_offset: 0,
            control_scroll_offset: 0,
            rows,
            focused: None,
            quick_controls: None,
        }
    }

    #[must_use]
    pub const fn with_active_section(mut self, section: Option<SettingsSectionId>) -> Self {
        self.active_section = section;
        self
    }

    #[must_use]
    pub fn with_controls(mut self, controls: Vec<SettingsControl>) -> Self {
        self.controls = controls;
        self
    }

    #[must_use]
    pub fn with_focus(mut self, focus: Option<SettingsFocus>) -> Self {
        self.focus = focus;
        self.focused = match focus {
            Some(SettingsFocus::Navigation(section)) => self
                .navigation
                .iter()
                .position(|item| item.section() == section),
            Some(SettingsFocus::Control(control)) => {
                self.controls.iter().position(|item| item.id() == control)
            }
            Some(
                SettingsFocus::Back
                | SettingsFocus::Apply
                | SettingsFocus::Cancel
                | SettingsFocus::Reset,
            )
            | None => None,
        };
        self
    }

    #[must_use]
    pub const fn with_dirty(mut self, dirty: bool) -> Self {
        self.dirty = dirty;
        self
    }

    #[must_use]
    pub fn with_status_text(mut self, status: &str) -> Self {
        self.status_text = status.to_owned();
        self
    }

    #[must_use]
    pub const fn with_scroll_offsets(
        mut self,
        navigation_offset: usize,
        control_offset: usize,
    ) -> Self {
        self.navigation_scroll_offset = navigation_offset;
        self.control_scroll_offset = control_offset;
        self
    }

    /// Adds compatibility metadata for the legacy quick-control editor.
    ///
    /// Typed controls supplied by the current controller are preserved so their
    /// detail, focus, and per-row modified state remain authoritative.
    #[must_use]
    pub fn with_quick_controls(mut self, rows: Vec<QuickControlSettingsRow>) -> Self {
        if self.controls.is_empty() {
            if self.active_section.is_none() {
                self.active_section = self
                    .navigation
                    .iter()
                    .find(|item| item.label().eq_ignore_ascii_case("Quick Controls"))
                    .or_else(|| self.navigation.first())
                    .map(SettingsNavigationItem::section);
            }
            self.controls = rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    SettingsControl::new(
                        SettingsControlId::new(index_id(index)),
                        row.label(),
                        "Show this control and change its position from the keyboard.",
                        if row.visible() { "Visible" } else { "Hidden" },
                        SettingsControlKind::Toggle {
                            checked: row.visible(),
                        },
                        true,
                        false,
                    )
                })
                .collect();
            if !matches!(
                self.focus,
                Some(
                    SettingsFocus::Back
                        | SettingsFocus::Apply
                        | SettingsFocus::Cancel
                        | SettingsFocus::Reset
                )
            ) {
                self.focus = self
                    .focused
                    .and_then(|index| self.controls.get(index))
                    .map(|control| SettingsFocus::Control(control.id()));
            }
        }
        self.quick_controls = Some(rows);
        self
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn navigation(&self) -> &[SettingsNavigationItem] {
        &self.navigation
    }

    #[must_use]
    pub const fn active_section(&self) -> Option<SettingsSectionId> {
        self.active_section
    }

    #[must_use]
    pub fn active_navigation_item(&self) -> Option<&SettingsNavigationItem> {
        let active = self.active_section?;
        self.navigation.iter().find(|item| item.section() == active)
    }

    #[must_use]
    pub fn navigation_item(&self, section: SettingsSectionId) -> Option<&SettingsNavigationItem> {
        self.navigation
            .iter()
            .find(|item| item.section() == section)
    }

    #[must_use]
    pub fn controls(&self) -> &[SettingsControl] {
        &self.controls
    }

    #[must_use]
    pub fn control(&self, id: SettingsControlId) -> Option<&SettingsControl> {
        self.controls.iter().find(|control| control.id() == id)
    }

    #[must_use]
    pub const fn focus(&self) -> Option<SettingsFocus> {
        self.focus
    }

    #[must_use]
    pub const fn dirty(&self) -> bool {
        self.dirty
    }

    #[must_use]
    pub fn status_text(&self) -> &str {
        &self.status_text
    }

    #[must_use]
    pub const fn navigation_scroll_offset(&self) -> usize {
        self.navigation_scroll_offset
    }

    #[must_use]
    pub const fn control_scroll_offset(&self) -> usize {
        self.control_scroll_offset
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

fn index_id(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

/// Compatibility row used by the current Settings controller.
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

/// Compatibility row used by the current quick-control editor.
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
