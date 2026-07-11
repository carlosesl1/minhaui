#![deny(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dpi {
    raw: u32,
}

impl Dpi {
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self { raw }
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.raw
    }

    #[must_use]
    pub fn scale(self) -> f32 {
        self.raw as f32 / 96.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DipPoint {
    pub x: f32,
    pub y: f32,
}

impl DipPoint {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DipRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DipRect {
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl PhysicalRect {
    #[must_use]
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellMetrics {
    topbar_margin_x_dip: f32,
    topbar_margin_top_dip: f32,
    topbar_height_dip: f32,
    dock_width_dip: f32,
    dock_height_dip: f32,
    dock_margin_bottom_dip: f32,
}

impl Default for ShellMetrics {
    fn default() -> Self {
        Self {
            topbar_margin_x_dip: 9.6,
            topbar_margin_top_dip: 8.0,
            topbar_height_dip: 32.0,
            dock_width_dip: 574.4,
            dock_height_dip: 72.0,
            dock_margin_bottom_dip: 12.0,
        }
    }
}

#[must_use]
pub fn physical_from_dip(dip: f32, dpi: Dpi) -> i32 {
    (dip * dpi.scale()).round() as i32
}

#[must_use]
pub fn topbar_rect(work_area: PhysicalRect, dpi: Dpi, metrics: ShellMetrics) -> PhysicalRect {
    let margin_x = physical_from_dip(metrics.topbar_margin_x_dip, dpi);
    let margin_top = physical_from_dip(metrics.topbar_margin_top_dip, dpi);
    let height = physical_from_dip(metrics.topbar_height_dip, dpi);
    PhysicalRect::new(
        work_area.x + margin_x,
        work_area.y + margin_top,
        work_area.width - margin_x * 2,
        height,
    )
}

#[must_use]
pub fn dock_showcase_rect(
    work_area: PhysicalRect,
    dpi: Dpi,
    metrics: ShellMetrics,
) -> PhysicalRect {
    let width = physical_from_dip(metrics.dock_width_dip, dpi);
    let height = physical_from_dip(metrics.dock_height_dip, dpi);
    let bottom = physical_from_dip(metrics.dock_margin_bottom_dip, dpi);
    PhysicalRect::new(
        work_area.x + (work_area.width - width) / 2,
        work_area.y + work_area.height - height - bottom,
        width,
        height,
    )
}

#[must_use]
pub const fn apply_dpi_suggested_rect(
    _current: PhysicalRect,
    suggested: PhysicalRect,
    _old_dpi: Dpi,
    _new_dpi: Dpi,
) -> PhysicalRect {
    suggested
}

#[must_use]
pub fn rounded_content_hit(bounds: DipRect, radius: f32, point: DipPoint) -> bool {
    if point.x < bounds.x
        || point.y < bounds.y
        || point.x > bounds.x + bounds.width
        || point.y > bounds.y + bounds.height
    {
        return false;
    }

    let left = bounds.x + radius;
    let right = bounds.x + bounds.width - radius;
    let top = bounds.y + radius;
    let bottom = bounds.y + bounds.height - radius;

    if (left..=right).contains(&point.x) || (top..=bottom).contains(&point.y) {
        return true;
    }

    let center_x = if point.x < left { left } else { right };
    let center_y = if point.y < top { top } else { bottom };
    let dx = point.x - center_x;
    let dy = point.y - center_y;
    dx * dx + dy * dy <= radius * radius
}
