use crate::{DipRect, Dpi, PhysicalRect, PopoverScene, physical_from_dip};

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
}

impl PopoverLayout {
    #[must_use]
    pub fn rows(&self) -> &[PopoverLaidOutRow] {
        &self.rows
    }
}

#[must_use]
pub fn layout_popover_scene(scene: &PopoverScene, surface: DipRect) -> PopoverLayout {
    let mut rows = Vec::new();
    let mut y = surface.y + 48.0;
    for (index, row) in scene.rows().iter().enumerate() {
        if y + 34.0 > surface.y + surface.height - 12.0 {
            break;
        }
        rows.push(PopoverLaidOutRow {
            index,
            bounds: DipRect::new(surface.x + 12.0, y, surface.width - 24.0, 30.0),
            focused: row.enabled() && scene.focused() == Some(index),
        });
        y += 36.0;
    }
    PopoverLayout { rows }
}

#[must_use]
pub fn popover_anchor_rect(anchor: PhysicalRect, work: PhysicalRect, dpi: Dpi) -> PhysicalRect {
    let width = physical_from_dip(360.0, dpi);
    let height = physical_from_dip(420.0, dpi);
    let margin = physical_from_dip(8.0, dpi);
    let x = (anchor.x + anchor.width - width).clamp(work.x, work.x + work.width - width);
    let y = (anchor.y + anchor.height + margin).clamp(work.y, work.y + work.height - height);
    PhysicalRect::new(x, y, width, height)
}
