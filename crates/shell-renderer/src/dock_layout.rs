use crate::{
    DipPoint, DipRect, DockItemVisual, DockItemVisualKind, DockLayoutConfig, DockScene,
    PreviewUnavailableReason, WindowPreviewCapture, WindowPreviewVisual,
};

#[derive(Clone, Debug, PartialEq)]
pub struct DockLaidOutItem {
    visual: DockItemVisual,
    bounds: DipRect,
    focused: bool,
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
    pub const fn bounds(&self) -> DipRect {
        self.bounds
    }

    #[must_use]
    pub const fn focused(&self) -> bool {
        self.focused
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
pub enum WindowPreviewRenderKind {
    Thumbnail,
    Unavailable(PreviewUnavailableReason),
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
            .find(|item| contains(item.bounds(), point))
            .map(DockLaidOutItem::id)
    }
}

impl WindowPreviewLayout {
    #[must_use]
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
    let visual_width = total_width(scene);
    let mut x = match config.alignment() {
        crate::DockAlignment::Left => surface.x + config.padding(),
        crate::DockAlignment::Center => surface.x + (surface.width - visual_width) / 2.0,
        crate::DockAlignment::Right => surface.x + surface.width - visual_width - config.padding(),
    };
    let items = scene
        .items()
        .iter()
        .map(|visual| {
            let size = visual_size(visual, scene.hovered_item(), config);
            let y = surface.y + (surface.height - size) / 2.0;
            let bounds = DipRect::new(x, y, size, size);
            x += size + config.spacing();
            DockLaidOutItem {
                visual: visual.clone(),
                bounds,
                focused: scene.focused_item() == Some(visual.id()),
            }
        })
        .collect();
    DockLayout { items }
}

#[must_use]
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

fn total_width(scene: &DockScene) -> f32 {
    let config = scene.config();
    let item_total = scene
        .items()
        .iter()
        .map(|visual| visual_size(visual, scene.hovered_item(), config))
        .sum::<f32>();
    let gaps = scene.items().len().saturating_sub(1) as f32 * config.spacing();
    item_total + gaps + config.padding() * 2.0
}

fn visual_size(
    visual: &DockItemVisual,
    hovered_item: Option<u64>,
    config: DockLayoutConfig,
) -> f32 {
    match visual.kind() {
        DockItemVisualKind::Separator => config.item_size() * 0.28,
        DockItemVisualKind::App if hovered_item == Some(visual.id()) => {
            config.magnified_item_size()
        }
        DockItemVisualKind::App => config.item_size(),
    }
}

fn contains(bounds: DipRect, point: DipPoint) -> bool {
    point.x >= bounds.x
        && point.x <= bounds.x + bounds.width
        && point.y >= bounds.y
        && point.y <= bounds.y + bounds.height
}
