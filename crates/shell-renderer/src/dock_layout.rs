use crate::{DipPoint, DipRect, DockItemVisual, DockItemVisualKind, DockLayoutConfig, DockScene};
#[cfg(test)]
use crate::{PreviewUnavailableReason, WindowPreviewCapture, WindowPreviewVisual};

#[derive(Clone, Debug, PartialEq)]
pub struct DockLaidOutItem {
    visual: DockItemVisual,
    bounds: DipRect,
    hit_bounds: DipRect,
    focused: bool,
    hovered: bool,
    pressed: bool,
    dragged: bool,
}

impl DockLaidOutItem {
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.visual.id()
    }

    #[must_use]
    pub const fn kind(&self) -> DockItemVisualKind {
        self.visual.kind()
    }

    #[must_use]
    pub const fn indicator(&self) -> crate::RunningIndicator {
        self.visual.indicator()
    }

    #[must_use]
    pub const fn icon(&self) -> &crate::DockIcon {
        self.visual.icon()
    }

    #[must_use]
    pub const fn bounds(&self) -> DipRect {
        self.bounds
    }

    #[must_use]
    pub const fn focused(&self) -> bool {
        self.focused
    }

    #[must_use]
    pub const fn hovered(&self) -> bool {
        self.hovered
    }

    #[must_use]
    pub const fn pressed(&self) -> bool {
        self.pressed
    }

    #[must_use]
    pub const fn dragged(&self) -> bool {
        self.dragged
    }

    #[must_use]
    pub fn label(&self) -> &str {
        self.visual.label()
    }

    #[must_use]
    pub fn accessible_name(&self) -> &str {
        self.visual.label()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockLayout {
    items: Vec<DockLaidOutItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub enum WindowPreviewRenderKind {
    Thumbnail,
    Unavailable(PreviewUnavailableReason),
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg(test)]
pub struct WindowPreviewLayout {
    visual: WindowPreviewVisual,
    bounds: DipRect,
    kind: WindowPreviewRenderKind,
}

impl DockLayout {
    #[must_use]
    pub fn items(&self) -> &[DockLaidOutItem] {
        &self.items
    }

    #[must_use]
    pub fn hit_test(&self, point: DipPoint) -> Option<u64> {
        self.items
            .iter()
            .find(|item| contains(item.hit_bounds, point))
            .map(DockLaidOutItem::id)
    }
}

#[cfg(test)]
impl WindowPreviewLayout {
    #[must_use]
    #[expect(
        dead_code,
        reason = "retained for the internal legacy preview-layout test seam"
    )]
    pub const fn visual(self) -> WindowPreviewVisual {
        self.visual
    }

    #[must_use]
    pub const fn bounds(self) -> DipRect {
        self.bounds
    }

    #[must_use]
    pub const fn kind(self) -> WindowPreviewRenderKind {
        self.kind
    }
}

#[must_use]
pub fn layout_dock_scene(scene: &DockScene, surface: DipRect) -> DockLayout {
    let config = scene.config();
    let resting_width = dock_scene_width(scene);
    let mut resting_x = match config.alignment() {
        crate::DockAlignment::Left => surface.x + config.padding(),
        crate::DockAlignment::Center => {
            surface.x + (surface.width - resting_width) / 2.0 + config.padding()
        }
        crate::DockAlignment::Right => surface.x + surface.width - resting_width + config.padding(),
    };
    let resting_bounds = scene
        .items()
        .iter()
        .map(|visual| {
            let base_size = base_size(visual, config);
            let bounds = DipRect::new(
                resting_x,
                surface.y + config.padding(),
                base_size,
                config.item_size(),
            );
            resting_x += base_size + config.spacing();
            bounds
        })
        .collect::<Vec<_>>();
    let hover_position_x = scene.hover_position_x().or_else(|| {
        scene
            .items()
            .iter()
            .position(|item| Some(item.id()) == scene.hovered_item())
            .map(|index| center_x(resting_bounds[index]))
    });
    let visual_sizes = scene
        .items()
        .iter()
        .enumerate()
        .map(|(index, visual)| visual_size(scene, visual, resting_bounds[index], hover_position_x))
        .collect::<Vec<_>>();
    let visual_content_width = visual_sizes.iter().sum::<f32>()
        + scene.items().len().saturating_sub(1) as f32 * config.spacing();
    let resting_content_width = resting_width - config.padding() * 2.0;
    let resting_center = resting_bounds
        .first()
        .map_or(surface.x + surface.width / 2.0, |first| {
            first.x + resting_content_width / 2.0
        });
    let mut visual_x = resting_center - visual_content_width / 2.0;
    let items = scene
        .items()
        .iter()
        .enumerate()
        .map(|(index, visual)| {
            let size = visual_sizes[index];
            let (width, height) = match visual.kind() {
                DockItemVisualKind::Separator => (size, config.item_size()),
                DockItemVisualKind::App => (size, size),
            };
            let growth = (height - config.item_size()).max(0.0);
            let mut bounds = DipRect::new(
                visual_x,
                surface.y + config.padding() - growth,
                width,
                height,
            );
            visual_x += width + config.spacing();
            let dragged = scene.dragged_item() == Some(visual.id());
            if dragged {
                let scale = if visual.kind() == DockItemVisualKind::App {
                    1.05
                } else {
                    1.0
                };
                let dragged_width = width * scale;
                let dragged_height = height * scale;
                let center = scene.drag_position_x().unwrap_or(center_x(bounds)).clamp(
                    surface.x + config.padding() + dragged_width / 2.0,
                    surface.x + surface.width - config.padding() - dragged_width / 2.0,
                );
                bounds = DipRect::new(
                    center - dragged_width / 2.0,
                    surface.y + config.padding() - 4.0 - (dragged_height - height) / 2.0,
                    dragged_width,
                    dragged_height,
                );
            } else {
                bounds.x += scene.item_offset(visual.id());
            }
            let hit_bounds = match visual.kind() {
                DockItemVisualKind::Separator => DipRect::new(
                    resting_bounds[index].x - config.spacing() / 2.0,
                    resting_bounds[index].y,
                    resting_bounds[index].width + config.spacing(),
                    resting_bounds[index].height,
                ),
                DockItemVisualKind::App => resting_bounds[index],
            };
            DockLaidOutItem {
                visual: visual.clone(),
                bounds,
                hit_bounds,
                focused: scene.focused_item() == Some(visual.id()),
                hovered: scene.hovered_item() == Some(visual.id()),
                pressed: scene.pressed_item() == Some(visual.id()),
                dragged,
            }
        })
        .collect();
    DockLayout { items }
}

