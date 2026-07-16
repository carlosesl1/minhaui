use crate::{
    ContextMenuEntry, ContextMenuScene, DipPoint, DipRect, Dpi, PhysicalRect, physical_from_dip,
};

const CARD_WIDTH: f32 = 260.0;
const GUTTER_X: f32 = 20.0;
const GUTTER_TOP: f32 = 12.0;
const GUTTER_BOTTOM: f32 = 36.0;
const PADDING_X: f32 = 12.0;
const PADDING_Y: f32 = 5.0;
const ROW_HEIGHT: f32 = 30.0;
const SEPARATOR_HEIGHT: f32 = 9.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextMenuLaidOutRow {
    entry_index: usize,
    bounds: DipRect,
    focused: bool,
}

impl ContextMenuLaidOutRow {
    #[must_use]
    pub const fn entry_index(self) -> usize {
        self.entry_index
    }

    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }

    #[must_use]
    pub const fn focused(self) -> bool {
        self.focused
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextMenuSeparator {
    bounds: DipRect,
}

impl ContextMenuSeparator {
    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContextMenuLayout {
    card_bounds: DipRect,
    rows: Vec<ContextMenuLaidOutRow>,
    separators: Vec<ContextMenuSeparator>,
}

impl ContextMenuLayout {
    #[must_use]
    pub const fn card_bounds(&self) -> DipRect {
        self.card_bounds
    }

    #[must_use]
    pub fn rows(&self) -> &[ContextMenuLaidOutRow] {
        &self.rows
    }

    #[must_use]
    pub fn separators(&self) -> &[ContextMenuSeparator] {
        &self.separators
    }

    #[must_use]
    pub fn hit_test(&self, point: DipPoint) -> Option<usize> {
        self.rows
            .iter()
            .find(|row| contains_semi_open(row.bounds, point))
            .map(|row| row.entry_index)
    }
}

#[must_use]
pub const fn context_menu_surface_width() -> f32 {
    CARD_WIDTH + GUTTER_X * 2.0
}

#[must_use]
pub fn context_menu_height_for_entries(entries: &[ContextMenuEntry]) -> f32 {
    let content = entries.iter().fold(PADDING_Y * 2.0, |height, entry| {
        height
            + if entry.is_separator() {
                SEPARATOR_HEIGHT
            } else {
                ROW_HEIGHT
            }
    });
    GUTTER_TOP + content + GUTTER_BOTTOM
}

#[must_use]
pub fn layout_context_menu_scene(scene: &ContextMenuScene, _surface: DipRect) -> ContextMenuLayout {
    let card_height = context_menu_height_for_entries(scene.entries()) - GUTTER_TOP - GUTTER_BOTTOM;
    let card_bounds = DipRect::new(GUTTER_X, GUTTER_TOP, CARD_WIDTH, card_height);
    let mut rows = Vec::new();
    let mut separators = Vec::new();
    let mut y = card_bounds.y + PADDING_Y;
    for (entry_index, entry) in scene.entries().iter().enumerate() {
        if entry.is_separator() {
            separators.push(ContextMenuSeparator {
                bounds: DipRect::new(
                    card_bounds.x + PADDING_X,
                    y,
                    card_bounds.width - PADDING_X * 2.0,
                    SEPARATOR_HEIGHT,
                ),
            });
            y += SEPARATOR_HEIGHT;
        } else {
            rows.push(ContextMenuLaidOutRow {
                entry_index,
                bounds: DipRect::new(
                    card_bounds.x + PADDING_X,
                    y,
                    card_bounds.width - PADDING_X * 2.0,
                    ROW_HEIGHT,
                ),
                focused: entry.enabled() && scene.focused() == Some(entry_index),
            });
            y += ROW_HEIGHT;
        }
    }
    ContextMenuLayout {
        card_bounds,
        rows,
        separators,
    }
}

#[must_use]
pub fn context_menu_anchor_rect(
    anchor: PhysicalRect,
    work: PhysicalRect,
    dpi: Dpi,
    height_dip: f32,
) -> PhysicalRect {
    let width = physical_from_dip(context_menu_surface_width(), dpi).min(work.width.max(1));
    let height = physical_from_dip(height_dip, dpi).min(work.height.max(1));
    let x =
        (anchor.x - physical_from_dip(GUTTER_X, dpi)).clamp(work.x, work.x + work.width - width);
    let above = anchor.y - height;
    let below = anchor.y + anchor.height;
    let y = if above >= work.y {
        above
    } else {
        below.clamp(work.y, work.y + work.height - height)
    };
    PhysicalRect::new(x, y, width, height)
}

fn contains_semi_open(bounds: DipRect, point: DipPoint) -> bool {
    point.x >= bounds.x
        && point.x < bounds.x + bounds.width
        && point.y >= bounds.y
        && point.y < bounds.y + bounds.height
}
