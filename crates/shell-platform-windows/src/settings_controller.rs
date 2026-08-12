#![deny(unsafe_code)]

use std::error::Error;
use std::fmt;

use shell_config::{
    AppearanceSettings, ConfigError, DockAlignmentPreference, ShellConfigV1, ThemeError,
    TopbarDensityPreference, export_theme, import_theme,
};
use shell_core::{
    QuickControlKind, ShellEvent, ShellState, TopbarModule, TopbarModuleKind, TransitionError,
    reduce,
};
use shell_renderer::{
    DipPoint, DipRect, QuickControlSettingsRow, SETTINGS_SPLIT_BREAKPOINT, SettingsControl,
    SettingsControlId, SettingsControlKind, SettingsFocus, SettingsHit, SettingsNavigationItem,
    SettingsScene, SettingsSectionId, layout_settings_scene,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsEdit {
    DockItemSize(u16),
    DockSpacing(u16),
    DockAlignment(DockAlignmentPreference),
    DockMagnification(u16),
    DockAutohide(bool),
    Appearance(AppearanceSettings),
    TopbarDensity(TopbarDensityPreference),
    TopbarVisibility {
        module: TopbarModuleKind,
        visible: bool,
    },
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "module reordering is retained for the next Settings control"
        )
    )]
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
    Apply,
    Dismiss,
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
    Dismiss,
    PreviewConfig,
    CommitConfig,
    RevertConfig,
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
    focus_area: SettingsFocusArea,
    surface: DipRect,
    navigation_scroll_offset: usize,
    control_scroll_offset: usize,
    commit_failed: bool,
    external_change_pending: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsFocusArea {
    Navigation,
    Controls,
    Back,
    Reset,
    Cancel,
    Apply,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsFocusTarget {
    Navigation(usize),
    Control(usize),
    Back,
    Reset,
    Cancel,
    Apply,
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
            focus_area: SettingsFocusArea::Navigation,
            surface: DipRect::new(0.0, 0.0, 992.0, 620.0),
            navigation_scroll_offset: 0,
            control_scroll_offset: 0,
            commit_failed: false,
            external_change_pending: false,
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

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.draft != self.committed
    }

    pub fn replace_committed(&mut self, config: ShellConfigV1) {
        self.committed = config.clone();
        self.draft = config.clone();
        self.preview = config;
        self.commit_failed = false;
        self.external_change_pending = false;
        self.normalize_focus();
    }

    /// Reconciles a commit broadcast by another Settings surface.
    ///
    /// A local draft remains authoritative while it is being edited. The new
    /// committed value still becomes the cancellation baseline, preventing a
    /// later Cancel from restoring stale settings over a newer remote commit.
    /// Returns `true` when the local draft and preview were preserved.
    pub fn reconcile_committed(&mut self, config: ShellConfigV1) -> bool {
        if self.is_dirty() && self.draft != config {
            let committed_changed = self.committed != config;
            self.committed = config;
            self.commit_failed = false;
            self.external_change_pending |= committed_changed;
            self.normalize_focus();
            return true;
        }
        self.replace_committed(config);
        false
    }

    pub fn update_surface(&mut self, surface: DipRect) {
        self.surface = surface;
        self.ensure_focus_visible();
    }

    pub fn edit(&mut self, edit: SettingsEdit) -> Result<(), SettingsError> {
        self.draft = apply_edit(&self.draft, edit)?;
        self.update_preview()?;
        self.commit_failed = false;
        Ok(())
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
        self.commit_failed = false;
        self.external_change_pending = false;
        Ok(self.committed.clone())
    }

    pub fn validated_draft(&self) -> Result<ShellConfigV1, SettingsError> {
        self.draft.validate()?;
        Ok(self.draft.clone())
    }

    pub fn cancel(&mut self) {
        self.draft = self.committed.clone();
        self.preview = self.committed.clone();
        self.commit_failed = false;
        self.external_change_pending = false;
        self.normalize_focus();
    }

    pub fn reset(&mut self) {
        let dock_items = self.draft.dock_items().to_vec();
        let dock_layout = self.draft.dock_layout().to_vec();
        self.draft = ShellConfigV1::default()
            .with_dock_items(dock_items)
            .with_dock_layout(dock_layout);
        if self.draft.validate().is_ok() {
            self.preview = self.draft.clone();
        }
        self.commit_failed = false;
    }

    pub fn commit_failed(&mut self) {
        self.commit_failed = true;
    }

    pub fn focus_next(&mut self) {
        self.move_focus(true);
    }

    pub fn focus_previous(&mut self) {
        self.move_focus(false);
    }

    pub fn handle_key(
        &mut self,
        key: SettingsKey,
    ) -> Result<Vec<QueuedSettingsAction>, SettingsError> {
        if matches!(key, SettingsKey::Apply) {
            return Ok(self
                .is_dirty()
                .then_some(QueuedSettingsAction::CommitConfig)
                .into_iter()
                .collect());
        }
        if matches!(key, SettingsKey::Dismiss) {
            self.cancel();
            return Ok(vec![
                QueuedSettingsAction::RevertConfig,
                QueuedSettingsAction::Dismiss,
            ]);
        }
        match key {
            SettingsKey::Next => {
                self.focus_next();
                self.ensure_focus_visible();
                Ok(vec![QueuedSettingsAction::Redraw])
            }
            SettingsKey::Previous => {
                self.focus_previous();
                self.ensure_focus_visible();
                Ok(vec![QueuedSettingsAction::Redraw])
            }
            SettingsKey::Activate => self.activate_focused(),
            SettingsKey::Escape => {
                if let Some(section) = self.active_section.take() {
                    self.focus = section_index(section);
                    self.focus_area = SettingsFocusArea::Navigation;
                    self.ensure_focus_visible();
                    Ok(vec![QueuedSettingsAction::Redraw])
                } else {
                    self.cancel();
                    Ok(vec![
                        QueuedSettingsAction::RevertConfig,
                        QueuedSettingsAction::Dismiss,
                    ])
                }
            }
            SettingsKey::MoveUp | SettingsKey::MoveDown => self.adjust_focused(key),
            SettingsKey::Apply | SettingsKey::Dismiss => unreachable!(),
        }
    }

    #[must_use]
    pub fn scene(&self) -> SettingsScene {
        let active = self.active_section.map(section_id);
        let focus = match self.focus_area {
            SettingsFocusArea::Navigation => Some(SettingsFocus::Navigation(section_id(
                section_for_focus(self.focus),
            ))),
            SettingsFocusArea::Controls => self
                .section_controls()
                .get(self.focus)
                .map(|control| SettingsFocus::Control(control.id())),
            SettingsFocusArea::Back => Some(SettingsFocus::Back),
            SettingsFocusArea::Reset => Some(SettingsFocus::Reset),
            SettingsFocusArea::Cancel => Some(SettingsFocus::Cancel),
            SettingsFocusArea::Apply => Some(SettingsFocus::Apply),
        };
        let scene = SettingsScene::from_navigation("Settings", settings_navigation(self))
            .with_active_section(active)
            .with_controls(self.section_controls())
            .with_focus(focus)
            .with_dirty(self.is_dirty())
            .with_status_text(if self.external_change_pending {
                "Settings changed elsewhere. Review your draft before applying."
            } else if self.commit_failed {
                "Couldn't save changes. Your draft is still available."
            } else if self.is_dirty() {
                "Changes not applied"
            } else {
                "Up to date"
            })
            .with_scroll_offsets(self.navigation_scroll_offset, self.control_scroll_offset);
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
        if self.active_section == Some(SettingsSection::QuickControls)
            && self.focus_area == SettingsFocusArea::Controls
        {
            self.focus = self
                .focus
                .min(self.quick_control_rows().len().saturating_sub(1));
        }
        self.normalize_focus();
        self.ensure_focus_visible();
    }

    pub fn open_section(&mut self, section: SettingsSection) {
        self.active_section = Some(section);
        self.focus = 0;
        self.focus_area = SettingsFocusArea::Controls;
        self.control_scroll_offset = 0;
        self.ensure_focus_visible();
    }

    pub fn handle_pointer(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Result<Vec<QueuedSettingsAction>, SettingsError> {
        self.update_surface(surface);
        let layout = layout_settings_scene(&self.scene(), surface);
        let Some(hit) = layout.hit_test(point) else {
            return Ok(Vec::new());
        };
        match hit {
            SettingsHit::Navigation(id) => {
                let section = section_for_id(id).unwrap_or(SettingsSection::Dock);
                self.open_section(section);
                Ok(vec![QueuedSettingsAction::Redraw])
            }
            SettingsHit::Control(id) => {
                let controls = self.section_controls();
                if let Some(index) = controls.iter().position(|row| row.id() == id) {
                    self.focus = index;
                    self.focus_area = SettingsFocusArea::Controls;
                    if matches!(controls[index].kind(), SettingsControlKind::Slider { .. }) {
                        let position = layout
                            .controls()
                            .iter()
                            .find(|control| control.id() == id)
                            .and_then(|control| {
                                slider_position_from_point(control.bounds(), point)
                            });
                        if let Some(position) = position {
                            self.set_focused_slider_position(position)
                        } else {
                            Ok(vec![QueuedSettingsAction::Redraw])
                        }
                    } else {
                        self.activate_focused()
                    }
                } else {
                    Ok(Vec::new())
                }
            }
            SettingsHit::Back => {
                if let Some(section) = self.active_section.take() {
                    self.focus = section_index(section);
                }
                self.focus_area = SettingsFocusArea::Navigation;
                self.ensure_focus_visible();
                Ok(vec![QueuedSettingsAction::Redraw])
            }
            SettingsHit::Apply => {
                self.focus_area = SettingsFocusArea::Apply;
                Ok(vec![QueuedSettingsAction::CommitConfig])
            }
            SettingsHit::Cancel => {
                self.focus_area = SettingsFocusArea::Cancel;
                self.cancel();
                Ok(vec![
                    QueuedSettingsAction::RevertConfig,
                    QueuedSettingsAction::Redraw,
                ])
            }
            SettingsHit::Reset => {
                self.focus_area = SettingsFocusArea::Reset;
                self.reset();
                Ok(vec![
                    QueuedSettingsAction::PreviewConfig,
                    QueuedSettingsAction::Redraw,
                ])
            }
        }
    }

    pub fn scroll(&mut self, rows: isize) -> Vec<QueuedSettingsAction> {
        let active = self.active_section.is_some();
        let length = if active {
            self.section_controls().len()
        } else {
            SETTINGS_SECTIONS.len()
        };
        let offset = if active {
            &mut self.control_scroll_offset
        } else {
            &mut self.navigation_scroll_offset
        };
        let next = offset
            .saturating_add_signed(rows)
            .min(length.saturating_sub(1));
        if *offset == next {
            return Vec::new();
        }
        *offset = next;
        vec![QueuedSettingsAction::Redraw]
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

    fn activate_focused(&mut self) -> Result<Vec<QueuedSettingsAction>, SettingsError> {
        match self.focus_area {
            SettingsFocusArea::Navigation => {
                let section = section_for_focus(self.focus);
                if !section_enabled(self, section) {
                    return Ok(Vec::new());
                }
                self.open_section(section);
                return Ok(vec![QueuedSettingsAction::Redraw]);
            }
            SettingsFocusArea::Back => {
                if let Some(section) = self.active_section.take() {
                    self.focus = section_index(section);
                }
                self.focus_area = SettingsFocusArea::Navigation;
                self.ensure_focus_visible();
                return Ok(vec![QueuedSettingsAction::Redraw]);
            }
            SettingsFocusArea::Reset => {
                self.reset();
                return Ok(vec![
                    QueuedSettingsAction::PreviewConfig,
                    QueuedSettingsAction::Redraw,
                ]);
            }
            SettingsFocusArea::Cancel => {
                self.cancel();
                return Ok(vec![
                    QueuedSettingsAction::RevertConfig,
                    QueuedSettingsAction::Redraw,
                ]);
            }
            SettingsFocusArea::Apply => {
                return Ok(self
                    .is_dirty()
                    .then_some(QueuedSettingsAction::CommitConfig)
                    .into_iter()
                    .collect());
            }
            SettingsFocusArea::Controls => {}
        }
        let Some(section) = self.active_section else {
            return Ok(Vec::new());
        };
        match section {
            SettingsSection::Dock => match self.focus {
                0 => self.edit(SettingsEdit::DockItemSize(step_u16(
                    self.draft.dock().item_size(),
                    36,
                    72,
                    4,
                    true,
                )))?,
                1 => self.edit(SettingsEdit::DockSpacing(step_u16(
                    self.draft.dock().spacing(),
                    4,
                    20,
                    1,
                    true,
                )))?,
                2 => self.edit(SettingsEdit::DockAlignment(next_alignment(
                    self.draft.dock().alignment(),
                    true,
                )))?,
                3 => self.edit(SettingsEdit::DockMagnification(step_u16(
                    self.draft.dock().magnification(),
                    100,
                    140,
                    5,
                    true,
                )))?,
                4 => self.edit(SettingsEdit::DockAutohide(!self.draft.autohide()))?,
                _ => return Ok(Vec::new()),
            },
            SettingsSection::Topbar => match self.focus {
                0 => self.edit(SettingsEdit::TopbarDensity(
                    match self.draft.topbar().density() {
                        TopbarDensityPreference::Compact => TopbarDensityPreference::Comfortable,
                        TopbarDensityPreference::Comfortable => TopbarDensityPreference::Compact,
                    },
                ))?,
                _ => return Ok(Vec::new()),
            },
            SettingsSection::Modules => {
                let Some(module) = customizable_modules(self.draft.topbar_modules())
                    .get(self.focus)
                    .copied()
                else {
                    return Ok(Vec::new());
                };
                self.edit(SettingsEdit::TopbarVisibility {
                    module: module.kind(),
                    visible: !module.visible(),
                })?;
            }
            SettingsSection::QuickControls => {
                let Some(row) = self.quick_control_rows().get(self.focus).cloned() else {
                    return Ok(Vec::new());
                };
                self.edit(SettingsEdit::QuickControlVisibility {
                    control: row.kind(),
                    visible: !row.visible(),
                })?;
            }
            SettingsSection::Appearance => match self.focus {
                0 => self.edit(SettingsEdit::Appearance(
                    self.draft.appearance().clone().with_opacity(step_u8(
                        self.draft.appearance().opacity(),
                        60,
                        100,
                        5,
                        true,
                    )),
                ))?,
                1 => self.edit(SettingsEdit::Appearance(
                    self.draft.appearance().clone().with_radius(step_u16(
                        self.draft.appearance().radius(),
                        6,
                        24,
                        2,
                        true,
                    )),
                ))?,
                _ => return Ok(Vec::new()),
            },
            SettingsSection::Behavior
            | SettingsSection::Performance
            | SettingsSection::Accessibility
            | SettingsSection::Startup
            | SettingsSection::Taskbar
            | SettingsSection::AdvancedRecovery => return Ok(Vec::new()),
        }
        Ok(vec![
            QueuedSettingsAction::PreviewConfig,
            QueuedSettingsAction::Redraw,
        ])
    }

    fn adjust_focused(
        &mut self,
        key: SettingsKey,
    ) -> Result<Vec<QueuedSettingsAction>, SettingsError> {
        if self.focus_area != SettingsFocusArea::Controls {
            return Ok(Vec::new());
        }
        let increase = key == SettingsKey::MoveDown;
        let Some(section) = self.active_section else {
            return Ok(Vec::new());
        };
        match section {
            SettingsSection::Dock => match self.focus {
                0 => self.edit(SettingsEdit::DockItemSize(step_u16(
                    self.draft.dock().item_size(),
                    36,
                    72,
                    4,
                    increase,
                )))?,
                1 => self.edit(SettingsEdit::DockSpacing(step_u16(
                    self.draft.dock().spacing(),
                    4,
                    20,
                    1,
                    increase,
                )))?,
                2 => self.edit(SettingsEdit::DockAlignment(next_alignment(
                    self.draft.dock().alignment(),
                    increase,
                )))?,
                3 => self.edit(SettingsEdit::DockMagnification(step_u16(
                    self.draft.dock().magnification(),
                    100,
                    140,
                    5,
                    increase,
                )))?,
                _ => return Ok(Vec::new()),
            },
            SettingsSection::Appearance => match self.focus {
                0 => self.edit(SettingsEdit::Appearance(
                    self.draft.appearance().clone().with_opacity(step_u8(
                        self.draft.appearance().opacity(),
                        60,
                        100,
                        5,
                        increase,
                    )),
                ))?,
                1 => self.edit(SettingsEdit::Appearance(
                    self.draft.appearance().clone().with_radius(step_u16(
                        self.draft.appearance().radius(),
                        6,
                        24,
                        2,
                        increase,
                    )),
                ))?,
                _ => return Ok(Vec::new()),
            },
            SettingsSection::QuickControls => {
                let rows = self.quick_control_rows();
                let Some(row) = rows.get(self.focus) else {
                    return Ok(Vec::new());
                };
                let before = if increase {
                    rows.get(self.focus + 2).map(|row| row.kind())
                } else if self.focus > 0 {
                    Some(rows[self.focus - 1].kind())
                } else {
                    return Ok(Vec::new());
                };
                self.edit(SettingsEdit::QuickControlReorder {
                    control: row.kind(),
                    before,
                })?;
                self.focus = if increase {
                    (self.focus + 1).min(rows.len().saturating_sub(1))
                } else {
                    self.focus.saturating_sub(1)
                };
            }
            _ => return Ok(Vec::new()),
        }
        self.ensure_focus_visible();
        Ok(vec![
            QueuedSettingsAction::PreviewConfig,
            QueuedSettingsAction::Redraw,
        ])
    }

    fn section_controls(&self) -> Vec<SettingsControl> {
        let Some(section) = self.active_section else {
            return Vec::new();
        };
        controls_for_section(self, section)
    }

    fn set_focused_slider_position(
        &mut self,
        position: u8,
    ) -> Result<Vec<QueuedSettingsAction>, SettingsError> {
        let Some(section) = self.active_section else {
            return Ok(Vec::new());
        };
        match (section, self.focus) {
            (SettingsSection::Dock, 0) => {
                self.edit(SettingsEdit::DockItemSize(slider_u16(position, 36, 72, 4)))?
            }
            (SettingsSection::Dock, 1) => {
                self.edit(SettingsEdit::DockSpacing(slider_u16(position, 4, 20, 1)))?
            }
            (SettingsSection::Dock, 3) => self.edit(SettingsEdit::DockMagnification(
                slider_u16(position, 100, 140, 5),
            ))?,
            (SettingsSection::Appearance, 0) => {
                self.edit(SettingsEdit::Appearance(
                    self.draft
                        .appearance()
                        .clone()
                        .with_opacity(slider_u8(position, 60, 100, 5)),
                ))?;
            }
            (SettingsSection::Appearance, 1) => {
                self.edit(SettingsEdit::Appearance(
                    self.draft
                        .appearance()
                        .clone()
                        .with_radius(slider_u16(position, 6, 24, 2)),
                ))?;
            }
            _ => return Ok(vec![QueuedSettingsAction::Redraw]),
        }
        Ok(vec![
            QueuedSettingsAction::PreviewConfig,
            QueuedSettingsAction::Redraw,
        ])
    }

    fn focus_targets(&self) -> Vec<SettingsFocusTarget> {
        let mut targets = Vec::new();
        if self.active_section.is_some() {
            if self.surface.width < SETTINGS_SPLIT_BREAKPOINT {
                targets.push(SettingsFocusTarget::Back);
            } else {
                targets.extend(
                    SETTINGS_SECTIONS
                        .iter()
                        .copied()
                        .enumerate()
                        .filter(|(_, section)| section_enabled(self, *section))
                        .map(|(index, _)| SettingsFocusTarget::Navigation(index)),
                );
            }
            targets.extend(
                self.section_controls()
                    .iter()
                    .enumerate()
                    .filter(|(_, control)| control.enabled())
                    .map(|(index, _)| SettingsFocusTarget::Control(index)),
            );
        } else {
            targets.extend(
                SETTINGS_SECTIONS
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(|(_, section)| section_enabled(self, *section))
                    .map(|(index, _)| SettingsFocusTarget::Navigation(index)),
            );
        }
        targets.push(SettingsFocusTarget::Reset);
        if self.is_dirty() {
            targets.push(SettingsFocusTarget::Cancel);
            targets.push(SettingsFocusTarget::Apply);
        }
        targets
    }

    const fn current_focus_target(&self) -> SettingsFocusTarget {
        match self.focus_area {
            SettingsFocusArea::Navigation => SettingsFocusTarget::Navigation(self.focus),
            SettingsFocusArea::Controls => SettingsFocusTarget::Control(self.focus),
            SettingsFocusArea::Back => SettingsFocusTarget::Back,
            SettingsFocusArea::Reset => SettingsFocusTarget::Reset,
            SettingsFocusArea::Cancel => SettingsFocusTarget::Cancel,
            SettingsFocusArea::Apply => SettingsFocusTarget::Apply,
        }
    }

    fn set_focus_target(&mut self, target: SettingsFocusTarget) {
        match target {
            SettingsFocusTarget::Navigation(index) => {
                self.focus = index;
                self.focus_area = SettingsFocusArea::Navigation;
            }
            SettingsFocusTarget::Control(index) => {
                self.focus = index;
                self.focus_area = SettingsFocusArea::Controls;
            }
            SettingsFocusTarget::Back => self.focus_area = SettingsFocusArea::Back,
            SettingsFocusTarget::Reset => self.focus_area = SettingsFocusArea::Reset,
            SettingsFocusTarget::Cancel => self.focus_area = SettingsFocusArea::Cancel,
            SettingsFocusTarget::Apply => self.focus_area = SettingsFocusArea::Apply,
        }
    }

    fn move_focus(&mut self, forward: bool) {
        let targets = self.focus_targets();
        let Some(current) = targets
            .iter()
            .position(|target| *target == self.current_focus_target())
        else {
            if let Some(target) = targets.first().copied() {
                self.set_focus_target(target);
            }
            return;
        };
        let next = if forward {
            (current + 1) % targets.len()
        } else {
            current.checked_sub(1).unwrap_or(targets.len() - 1)
        };
        self.set_focus_target(targets[next]);
    }

    fn normalize_focus(&mut self) {
        let targets = self.focus_targets();
        if targets.contains(&self.current_focus_target()) {
            return;
        }
        if let Some(target) = targets.first().copied() {
            self.set_focus_target(target);
        }
    }

    fn ensure_focus_visible(&mut self) {
        if self.focus_area == SettingsFocusArea::Controls {
            let controls = self.section_controls();
            let Some(control) = controls.get(self.focus) else {
                self.control_scroll_offset = 0;
                return;
            };
            let focused_id = control.id();
            if self.focus < self.control_scroll_offset {
                self.control_scroll_offset = self.focus;
            }
            for _ in 0..controls.len() {
                let layout = layout_settings_scene(&self.scene(), self.surface);
                if layout.controls().iter().any(|item| item.id() == focused_id) {
                    return;
                }
                if self.control_scroll_offset >= self.focus {
                    return;
                }
                self.control_scroll_offset += 1;
            }
            return;
        }

        if self.focus_area != SettingsFocusArea::Navigation {
            return;
        }

        let focused_section = section_id(section_for_focus(self.focus));
        if self.focus < self.navigation_scroll_offset {
            self.navigation_scroll_offset = self.focus;
        }
        for _ in 0..SETTINGS_SECTIONS.len() {
            let layout = layout_settings_scene(&self.scene(), self.surface);
            if layout
                .navigation()
                .iter()
                .any(|item| item.section() == focused_section)
            {
                return;
            }
            if self.navigation_scroll_offset >= self.focus {
                return;
            }
            self.navigation_scroll_offset += 1;
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
        SettingsEdit::DockAlignment(value) => {
            config.with_dock(config.dock().clone().with_alignment(value))
        }
        SettingsEdit::DockMagnification(value) => {
            config.with_dock(config.dock().clone().with_magnification(value))
        }
        SettingsEdit::DockAutohide(value) => config.with_autohide(value),
        SettingsEdit::Appearance(appearance) => config.with_appearance(appearance),
        SettingsEdit::TopbarDensity(value) => {
            config.with_topbar(config.topbar().clone().with_density(value))
        }
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

const SETTINGS_SECTIONS: [SettingsSection; 11] = [
    SettingsSection::Dock,
    SettingsSection::Topbar,
    SettingsSection::Modules,
    SettingsSection::QuickControls,
    SettingsSection::Behavior,
    SettingsSection::Appearance,
    SettingsSection::Performance,
    SettingsSection::Accessibility,
    SettingsSection::Startup,
    SettingsSection::Taskbar,
    SettingsSection::AdvancedRecovery,
];

fn settings_navigation(controller: &SettingsController) -> Vec<SettingsNavigationItem> {
    [
        (
            "Dock",
            "Size, spacing, alignment, magnification, animation, autohide",
            true,
        ),
        ("Top bar", "Density and overflow behavior", true),
        (
            "Modules",
            "Clock, network, volume, power, notifications",
            true,
        ),
        (
            "Quick Controls",
            "Show, hide, and reorder compatible controls",
            !controller.supported_quick_controls.is_empty(),
        ),
        (
            "Behavior",
            "Planned: focus, launch, and confirmations",
            false,
        ),
        (
            "Appearance",
            "Opacity and corner radius with live preview",
            true,
        ),
        (
            "Performance",
            "Planned: quality and battery profiles",
            false,
        ),
        (
            "Accessibility",
            "Managed by Windows and safe-mode capability checks",
            false,
        ),
        (
            "Startup",
            "Unavailable until startup registration is implemented",
            false,
        ),
        (
            "Taskbar",
            "Experimental; stable builds leave Explorer unchanged",
            false,
        ),
        (
            "Advanced/Recovery",
            "Unavailable until the watchdog is real",
            false,
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (label, detail, enabled))| {
        let section = SETTINGS_SECTIONS[index];
        let modified = section_modified(&controller.committed, &controller.draft, section);
        SettingsNavigationItem::new(section_id(section), label, detail, enabled)
            .with_modified(modified)
    })
    .collect()
}

fn controls_for_section(
    controller: &SettingsController,
    section: SettingsSection,
) -> Vec<SettingsControl> {
    let draft = &controller.draft;
    let committed = &controller.committed;
    match section {
        SettingsSection::Dock => vec![
            control(
                1,
                "Icon size",
                "Resting Dock icon size. Use Left/Right for precise adjustment.",
                &format!("{} DIP", draft.dock().item_size()),
                SettingsControlKind::Slider {
                    position: percent_u16(draft.dock().item_size(), 36, 72),
                },
                draft.dock().item_size() != committed.dock().item_size(),
            ),
            control(
                2,
                "Spacing",
                "Space between Dock items.",
                &format!("{} DIP", draft.dock().spacing()),
                SettingsControlKind::Slider {
                    position: percent_u16(draft.dock().spacing(), 4, 20),
                },
                draft.dock().spacing() != committed.dock().spacing(),
            ),
            control(
                3,
                "Alignment",
                "Align the Dock within the current monitor work area.",
                alignment_label(draft.dock().alignment()),
                SettingsControlKind::Choice,
                draft.dock().alignment() != committed.dock().alignment(),
            ),
            control(
                4,
                "Magnification",
                "Maximum pointer-near icon scale.",
                &format!("{}%", draft.dock().magnification()),
                SettingsControlKind::Slider {
                    position: percent_u16(draft.dock().magnification(), 100, 140),
                },
                draft.dock().magnification() != committed.dock().magnification(),
            ),
            control(
                5,
                "Automatically hide the Dock",
                "Reveal from the physical monitor edge.",
                if draft.autohide() { "On" } else { "Off" },
                SettingsControlKind::Toggle {
                    checked: draft.autohide(),
                },
                draft.autohide() != committed.autohide(),
            ),
        ],
        SettingsSection::Topbar => vec![control(
            101,
            "Density",
            "Compact saves horizontal space; narrow monitors compact automatically.",
            density_label(draft.topbar().density()),
            SettingsControlKind::Choice,
            draft.topbar() != committed.topbar(),
        )],
        SettingsSection::Modules => customizable_modules(draft.topbar_modules())
            .into_iter()
            .enumerate()
            .map(|(index, module)| {
                let committed_visible = committed
                    .topbar_modules()
                    .iter()
                    .find(|candidate| candidate.kind() == module.kind())
                    .is_some_and(|candidate| candidate.visible());
                control(
                    200 + index as u64,
                    topbar_module_label(module.kind()),
                    "Show this Windows status module in the Top bar.",
                    if module.visible() {
                        "Visible"
                    } else {
                        "Hidden"
                    },
                    SettingsControlKind::Toggle {
                        checked: module.visible(),
                    },
                    module.visible() != committed_visible,
                )
            })
            .collect(),
        SettingsSection::QuickControls => controller
            .quick_control_rows()
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                control(
                    index as u64,
                    row.label(),
                    "Click to show or hide; Left/Right reorders the control.",
                    if row.visible() { "Visible" } else { "Hidden" },
                    SettingsControlKind::Toggle {
                        checked: row.visible(),
                    },
                    draft.quick_settings() != committed.quick_settings(),
                )
            })
            .collect(),
        SettingsSection::Appearance => vec![
            control(
                401,
                "Surface opacity",
                "Adjust the tint strength while preserving text contrast.",
                &format!("{}%", draft.appearance().opacity()),
                SettingsControlKind::Slider {
                    position: percent_u8(draft.appearance().opacity(), 60, 100),
                },
                draft.appearance().opacity() != committed.appearance().opacity(),
            ),
            control(
                402,
                "Corner radius",
                "Applied consistently to Dock and owned panels.",
                &format!("{} DIP", draft.appearance().radius()),
                SettingsControlKind::Slider {
                    position: percent_u16(draft.appearance().radius(), 6, 24),
                },
                draft.appearance().radius() != committed.appearance().radius(),
            ),
        ],
        SettingsSection::Behavior
        | SettingsSection::Performance
        | SettingsSection::Accessibility
        | SettingsSection::Startup
        | SettingsSection::Taskbar
        | SettingsSection::AdvancedRecovery => Vec::new(),
    }
}

fn control(
    id: u64,
    label: &str,
    detail: &str,
    value: &str,
    kind: SettingsControlKind,
    modified: bool,
) -> SettingsControl {
    SettingsControl::new(
        SettingsControlId::new(id),
        label,
        detail,
        value,
        kind,
        true,
        modified,
    )
}

fn customizable_modules(modules: &[TopbarModule]) -> Vec<TopbarModule> {
    modules
        .iter()
        .copied()
        .filter(|module| module.kind().is_customizable())
        .collect()
}

const fn section_id(section: SettingsSection) -> SettingsSectionId {
    SettingsSectionId::new(section_index(section) as u64 + 1)
}

const fn section_index(section: SettingsSection) -> usize {
    match section {
        SettingsSection::Dock => 0,
        SettingsSection::Topbar => 1,
        SettingsSection::Modules => 2,
        SettingsSection::QuickControls => 3,
        SettingsSection::Behavior => 4,
        SettingsSection::Appearance => 5,
        SettingsSection::Performance => 6,
        SettingsSection::Accessibility => 7,
        SettingsSection::Startup => 8,
        SettingsSection::Taskbar => 9,
        SettingsSection::AdvancedRecovery => 10,
    }
}

fn section_for_id(id: SettingsSectionId) -> Option<SettingsSection> {
    let index = usize::try_from(id.value().checked_sub(1)?).ok()?;
    SETTINGS_SECTIONS.get(index).copied()
}

fn section_modified(
    committed: &ShellConfigV1,
    draft: &ShellConfigV1,
    section: SettingsSection,
) -> bool {
    match section {
        SettingsSection::Dock => {
            committed.dock() != draft.dock() || committed.autohide() != draft.autohide()
        }
        SettingsSection::Topbar => committed.topbar() != draft.topbar(),
        SettingsSection::Modules => committed.topbar_modules() != draft.topbar_modules(),
        SettingsSection::QuickControls => committed.quick_settings() != draft.quick_settings(),
        SettingsSection::Appearance => committed.appearance() != draft.appearance(),
        SettingsSection::Behavior
        | SettingsSection::Performance
        | SettingsSection::Accessibility
        | SettingsSection::Startup
        | SettingsSection::Taskbar
        | SettingsSection::AdvancedRecovery => false,
    }
}

fn section_enabled(controller: &SettingsController, section: SettingsSection) -> bool {
    matches!(
        section,
        SettingsSection::Dock
            | SettingsSection::Topbar
            | SettingsSection::Modules
            | SettingsSection::Appearance
    ) || (section == SettingsSection::QuickControls
        && !controller.supported_quick_controls.is_empty())
}

fn step_u16(value: u16, min: u16, max: u16, step: u16, increase: bool) -> u16 {
    if increase {
        value.saturating_add(step).min(max)
    } else {
        value.saturating_sub(step).max(min)
    }
}

fn step_u8(value: u8, min: u8, max: u8, step: u8, increase: bool) -> u8 {
    if increase {
        value.saturating_add(step).min(max)
    } else {
        value.saturating_sub(step).max(min)
    }
}

fn slider_position_from_point(bounds: DipRect, point: DipPoint) -> Option<u8> {
    const CONTROL_VALUE_WIDTH: f32 = 164.0;
    let row_width = (bounds.width - 24.0).max(0.0);
    let value_width = CONTROL_VALUE_WIDTH.min((row_width * 0.42).max(112.0));
    let track_left = bounds.x + 12.0 + row_width - value_width + 8.0;
    let track_width = (value_width - 16.0).max(1.0);
    let track_right = track_left + track_width;
    if point.x < track_left || point.x > track_right {
        return None;
    }
    let ratio = ((point.x - track_left) / track_width).clamp(0.0, 1.0);
    Some((ratio * 100.0).round() as u8)
}

fn slider_u16(position: u8, min: u16, max: u16, step: u16) -> u16 {
    let span = u32::from(max.saturating_sub(min));
    let raw = (span * u32::from(position.min(100)) + 50) / 100;
    let step = u32::from(step.max(1));
    let snapped = ((raw + step / 2) / step) * step;
    min.saturating_add(u16::try_from(snapped).unwrap_or(u16::MAX))
        .min(max)
}

fn slider_u8(position: u8, min: u8, max: u8, step: u8) -> u8 {
    u8::try_from(slider_u16(
        position,
        u16::from(min),
        u16::from(max),
        u16::from(step),
    ))
    .unwrap_or(max)
}

const fn next_alignment(value: DockAlignmentPreference, forward: bool) -> DockAlignmentPreference {
    match (value, forward) {
        (DockAlignmentPreference::Left, true) | (DockAlignmentPreference::Right, false) => {
            DockAlignmentPreference::Center
        }
        (DockAlignmentPreference::Center, true) => DockAlignmentPreference::Right,
        (DockAlignmentPreference::Center, false) => DockAlignmentPreference::Left,
        (DockAlignmentPreference::Right, true) => DockAlignmentPreference::Left,
        (DockAlignmentPreference::Left, false) => DockAlignmentPreference::Right,
    }
}

const fn percent_u16(value: u16, min: u16, max: u16) -> u8 {
    (((value.saturating_sub(min)) as u32 * 100) / (max - min) as u32) as u8
}

const fn percent_u8(value: u8, min: u8, max: u8) -> u8 {
    (((value.saturating_sub(min)) as u16 * 100) / (max - min) as u16) as u8
}

const fn alignment_label(value: DockAlignmentPreference) -> &'static str {
    match value {
        DockAlignmentPreference::Left => "Left",
        DockAlignmentPreference::Center => "Center",
        DockAlignmentPreference::Right => "Right",
    }
}

const fn density_label(value: TopbarDensityPreference) -> &'static str {
    match value {
        TopbarDensityPreference::Compact => "Compact",
        TopbarDensityPreference::Comfortable => "Comfortable",
    }
}

const fn topbar_module_label(kind: TopbarModuleKind) -> &'static str {
    match kind {
        TopbarModuleKind::Clock => "Clock and calendar",
        TopbarModuleKind::Network => "Network",
        TopbarModuleKind::Volume => "Sound",
        TopbarModuleKind::Power => "Power and battery",
        TopbarModuleKind::Notifications => "Notifications",
        TopbarModuleKind::SystemMenu => "System menu",
        TopbarModuleKind::AppIdentity => "Active app",
        TopbarModuleKind::Search => "Search",
        TopbarModuleKind::BackgroundApps => "Background apps",
    }
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