#[must_use]
#[cfg(test)]
pub fn layout_window_previews(scene: &DockScene, surface: DipRect) -> Vec<WindowPreviewLayout> {
    scene
        .window_previews()
        .iter()
        .copied()
        .map(|visual| WindowPreviewLayout {
            visual,
            bounds: DipRect::new(
                surface.x + 16.0,
                surface.y + 8.0,
                (surface.width - 32.0).clamp(1.0, 260.0),
                (surface.height - 24.0).clamp(1.0, 132.0),
            ),
            kind: match visual.capture() {
                WindowPreviewCapture::DwmThumbnail => WindowPreviewRenderKind::Thumbnail,
                WindowPreviewCapture::Restricted(reason) => {
                    WindowPreviewRenderKind::Unavailable(reason)
                }
            },
        })
        .collect()
}

pub fn dock_scene_width(scene: &DockScene) -> f32 {
    let config = scene.config();
    let item_total = scene
        .items()
        .iter()
        .map(|visual| base_size(visual, config))
        .sum::<f32>();
    let gaps = scene.items().len().saturating_sub(1) as f32 * config.spacing();
    item_total + gaps + config.padding() * 2.0
}

#[must_use]
pub fn dock_scene_max_width(scene: &DockScene) -> f32 {
    let config = scene.config();
    let app_count = scene
        .items()
        .iter()
        .filter(|visual| visual.kind() == DockItemVisualKind::App)
        .count();
    let maximum = config.magnified_item_size().min(config.item_size() * 1.22);
    let maximum_growth = (maximum - config.item_size()).max(0.0);
    dock_scene_width(scene) + maximum_growth * app_count.min(2) as f32
}

#[must_use]
pub fn dock_material_bounds(scene: &DockScene, surface: DipRect) -> DipRect {
    let config = scene.config();
    let layout = layout_dock_scene(scene, surface);
    let visual_width = match (layout.items().first(), layout.items().last()) {
        (Some(first), Some(last)) => {
            last.bounds().x + last.bounds().width - first.bounds().x + config.padding() * 2.0
        }
        _ => config.padding() * 2.0,
    }
    .min(surface.width)
    .max(1.0);
    DipRect::new(
        surface.x + (surface.width - visual_width) / 2.0,
        surface.y,
        visual_width,
        surface.height,
    )
}

fn base_size(visual: &DockItemVisual, config: DockLayoutConfig) -> f32 {
    match visual.kind() {
        DockItemVisualKind::Separator => config.item_size() * (11.0 / 36.0),
        DockItemVisualKind::App => config.item_size(),
    }
}

fn visual_size(
    scene: &DockScene,
    visual: &DockItemVisual,
    resting_bounds: DipRect,
    hover_position_x: Option<f32>,
) -> f32 {
    let config = scene.config();
    if scene.dragged_item().is_some() {
        return base_size(visual, config);
    }
    if visual.kind() == DockItemVisualKind::Separator {
        return base_size(visual, config);
    }
    let Some(pointer_x) = hover_position_x else {
        return config.item_size();
    };
    let radius = (config.item_size() + config.spacing()) * 2.0;
    let distance = (pointer_x - center_x(resting_bounds)).abs();
    let influence = raised_cosine(distance, radius);
    let maximum = config.magnified_item_size().min(config.item_size() * 1.22);
    config.item_size()
        + (maximum - config.item_size()) * influence * smoothstep(scene.hover_strength())
}

fn raised_cosine(distance: f32, radius: f32) -> f32 {
    if distance >= radius || radius <= 0.0 {
        return 0.0;
    }
    0.5 * (1.0 + (std::f32::consts::PI * distance / radius).cos())
}

const fn center_x(bounds: DipRect) -> f32 {
    bounds.x + bounds.width / 2.0
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn contains(bounds: DipRect, point: DipPoint) -> bool {
    point.x >= bounds.x
        && point.x <= bounds.x + bounds.width
        && point.y >= bounds.y
        && point.y <= bounds.y + bounds.height
}
