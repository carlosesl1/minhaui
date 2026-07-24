use crate::{
    DipPoint, DipRect, Dpi, PhysicalRect, PopoverLayoutStyle, PopoverScene, physical_from_dip,
};

const COMPACT_WIDTH: f32 = 244.0;
const SYSTEM_WIDTH: f32 = 288.0;
const SYSTEM_TITLE_Y: f32 = 20.0;
const SYSTEM_TITLE_HEIGHT: f32 = 28.0;
const SYSTEM_ROW_HEIGHT: f32 = 40.0;
const SYSTEM_SECTION_GAP: f32 = 12.0;
const SYSTEM_BODY_TOP: f32 = 60.0;
const SYSTEM_BOTTOM_PADDING: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverSurfaceSize {
    width: f32,
    height: f32,
}

impl PopoverSurfaceSize {
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn width(self) -> f32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverPlacement {
    rect: PhysicalRect,
    anchor_x_dip: f32,
}

impl PopoverPlacement {
    #[must_use]
    pub const fn rect(self) -> PhysicalRect {
        self.rect
    }

    #[must_use]
    pub const fn anchor_x_dip(self) -> f32 {
        self.anchor_x_dip
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverSeparator {
    bounds: DipRect,
}

impl PopoverSeparator {
    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverNotch {
    bounds: DipRect,
    tip_x: f32,
}

impl PopoverNotch {
    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }

    #[must_use]
    pub const fn tip_x(self) -> f32 {
        self.tip_x
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverLaidOutRow {
    index: usize,
    bounds: DipRect,
    focused: bool,
}

impl PopoverLaidOutRow {
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
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

#[derive(Clone, Debug, PartialEq)]
pub struct PopoverLayout {
    rows: Vec<PopoverLaidOutRow>,
    title_bounds: Option<DipRect>,
    separators: Vec<PopoverSeparator>,
    notch: Option<PopoverNotch>,
}

impl PopoverLayout {
    #[must_use]
    pub fn rows(&self) -> &[PopoverLaidOutRow] {
        &self.rows
    }

    #[must_use]
    pub const fn title_bounds(&self) -> Option<DipRect> {
        self.title_bounds
    }

    #[must_use]
    pub fn separators(&self) -> &[PopoverSeparator] {
        &self.separators
    }

    #[must_use]
    pub const fn notch(&self) -> Option<PopoverNotch> {
        self.notch
    }

    #[must_use]
    pub fn hit_test(&self, point: DipPoint) -> Option<usize> {
        self.rows
            .iter()
            .find(|row| contains(row.bounds, point))
            .map(|row| row.index)
    }
}

fn contains(bounds: DipRect, point: DipPoint) -> bool {
    point.x >= bounds.x
        && point.x <= bounds.x + bounds.width
        && point.y >= bounds.y
        && point.y <= bounds.y + bounds.height
}

#[must_use]
pub fn layout_popover_scene(scene: &PopoverScene, surface: DipRect) -> PopoverLayout {
    match scene.layout_style() {
        PopoverLayoutStyle::Compact => layout_compact_popover_scene(scene, surface),
        PopoverLayoutStyle::SystemPanel => layout_system_panel_scene(scene, surface),
    }
}

fn layout_compact_popover_scene(scene: &PopoverScene, surface: DipRect) -> PopoverLayout {
    let mut rows = Vec::new();
    let mut y = surface.y + 5.0;
    for (index, row) in scene.rows().iter().enumerate().skip(scene.scroll_offset()) {
        if y + 24.0 > surface.y + surface.height - 5.0 {
            break;
        }
        rows.push(PopoverLaidOutRow {
            index,
            bounds: DipRect::new(surface.x + 12.0, y, surface.width - 24.0, 24.0),
            focused: row.enabled() && scene.focused() == Some(index),
        });
        y += 24.0;
    }
    PopoverLayout {
        rows,
        title_bounds: None,
        separators: Vec::new(),
        notch: None,
    }
}

fn layout_system_panel_scene(scene: &PopoverScene, surface: DipRect) -> PopoverLayout {
    let mut rows = Vec::new();
    let mut separators = Vec::new();
    let mut y = surface.y + SYSTEM_BODY_TOP;
    let tip_x = (surface.x + scene.anchor_x().unwrap_or(surface.width / 2.0))
        .clamp(surface.x + 16.0, surface.x + surface.width - 16.0);

    for (index, row) in scene.rows().iter().enumerate().skip(scene.scroll_offset()) {
        let section_gap = index > 0 && row.section_start();
        let row_y = y + if section_gap { SYSTEM_SECTION_GAP } else { 0.0 };
        if row_y + SYSTEM_ROW_HEIGHT > surface.y + surface.height - SYSTEM_BOTTOM_PADDING {
            break;
        }
        if section_gap {
            separators.push(PopoverSeparator {
                bounds: DipRect::new(
                    surface.x + 16.0,
                    y + (SYSTEM_SECTION_GAP - 1.0) / 2.0,
                    surface.width - 32.0,
                    1.0,
                ),
            });
            y += SYSTEM_SECTION_GAP;
        }
        rows.push(PopoverLaidOutRow {
            index,
            bounds: DipRect::new(surface.x + 16.0, y, surface.width - 32.0, SYSTEM_ROW_HEIGHT),
            focused: row.enabled() && scene.focused() == Some(index),
        });
        y += SYSTEM_ROW_HEIGHT;
    }

    PopoverLayout {
        rows,
        title_bounds: Some(DipRect::new(
            surface.x + 16.0,
            surface.y + SYSTEM_TITLE_Y,
            surface.width - 32.0,
            SYSTEM_TITLE_HEIGHT,
        )),
        separators,
        notch: Some(PopoverNotch {
            bounds: DipRect::new(tip_x - 8.0, surface.y, 16.0, 8.0),
            tip_x,
        }),
    }
}

#[must_use]
pub fn popover_surface_size(scene: &PopoverScene) -> PopoverSurfaceSize {
    match scene.layout_style() {
        PopoverLayoutStyle::Compact => {
            PopoverSurfaceSize::new(COMPACT_WIDTH, popover_height_for_rows(scene.rows().len()))
        }
        PopoverLayoutStyle::SystemPanel => {
            let section_count = scene
                .rows()
                .iter()
                .enumerate()
                .filter(|(index, row)| *index > 0 && row.section_start())
                .count() as f32;
            PopoverSurfaceSize::new(
                SYSTEM_WIDTH,
                SYSTEM_BODY_TOP
                    + scene.rows().len() as f32 * SYSTEM_ROW_HEIGHT
                    + section_count * SYSTEM_SECTION_GAP
                    + SYSTEM_BOTTOM_PADDING,
            )
        }
    }
}

#[must_use]
pub fn popover_anchor_rect(anchor: PhysicalRect, work: PhysicalRect, dpi: Dpi) -> PhysicalRect {
    popover_anchor_rect_with_height(anchor, work, dpi, 420.0)
}

#[must_use]
pub fn popover_height_for_rows(row_count: usize) -> f32 {
    (10.0 + row_count as f32 * 24.0).clamp(58.0, 420.0)
}

#[must_use]
pub fn popover_anchor_rect_with_height(
    anchor: PhysicalRect,
    work: PhysicalRect,
    dpi: Dpi,
    height_dip: f32,
) -> PhysicalRect {
    popover_placement(
        anchor,
        work,
        dpi,
        PopoverSurfaceSize::new(COMPACT_WIDTH, height_dip),
    )
    .rect()
}

#[must_use]
pub fn popover_placement(
    anchor: PhysicalRect,
    work: PhysicalRect,
    dpi: Dpi,
    size: PopoverSurfaceSize,
) -> PopoverPlacement {
    let width = physical_from_dip(size.width().clamp(244.0, 440.0), dpi);
    let max_height = work.height as f32 / dpi.scale();
    let height = physical_from_dip(size.height().clamp(58.0, max_height), dpi);
    let margin = physical_from_dip(8.0, dpi);
    let x = (anchor.x + anchor.width / 2 - width / 2).clamp(work.x, work.x + work.width - width);
    let y = (anchor.y + anchor.height + margin).clamp(work.y, work.y + work.height - height);
    let anchor_center = anchor.x + anchor.width / 2;
    let anchor_x_dip =
        ((anchor_center - x) as f32 * 96.0 / dpi.raw() as f32).clamp(16.0, size.width() - 16.0);
    PopoverPlacement {
        rect: PhysicalRect::new(x, y, width, height),
        anchor_x_dip,
    }
}
