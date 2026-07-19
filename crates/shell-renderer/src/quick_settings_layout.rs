use shell_core::QuickControlKind;

use crate::{
    DipPoint, DipRect, QuickSettingsChoiceId, QuickSettingsHit, QuickSettingsMediaAction,
    QuickSettingsMediaSessionId, QuickSettingsScene,
};

pub const QUICK_SETTINGS_WIDTH: f32 = 368.0;
pub const QUICK_SETTINGS_BODY_TOP: f32 = 8.0;
const OUTER: f32 = 12.0;
const GAP: f32 = 8.0;
const TILE_HEIGHT: f32 = 56.0;
const SECTION_GAP: f32 = 8.0;
const SECTION_PADDING: f32 = 10.0;
const SECTION_HEADER_HEIGHT: f32 = 28.0;
const SLIDER_HEIGHT: f32 = 36.0;
const ACTION_HEIGHT: f32 = 48.0;
const FOOTER_HEIGHT: f32 = 36.0;
const SUBMENU_HEADER_HEIGHT: f32 = 48.0;
const SUBMENU_CHOICE_HEIGHT: f32 = 58.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickSettingsLaidOutTile {
    kind: QuickControlKind,
    bounds: DipRect,
}
impl QuickSettingsLaidOutTile {
    #[must_use]
    pub const fn kind(self) -> QuickControlKind {
        self.kind
    }
    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickSettingsLaidOutSlider {
    kind: QuickControlKind,
    bounds: DipRect,
    track: DipRect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickSettingsLaidOutChoice {
    id: QuickSettingsChoiceId,
    bounds: DipRect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickSettingsLaidOutMedia {
    bounds: DipRect,
    artwork: DipRect,
    previous: DipRect,
    toggle: DipRect,
    next: DipRect,
}

impl QuickSettingsLaidOutMedia {
    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }
    #[must_use]
    pub const fn artwork(self) -> DipRect {
        self.artwork
    }
    #[must_use]
    pub const fn previous(self) -> DipRect {
        self.previous
    }
    #[must_use]
    pub const fn toggle(self) -> DipRect {
        self.toggle
    }
    #[must_use]
    pub const fn next(self) -> DipRect {
        self.next
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickSettingsLaidOutMediaChoice {
    id: QuickSettingsMediaSessionId,
    bounds: DipRect,
}

impl QuickSettingsLaidOutMediaChoice {
    #[must_use]
    pub const fn id(self) -> QuickSettingsMediaSessionId {
        self.id
    }
    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }
}
impl QuickSettingsLaidOutChoice {
    #[must_use]
    pub const fn id(self) -> QuickSettingsChoiceId {
        self.id
    }
    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }
}
impl QuickSettingsLaidOutSlider {
    #[must_use]
    pub const fn kind(self) -> QuickControlKind {
        self.kind
    }
    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }
    #[must_use]
    pub const fn track(self) -> DipRect {
        self.track
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickSettingsLayout {
    content_bounds: DipRect,
    tiles: Vec<QuickSettingsLaidOutTile>,
    sections: Vec<(QuickControlKind, DipRect)>,
    sliders: Vec<QuickSettingsLaidOutSlider>,
    choices: Vec<QuickSettingsLaidOutChoice>,
    media: Option<QuickSettingsLaidOutMedia>,
    media_choices: Vec<QuickSettingsLaidOutMediaChoice>,
    back_bounds: Option<DipRect>,
    edit_bounds: Option<DipRect>,
    content_height: f32,
}

impl QuickSettingsLayout {
    #[must_use]
    pub const fn content_bounds(&self) -> DipRect {
        self.content_bounds
    }
    #[must_use]
    pub fn tiles(&self) -> &[QuickSettingsLaidOutTile] {
        &self.tiles
    }
    #[must_use]
    pub fn sections(&self) -> &[(QuickControlKind, DipRect)] {
        &self.sections
    }
    #[must_use]
    pub fn sliders(&self) -> &[QuickSettingsLaidOutSlider] {
        &self.sliders
    }
    #[must_use]
    pub fn choices(&self) -> &[QuickSettingsLaidOutChoice] {
        &self.choices
    }
    #[must_use]
    pub const fn media(&self) -> Option<QuickSettingsLaidOutMedia> {
        self.media
    }
    #[must_use]
    pub fn media_choices(&self) -> &[QuickSettingsLaidOutMediaChoice] {
        &self.media_choices
    }
    #[must_use]
    pub const fn back_bounds(&self) -> Option<DipRect> {
        self.back_bounds
    }
    #[must_use]
    pub fn brightness(&self) -> Option<QuickSettingsLaidOutSlider> {
        self.sliders
            .iter()
            .copied()
            .find(|slider| slider.kind == QuickControlKind::Brightness)
    }
    #[must_use]
    pub const fn edit_bounds(&self) -> Option<DipRect> {
        self.edit_bounds
    }
    #[must_use]
    pub const fn content_height(&self) -> f32 {
        self.content_height
    }

    #[must_use]
    pub fn slider_value_at(&self, kind: QuickControlKind, x: f32) -> Option<u8> {
        self.sliders
            .iter()
            .find(|slider| slider.kind == kind)
            .map(|slider| {
                let ratio = ((x - slider.track.x) / slider.track.width).clamp(0.0, 1.0);
                (ratio * 100.0).round() as u8
            })
    }

    #[must_use]
    pub fn hit_test(&self, point: DipPoint) -> Option<QuickSettingsHit> {
        if self
            .back_bounds
            .is_some_and(|bounds| contains(bounds, point))
        {
            return Some(QuickSettingsHit::Back);
        }
        if let Some(choice) = self
            .media_choices
            .iter()
            .find(|choice| contains(choice.bounds, point))
        {
            return Some(QuickSettingsHit::MediaSession(choice.id));
        }
        if let Some(choice) = self
            .choices
            .iter()
            .find(|choice| contains(choice.bounds, point))
        {
            return Some(QuickSettingsHit::Choice(choice.id));
        }
        if let Some(media) = self.media {
            if contains(media.artwork, point) {
                return Some(QuickSettingsHit::MediaArtwork);
            }
            for (bounds, action) in [
                (media.previous, QuickSettingsMediaAction::Previous),
                (media.toggle, QuickSettingsMediaAction::TogglePlayback),
                (media.next, QuickSettingsMediaAction::Next),
            ] {
                if contains(bounds, point) {
                    return Some(QuickSettingsHit::MediaAction(action));
                }
            }
        }
        for slider in &self.sliders {
            if contains(slider.bounds, point) {
                let ratio = ((point.x - slider.track.x) / slider.track.width).clamp(0.0, 1.0);
                return Some(QuickSettingsHit::Slider {
                    kind: slider.kind,
                    value: (ratio * 100.0).round() as u8,
                });
            }
        }
        if let Some(tile) = self.tiles.iter().find(|tile| contains(tile.bounds, point)) {
            return Some(QuickSettingsHit::Tile(tile.kind));
        }
        if self
            .edit_bounds
            .is_some_and(|bounds| contains(bounds, point))
        {
            Some(QuickSettingsHit::EditControls)
        } else {
            None
        }
    }
}

#[must_use]
pub fn layout_quick_settings(scene: &QuickSettingsScene, surface: DipRect) -> QuickSettingsLayout {
    let offset = scene.scroll_offset();
    let content = DipRect::new(
        surface.x + OUTER,
        surface.y + QUICK_SETTINGS_BODY_TOP + OUTER,
        surface.width - OUTER * 2.0,
        (surface.height - QUICK_SETTINGS_BODY_TOP - OUTER * 2.0).max(0.0),
    );
    let mut y = content.y - offset;
    if let Some(media_choices) = scene.media_choices() {
        let back_bounds = DipRect::new(content.x, y + 6.0, 36.0, 36.0);
        y += SUBMENU_HEADER_HEIGHT + GAP;
        let choices = media_choices
            .iter()
            .enumerate()
            .map(|(index, choice)| QuickSettingsLaidOutMediaChoice {
                id: choice.session_id(),
                bounds: DipRect::new(
                    content.x,
                    y + index as f32 * (SUBMENU_CHOICE_HEIGHT + 6.0),
                    content.width,
                    SUBMENU_CHOICE_HEIGHT,
                ),
            })
            .collect::<Vec<_>>();
        if !choices.is_empty() {
            y += choices.len() as f32 * SUBMENU_CHOICE_HEIGHT
                + choices.len().saturating_sub(1) as f32 * 6.0;
        }
        let content_height = y + OUTER + offset - surface.y;
        return QuickSettingsLayout {
            content_bounds: content,
            tiles: Vec::new(),
            sections: Vec::new(),
            sliders: Vec::new(),
            choices: Vec::new(),
            media: None,
            media_choices: choices,
            back_bounds: Some(back_bounds),
            edit_bounds: None,
            content_height,
        };
    }
    if let Some(submenu) = scene.submenu() {
        let back_bounds = DipRect::new(content.x, y + 6.0, 36.0, 36.0);
        y += SUBMENU_HEADER_HEIGHT + GAP;
        let choices = submenu
            .choices()
            .iter()
            .enumerate()
            .map(|(index, choice)| QuickSettingsLaidOutChoice {
                id: choice.id(),
                bounds: DipRect::new(
                    content.x,
                    y + index as f32 * (SUBMENU_CHOICE_HEIGHT + 6.0),
                    content.width,
                    SUBMENU_CHOICE_HEIGHT,
                ),
            })
            .collect::<Vec<_>>();
        if !choices.is_empty() {
            y += choices.len() as f32 * SUBMENU_CHOICE_HEIGHT
                + choices.len().saturating_sub(1) as f32 * 6.0;
        }
        let content_height = y + OUTER + offset - surface.y;
        return QuickSettingsLayout {
            content_bounds: content,
            tiles: Vec::new(),
            sections: Vec::new(),
            sliders: Vec::new(),
            choices,
            media: None,
            media_choices: Vec::new(),
            back_bounds: Some(back_bounds),
            edit_bounds: None,
            content_height,
        };
    }
    let mut tiles = Vec::with_capacity(scene.tiles().len());
    let column = (content.width - GAP) / 2.0;
    let mut media_layout = None;
    let mut remaining_tiles = scene.tiles().iter().collect::<Vec<_>>();
    if scene.media().is_some() {
        let media_bounds = DipRect::new(content.x, y, column, TILE_HEIGHT * 2.0 + GAP);
        let action_width = 30.0;
        let action_gap = 8.0;
        let actions_width = action_width * 3.0 + action_gap * 2.0;
        let actions_x = media_bounds.x + (media_bounds.width - actions_width) / 2.0;
        let actions_y = media_bounds.y + media_bounds.height - 38.0;
        media_layout = Some(QuickSettingsLaidOutMedia {
            bounds: media_bounds,
            artwork: DipRect::new(media_bounds.x + 12.0, media_bounds.y + 12.0, 48.0, 48.0),
            previous: DipRect::new(actions_x, actions_y, action_width, 30.0),
            toggle: DipRect::new(
                actions_x + action_width + action_gap,
                actions_y,
                action_width,
                30.0,
            ),
            next: DipRect::new(
                actions_x + (action_width + action_gap) * 2.0,
                actions_y,
                action_width,
                30.0,
            ),
        });
        for (row, kind) in [QuickControlKind::Focus, QuickControlKind::Projection]
            .into_iter()
            .enumerate()
        {
            if let Some(index) = remaining_tiles.iter().position(|tile| tile.kind() == kind) {
                let tile = remaining_tiles.remove(index);
                tiles.push(QuickSettingsLaidOutTile {
                    kind: tile.kind(),
                    bounds: DipRect::new(
                        content.x + column + GAP,
                        y + row as f32 * (TILE_HEIGHT + GAP),
                        column,
                        TILE_HEIGHT,
                    ),
                });
            }
        }
        y += TILE_HEIGHT * 2.0 + GAP;
        if !remaining_tiles.is_empty() {
            y += GAP;
        }
    }
    for (index, tile) in remaining_tiles.iter().enumerate() {
        let is_last_odd = index + 1 == remaining_tiles.len() && remaining_tiles.len() % 2 == 1;
        let row = index / 2;
        let x = if is_last_odd {
            content.x
        } else {
            content.x + (index % 2) as f32 * (column + GAP)
        };
        let width = if is_last_odd { content.width } else { column };
        tiles.push(QuickSettingsLaidOutTile {
            kind: tile.kind(),
            bounds: DipRect::new(x, y + row as f32 * (TILE_HEIGHT + GAP), width, TILE_HEIGHT),
        });
    }
    if !remaining_tiles.is_empty() {
        let rows = remaining_tiles.len().div_ceil(2);
        y += rows as f32 * TILE_HEIGHT + rows.saturating_sub(1) as f32 * GAP;
    }

    let mut sections = Vec::new();
    let mut sliders = Vec::new();
    if let Some(display) = scene.display() {
        y += SECTION_GAP;
        let action_rows = display.actions().len().div_ceil(2);
        let height = SECTION_PADDING * 2.0
            + SECTION_HEADER_HEIGHT
            + display.brightness().map_or(0.0, |_| SLIDER_HEIGHT + GAP)
            + action_rows as f32 * ACTION_HEIGHT
            + action_rows.saturating_sub(1) as f32 * GAP;
        let bounds = DipRect::new(content.x, y, content.width, height);
        sections.push((QuickControlKind::Brightness, bounds));
        let mut inner_y = y + SECTION_PADDING + SECTION_HEADER_HEIGHT;
        if let Some(brightness) = display.brightness() {
            let slider_bounds = DipRect::new(
                content.x + SECTION_PADDING,
                inner_y,
                content.width - SECTION_PADDING * 2.0,
                SLIDER_HEIGHT,
            );
            sliders.push(slider_layout(brightness.kind(), slider_bounds));
            inner_y += SLIDER_HEIGHT + GAP;
        }
        for (index, action) in display.actions().iter().enumerate() {
            let action_column = (content.width - SECTION_PADDING * 2.0 - GAP) / 2.0;
            let row = index / 2;
            let x = content.x + SECTION_PADDING + (index % 2) as f32 * (action_column + GAP);
            tiles.push(QuickSettingsLaidOutTile {
                kind: action.kind(),
                bounds: DipRect::new(
                    x,
                    inner_y + row as f32 * (ACTION_HEIGHT + GAP),
                    action_column,
                    ACTION_HEIGHT,
                ),
            });
        }
        y += height;
    }
    if let Some(sound) = scene.sound() {
        y += SECTION_GAP;
        let height = SECTION_PADDING * 2.0 + SECTION_HEADER_HEIGHT + SLIDER_HEIGHT + 20.0;
        let bounds = DipRect::new(content.x, y, content.width, height);
        sections.push((QuickControlKind::Volume, bounds));
        sliders.push(slider_layout(
            sound.volume().kind(),
            DipRect::new(
                content.x + SECTION_PADDING,
                y + SECTION_PADDING + SECTION_HEADER_HEIGHT,
                content.width - SECTION_PADDING * 2.0,
                SLIDER_HEIGHT,
            ),
        ));
        y += height;
    }
    if let Some(energy) = scene.energy() {
        y += SECTION_GAP;
        let height = SECTION_PADDING * 2.0
            + SECTION_HEADER_HEIGHT
            + 40.0
            + energy.saver().map_or(0.0, |_| GAP + ACTION_HEIGHT);
        let bounds = DipRect::new(content.x, y, content.width, height);
        sections.push((QuickControlKind::Battery, bounds));
        if let Some(saver) = energy.saver() {
            tiles.push(QuickSettingsLaidOutTile {
                kind: saver.kind(),
                bounds: DipRect::new(
                    content.x + SECTION_PADDING,
                    y + SECTION_PADDING + SECTION_HEADER_HEIGHT + 40.0 + GAP,
                    content.width - SECTION_PADDING * 2.0,
                    ACTION_HEIGHT,
                ),
            });
        }
        y += height;
    }
    y += SECTION_GAP;
    let edit_bounds = DipRect::new(content.x, y, content.width, FOOTER_HEIGHT);
    let content_height = y + FOOTER_HEIGHT + OUTER + offset - surface.y;
    QuickSettingsLayout {
        content_bounds: content,
        tiles,
        sections,
        sliders,
        choices: Vec::new(),
        media: media_layout,
        media_choices: Vec::new(),
        back_bounds: None,
        edit_bounds: Some(edit_bounds),
        content_height,
    }
}

#[must_use]
pub fn quick_settings_surface_size(scene: &QuickSettingsScene, max_height: f32) -> (f32, f32) {
    let unconstrained =
        layout_quick_settings(scene, DipRect::new(0.0, 0.0, QUICK_SETTINGS_WIDTH, 4096.0));
    (
        QUICK_SETTINGS_WIDTH,
        unconstrained.content_height().min(max_height.max(120.0)),
    )
}

fn slider_layout(kind: QuickControlKind, bounds: DipRect) -> QuickSettingsLaidOutSlider {
    QuickSettingsLaidOutSlider {
        kind,
        bounds,
        track: DipRect::new(
            bounds.x + 36.0,
            bounds.y + (bounds.height - 8.0) / 2.0,
            (bounds.width - 48.0).max(1.0),
            8.0,
        ),
    }
}

fn contains(rect: DipRect, point: DipPoint) -> bool {
    point.x >= rect.x
        && point.y >= rect.y
        && point.x <= rect.x + rect.width
        && point.y <= rect.y + rect.height
}
