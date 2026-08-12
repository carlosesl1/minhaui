use crate::{
    DipPoint, DipRect, SettingsControlId, SettingsFocus, SettingsScene, SettingsSectionId,
};

pub const SETTINGS_SPLIT_BREAKPOINT: f32 = 720.0;
pub const SETTINGS_RAIL_WIDTH: f32 = 232.0;
pub const SETTINGS_CONTENT_MAX_WIDTH: f32 = 760.0;

const STANDARD_GUTTER: f32 = 24.0;
const COMPACT_GUTTER: f32 = 16.0;
const HEADER_TOP: f32 = 12.0;
const HEADER_HEIGHT: f32 = 40.0;
const BODY_TOP: f32 = 64.0;
const FOOTER_HEIGHT: f32 = 56.0;
const BODY_FOOTER_GAP: f32 = 8.0;
const RAIL_CONTENT_GAP: f32 = 16.0;
const NAVIGATION_INSET: f32 = 8.0;
const NAVIGATION_ROW_HEIGHT: f32 = 40.0;
const NAVIGATION_ROW_GAP: f32 = 4.0;
const CONTENT_HEADING_HEIGHT: f32 = 56.0;
const CONTROL_ROW_HEIGHT: f32 = 64.0;
const CONTROL_ROW_GAP: f32 = 8.0;
const ACTION_HEIGHT: f32 = 36.0;
const ACTION_GAP: f32 = 8.0;
const RESET_WIDTH: f32 = 84.0;
const SECONDARY_ACTION_WIDTH: f32 = 84.0;
const PRIMARY_ACTION_WIDTH: f32 = 92.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsLayoutMode {
    Split,
    NavigationOnly,
    ContentOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsHit {
    Navigation(SettingsSectionId),
    Control(SettingsControlId),
    Back,
    Apply,
    Cancel,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SettingsLaidOutNavigation {
    section: SettingsSectionId,
    bounds: DipRect,
    active: bool,
    focused: bool,
    enabled: bool,
    modified: bool,
}

impl SettingsLaidOutNavigation {
    #[must_use]
    pub const fn section(self) -> SettingsSectionId {
        self.section
    }

    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }

    #[must_use]
    pub const fn active(self) -> bool {
        self.active
    }

    #[must_use]
    pub const fn focused(self) -> bool {
        self.focused
    }

    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn modified(self) -> bool {
        self.modified
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SettingsLaidOutControl {
    id: SettingsControlId,
    bounds: DipRect,
    focused: bool,
    enabled: bool,
    modified: bool,
}

impl SettingsLaidOutControl {
    #[must_use]
    pub const fn id(self) -> SettingsControlId {
        self.id
    }

    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }

    #[must_use]
    pub const fn focused(self) -> bool {
        self.focused
    }

    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn modified(self) -> bool {
        self.modified
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsLayout {
    mode: SettingsLayoutMode,
    title_bounds: DipRect,
    navigation_bounds: Option<DipRect>,
    content_bounds: Option<DipRect>,
    content_heading_bounds: Option<DipRect>,
    status_bounds: Option<DipRect>,
    navigation: Vec<SettingsLaidOutNavigation>,
    controls: Vec<SettingsLaidOutControl>,
    back_bounds: Option<DipRect>,
    apply_bounds: Option<DipRect>,
    cancel_bounds: Option<DipRect>,
    reset_bounds: Option<DipRect>,
    pending_changes: bool,
}

impl SettingsLayout {
    #[must_use]
    pub const fn mode(&self) -> SettingsLayoutMode {
        self.mode
    }

    #[must_use]
    pub const fn title_bounds(&self) -> DipRect {
        self.title_bounds
    }

    #[must_use]
    pub const fn navigation_bounds(&self) -> Option<DipRect> {
        self.navigation_bounds
    }

    #[must_use]
    pub const fn content_bounds(&self) -> Option<DipRect> {
        self.content_bounds
    }

    #[must_use]
    pub const fn content_heading_bounds(&self) -> Option<DipRect> {
        self.content_heading_bounds
    }

    #[must_use]
    pub const fn status_bounds(&self) -> Option<DipRect> {
        self.status_bounds
    }

    #[must_use]
    pub fn navigation(&self) -> &[SettingsLaidOutNavigation] {
        &self.navigation
    }

    #[must_use]
    pub fn controls(&self) -> &[SettingsLaidOutControl] {
        &self.controls
    }

    #[must_use]
    pub const fn back_bounds(&self) -> Option<DipRect> {
        self.back_bounds
    }

    #[must_use]
    pub const fn apply_bounds(&self) -> Option<DipRect> {
        self.apply_bounds
    }

    #[must_use]
    pub const fn cancel_bounds(&self) -> Option<DipRect> {
        self.cancel_bounds
    }

    #[must_use]
    pub const fn reset_bounds(&self) -> Option<DipRect> {
        self.reset_bounds
    }

    #[must_use]
    pub fn hit_test(&self, point: DipPoint) -> Option<SettingsHit> {
        if self
            .back_bounds
            .is_some_and(|bounds| contains(bounds, point))
        {
            return Some(SettingsHit::Back);
        }
        if self.pending_changes
            && self
                .apply_bounds
                .is_some_and(|bounds| contains(bounds, point))
        {
            return Some(SettingsHit::Apply);
        }
        if self.pending_changes
            && self
                .cancel_bounds
                .is_some_and(|bounds| contains(bounds, point))
        {
            return Some(SettingsHit::Cancel);
        }
        if self
            .reset_bounds
            .is_some_and(|bounds| contains(bounds, point))
        {
            return Some(SettingsHit::Reset);
        }
        if let Some(control) = self
            .controls
            .iter()
            .find(|control| control.enabled && contains(control.bounds, point))
        {
            return Some(SettingsHit::Control(control.id));
        }
        self.navigation
            .iter()
            .find(|item| item.enabled && contains(item.bounds, point))
            .map(|item| SettingsHit::Navigation(item.section))
    }
}

#[must_use]
pub fn layout_settings_scene(scene: &SettingsScene, surface: DipRect) -> SettingsLayout {
    let mode = settings_layout_mode(surface.width, scene.active_section().is_some());
    let gutter = if mode == SettingsLayoutMode::Split {
        STANDARD_GUTTER
    } else {
        COMPACT_GUTTER
    };
    let surface_right = surface.x + surface.width.max(0.0);
    let surface_bottom = surface.y + surface.height.max(0.0);
    let back_bounds = (mode == SettingsLayoutMode::ContentOnly).then_some(DipRect::new(
        surface.x + gutter,
        surface.y + HEADER_TOP + 2.0,
        36.0,
        36.0,
    ));
    let title_left = back_bounds.map_or(surface.x + gutter, |back| back.x + back.width + 12.0);
    let title_bounds = DipRect::new(
        title_left,
        surface.y + HEADER_TOP,
        (surface_right - gutter - title_left).max(0.0),
        HEADER_HEIGHT,
    );

    let body_y = (surface.y + BODY_TOP).min(surface_bottom);
    let footer_y = (surface_bottom - gutter - FOOTER_HEIGHT)
        .max(body_y)
        .min(surface_bottom);
    let body_height = (footer_y - BODY_FOOTER_GAP - body_y).max(0.0);
    let usable_width = (surface.width - gutter * 2.0).max(0.0);
    let (navigation_bounds, content_bounds) = match mode {
        SettingsLayoutMode::Split => {
            let rail_width = SETTINGS_RAIL_WIDTH.min(usable_width);
            let navigation = DipRect::new(surface.x + gutter, body_y, rail_width, body_height);
            let content_x = navigation.x + navigation.width + RAIL_CONTENT_GAP;
            let content_width =
                (surface_right - gutter - content_x).clamp(0.0, SETTINGS_CONTENT_MAX_WIDTH);
            (
                Some(navigation),
                Some(DipRect::new(content_x, body_y, content_width, body_height)),
            )
        }
        SettingsLayoutMode::NavigationOnly => (
            Some(DipRect::new(
                surface.x + gutter,
                body_y,
                usable_width,
                body_height,
            )),
            None,
        ),
        SettingsLayoutMode::ContentOnly => (
            None,
            Some(DipRect::new(
                surface.x + gutter,
                body_y,
                usable_width,
                body_height,
            )),
        ),
    };

    let navigation = layout_navigation(scene, navigation_bounds);
    let content_heading_bounds = content_bounds.and_then(|content| {
        scene.active_section().map(|_| {
            DipRect::new(
                content.x,
                content.y,
                content.width,
                CONTENT_HEADING_HEIGHT.min(content.height),
            )
        })
    });
    let controls = layout_controls(scene, content_bounds, content_heading_bounds);
    let action_area = content_bounds.or(navigation_bounds);
    let (reset_bounds, cancel_bounds, apply_bounds, status_bounds) =
        layout_actions(action_area, footer_y, surface_bottom);

    SettingsLayout {
        mode,
        title_bounds,
        navigation_bounds,
        content_bounds,
        content_heading_bounds,
        status_bounds,
        navigation,
        controls,
        back_bounds,
        apply_bounds,
        cancel_bounds,
        reset_bounds,
        pending_changes: scene.dirty(),
    }
}

const fn settings_layout_mode(width: f32, has_active_section: bool) -> SettingsLayoutMode {
    if width >= SETTINGS_SPLIT_BREAKPOINT {
        SettingsLayoutMode::Split
    } else if has_active_section {
        SettingsLayoutMode::ContentOnly
    } else {
        SettingsLayoutMode::NavigationOnly
    }
}

fn layout_navigation(
    scene: &SettingsScene,
    bounds: Option<DipRect>,
) -> Vec<SettingsLaidOutNavigation> {
    let Some(bounds) = bounds else {
        return Vec::new();
    };
    let mut y = bounds.y + NAVIGATION_INSET;
    let bottom = bounds.y + bounds.height - NAVIGATION_INSET;
    let width = (bounds.width - NAVIGATION_INSET * 2.0).max(0.0);
    let mut laid_out = Vec::new();
    for item in scene
        .navigation()
        .iter()
        .skip(scene.navigation_scroll_offset())
    {
        if y + NAVIGATION_ROW_HEIGHT > bottom {
            break;
        }
        laid_out.push(SettingsLaidOutNavigation {
            section: item.section(),
            bounds: DipRect::new(bounds.x + NAVIGATION_INSET, y, width, NAVIGATION_ROW_HEIGHT),
            active: scene.active_section() == Some(item.section()),
            focused: scene.focus() == Some(SettingsFocus::Navigation(item.section())),
            enabled: item.enabled(),
            modified: item.modified(),
        });
        y += NAVIGATION_ROW_HEIGHT + NAVIGATION_ROW_GAP;
    }
    laid_out
}

fn layout_controls(
    scene: &SettingsScene,
    content: Option<DipRect>,
    heading: Option<DipRect>,
) -> Vec<SettingsLaidOutControl> {
    let Some(content) = content else {
        return Vec::new();
    };
    if scene.active_section().is_none() {
        return Vec::new();
    }
    let mut y = heading.map_or(content.y, |bounds| bounds.y + bounds.height);
    let bottom = content.y + content.height;
    let mut laid_out = Vec::new();
    for control in scene.controls().iter().skip(scene.control_scroll_offset()) {
        if y + CONTROL_ROW_HEIGHT > bottom {
            break;
        }
        laid_out.push(SettingsLaidOutControl {
            id: control.id(),
            bounds: DipRect::new(content.x, y, content.width, CONTROL_ROW_HEIGHT),
            focused: scene.focus() == Some(SettingsFocus::Control(control.id())),
            enabled: control.enabled(),
            modified: control.modified(),
        });
        y += CONTROL_ROW_HEIGHT + CONTROL_ROW_GAP;
    }
    laid_out
}

fn layout_actions(
    area: Option<DipRect>,
    footer_y: f32,
    surface_bottom: f32,
) -> (
    Option<DipRect>,
    Option<DipRect>,
    Option<DipRect>,
    Option<DipRect>,
) {
    let Some(area) = area else {
        return (None, None, None, None);
    };
    let action_y = (footer_y + (FOOTER_HEIGHT - ACTION_HEIGHT) / 2.0)
        .min((surface_bottom - ACTION_HEIGHT).max(footer_y));
    let right = area.x + area.width;
    let apply = DipRect::new(
        (right - PRIMARY_ACTION_WIDTH).max(area.x),
        action_y,
        PRIMARY_ACTION_WIDTH.min(area.width),
        ACTION_HEIGHT,
    );
    let cancel_right = apply.x - ACTION_GAP;
    let cancel = DipRect::new(
        (cancel_right - SECONDARY_ACTION_WIDTH).max(area.x),
        action_y,
        SECONDARY_ACTION_WIDTH.min((cancel_right - area.x).max(0.0)),
        ACTION_HEIGHT,
    );
    let reset = DipRect::new(area.x, action_y, RESET_WIDTH.min(area.width), ACTION_HEIGHT);
    let status_left = reset.x + reset.width + ACTION_GAP;
    let status_right = cancel.x - ACTION_GAP;
    let status = (status_right > status_left).then_some(DipRect::new(
        status_left,
        action_y,
        status_right - status_left,
        ACTION_HEIGHT,
    ));
    (Some(reset), Some(cancel), Some(apply), status)
}

fn contains(bounds: DipRect, point: DipPoint) -> bool {
    point.x >= bounds.x
        && point.y >= bounds.y
        && point.x < bounds.x + bounds.width
        && point.y < bounds.y + bounds.height
}
