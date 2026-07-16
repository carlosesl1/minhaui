#![deny(unsafe_code)]

use shell_core::WindowId;

use crate::{DipRect, Dpi, PhysicalRect, PreviewCardVisual, WindowPreviewScene, physical_from_dip};

const PANEL_PADDING: f32 = 12.0;
const CARD_GAP: f32 = 10.0;
const TITLE_HEIGHT: f32 = 36.0;
const THUMBNAIL_WIDTH: f32 = 248.0;
const THUMBNAIL_HEIGHT: f32 = 140.0;
const CARD_MIN_WIDTH: f32 = 120.0;
const PAGINATION_HEIGHT: f32 = 24.0;
const WORK_MARGIN: f32 = 16.0;
const DOCK_GAP: f32 = 10.0;
pub const WINDOW_PREVIEW_CARD_RADIUS: f32 = 10.0;
pub const WINDOW_PREVIEW_THUMBNAIL_RADIUS: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewPanelSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewPlacementInput {
    pub anchor: PhysicalRect,
    pub dock: PhysicalRect,
    pub work: PhysicalRect,
    pub dpi: Dpi,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewCardLayout {
    window: WindowId,
    card: DipRect,
    thumbnail: DipRect,
    title: DipRect,
    close: DipRect,
}

impl PreviewCardLayout {
    #[must_use]
    pub const fn window(self) -> WindowId {
        self.window
    }

    #[must_use]
    pub const fn card(self) -> DipRect {
        self.card
    }

    #[must_use]
    pub const fn thumbnail(self) -> DipRect {
        self.thumbnail
    }

    #[must_use]
    pub const fn title(self) -> DipRect {
        self.title
    }

    #[must_use]
    pub const fn close(self) -> DipRect {
        self.close
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPreviewPanelLayout {
    panel: DipRect,
    cards: Vec<PreviewCardLayout>,
    previous: Option<DipRect>,
    next: Option<DipRect>,
    page_indicator: Option<DipRect>,
}

impl WindowPreviewPanelLayout {
    #[must_use]
    pub const fn panel(&self) -> DipRect {
        self.panel
    }

    #[must_use]
    pub fn cards(&self) -> &[PreviewCardLayout] {
        &self.cards
    }

    #[must_use]
    pub const fn previous(&self) -> Option<DipRect> {
        self.previous
    }

    #[must_use]
    pub const fn next(&self) -> Option<DipRect> {
        self.next
    }

    #[must_use]
    pub const fn page_indicator(&self) -> Option<DipRect> {
        self.page_indicator
    }
}

#[must_use]
pub fn preview_panel_size(card_count: usize, paginated: bool) -> PreviewPanelSize {
    let visible = card_count.clamp(1, 4);
    let single = visible == 1;
    let columns = if single { 1.0 } else { 2.0 };
    let rows = if visible <= 2 { 1.0 } else { 2.0 };
    let thumbnail_width = THUMBNAIL_WIDTH;
    let thumbnail_height = THUMBNAIL_HEIGHT;
    PreviewPanelSize {
        width: PANEL_PADDING * 2.0 + thumbnail_width * columns + CARD_GAP * (columns - 1.0),
        height: PANEL_PADDING * 2.0
            + (thumbnail_height + TITLE_HEIGHT) * rows
            + CARD_GAP * (rows - 1.0)
            + if paginated { PAGINATION_HEIGHT } else { 0.0 },
    }
}

#[must_use]
pub fn preview_panel_size_for_scene(scene: &WindowPreviewScene) -> PreviewPanelSize {
    let visible = scene.visible_cards();
    let columns = if visible.len() <= 1 { 1 } else { 2 };
    let rows = visible.len().clamp(1, 4).div_ceil(columns);
    let (column_widths, row_heights) = preview_grid_dimensions(scene);
    PreviewPanelSize {
        width: PANEL_PADDING * 2.0
            + column_widths[..columns].iter().sum::<f32>()
            + CARD_GAP * columns.saturating_sub(1) as f32,
        height: PANEL_PADDING * 2.0
            + row_heights[..rows].iter().sum::<f32>()
            + CARD_GAP * rows.saturating_sub(1) as f32
            + if scene.page_count() > 1 {
                PAGINATION_HEIGHT
            } else {
                0.0
            },
    }
}

#[must_use]
pub fn layout_window_preview(
    scene: &WindowPreviewScene,
    surface: DipRect,
) -> WindowPreviewPanelLayout {
    let visible = scene.visible_cards();
    let single = visible.len() <= 1;
    let (column_widths, row_heights) = preview_grid_dimensions(scene);
    let cards = visible
        .iter()
        .enumerate()
        .map(|(index, visual)| {
            let column = if single { 0 } else { index % 2 };
            let row = if single { 0 } else { index / 2 };
            let x = PANEL_PADDING
                + column_widths[..column].iter().sum::<f32>()
                + CARD_GAP * column as f32;
            let y = PANEL_PADDING + row_heights[..row].iter().sum::<f32>() + CARD_GAP * row as f32;
            let card_width = column_widths[column];
            let card_height = row_heights[row];
            let thumbnail_width = preview_thumbnail_width(visual);
            let thumbnail_height = THUMBNAIL_HEIGHT;
            let thumbnail_x = x + (card_width - thumbnail_width) / 2.0;
            PreviewCardLayout {
                window: visual.window(),
                card: DipRect::new(x, y, card_width, card_height),
                thumbnail: DipRect::new(
                    thumbnail_x,
                    y + TITLE_HEIGHT,
                    thumbnail_width,
                    thumbnail_height,
                ),
                title: DipRect::new(x + 10.0, y, card_width - 50.0, TITLE_HEIGHT),
                close: DipRect::new(x + card_width - 32.0, y + 6.0, 24.0, 24.0),
            }
        })
        .collect();
    let paginated = scene.page_count() > 1;
    let footer_y = surface.height - PANEL_PADDING - 20.0;
    WindowPreviewPanelLayout {
        panel: surface,
        cards,
        previous: paginated.then_some(DipRect::new(PANEL_PADDING, footer_y, 20.0, 20.0)),
        next: paginated.then_some(DipRect::new(
            surface.width - PANEL_PADDING - 20.0,
            footer_y,
            20.0,
            20.0,
        )),
        page_indicator: paginated.then_some(DipRect::new(
            surface.width / 2.0 - 30.0,
            footer_y,
            60.0,
            20.0,
        )),
    }
}

fn preview_grid_dimensions(scene: &WindowPreviewScene) -> ([f32; 2], [f32; 2]) {
    let single = scene.visible_cards().len() <= 1;
    let mut column_widths = [0.0_f32; 2];
    let mut row_heights = [0.0_f32; 2];
    for (index, visual) in scene.visible_cards().iter().enumerate() {
        let column = if single { 0 } else { index % 2 };
        let row = if single { 0 } else { index / 2 };
        column_widths[column] =
            column_widths[column].max(preview_thumbnail_width(visual).max(CARD_MIN_WIDTH));
        row_heights[row] = row_heights[row].max(THUMBNAIL_HEIGHT + TITLE_HEIGHT);
    }
    if scene.visible_cards().is_empty() {
        column_widths[0] = THUMBNAIL_WIDTH;
        row_heights[0] = THUMBNAIL_HEIGHT + TITLE_HEIGHT;
    }
    (column_widths, row_heights)
}

fn preview_thumbnail_width(visual: &PreviewCardVisual) -> f32 {
    let Some(source) = visual.source_size() else {
        return THUMBNAIL_WIDTH;
    };
    if source.width() == 0 || source.height() == 0 {
        return THUMBNAIL_WIDTH;
    }
    (THUMBNAIL_HEIGHT * source.width() as f32 / source.height() as f32).min(THUMBNAIL_WIDTH)
}

#[must_use]
pub fn place_window_preview(input: PreviewPlacementInput, size: PreviewPanelSize) -> PhysicalRect {
    let width = physical_from_dip(size.width, input.dpi);
    let height = physical_from_dip(size.height, input.dpi);
    let margin = physical_from_dip(WORK_MARGIN, input.dpi);
    let gap = physical_from_dip(DOCK_GAP, input.dpi);
    let min_x = input.work.x + margin;
    let max_x = input.work.x + input.work.width - margin - width;
    let x = (input.anchor.x + input.anchor.width / 2 - width / 2).clamp(min_x, max_x.max(min_x));
    let above = input.dock.y - gap - height;
    let below = input.dock.y + input.dock.height + gap;
    let preferred_y = if above >= input.work.y + margin {
        above
    } else {
        below
    };
    let min_y = input.work.y + margin;
    let max_y = input.work.y + input.work.height - margin - height;
    PhysicalRect::new(x, preferred_y.clamp(min_y, max_y.max(min_y)), width, height)
}
