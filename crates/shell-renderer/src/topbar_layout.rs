use shell_core::{Popover, TopbarModuleKind};

use crate::{DipPoint, DipRect, TopbarDensity, TopbarModuleVisual, TopbarScene};

#[derive(Clone, Debug, PartialEq)]
pub struct TopbarLaidOutItem {
    visual: TopbarModuleVisual,
    bounds: DipRect,
}

impl TopbarLaidOutItem {
    #[must_use]
    pub const fn kind(&self) -> TopbarModuleKind {
        self.visual.kind()
    }

    #[must_use]
    pub const fn bounds(&self) -> DipRect {
        self.bounds
    }

    #[must_use]
    pub fn icon(&self) -> &str {
        self.visual.icon()
    }

    #[must_use]
    pub fn text(&self) -> &str {
        self.visual.text()
    }

    #[must_use]
    pub const fn status(&self) -> crate::TopbarModuleStatus {
        self.visual.status()
    }

    #[must_use]
    pub const fn intent(&self) -> Option<Popover> {
        self.visual.intent()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TopbarOverflow {
    None,
    Collapsed {
        hidden_count: usize,
        bounds: DipRect,
        intent: Popover,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopbarLayout {
    items: Vec<TopbarLaidOutItem>,
    overflow: TopbarOverflow,
}

impl TopbarLayout {
    #[must_use]
    pub fn visible_items(&self) -> &[TopbarLaidOutItem] {
        &self.items
    }

    #[must_use]
    pub const fn overflow(&self) -> TopbarOverflow {
        self.overflow
    }

    #[must_use]
    pub fn hit_test(&self, point: DipPoint) -> Option<Popover> {
        self.items
            .iter()
            .find(|item| contains(item.bounds(), point))
            .and_then(TopbarLaidOutItem::intent)
            .or_else(|| match self.overflow {
                TopbarOverflow::None => None,
                TopbarOverflow::Collapsed { bounds, intent, .. } if contains(bounds, point) => {
                    Some(intent)
                }
                TopbarOverflow::Collapsed { .. } => None,
            })
    }
}

#[must_use]
pub fn layout_topbar_scene(scene: &TopbarScene, surface: DipRect) -> TopbarLayout {
    let metrics = metrics(scene.density());
    let mut x = surface.x + metrics.padding;
    let limit = surface.x + surface.width - metrics.padding - metrics.overflow_reserve;
    let mut items = Vec::new();
    let mut hidden_count = 0;
    for visual in scene.modules() {
        let width = module_width(visual, scene.density());
        if x + width > limit {
            hidden_count += 1;
            continue;
        }
        items.push(TopbarLaidOutItem {
            visual: visual.clone(),
            bounds: DipRect::new(x, surface.y + metrics.y, width, metrics.height),
        });
        x += width + metrics.gap;
    }
    let overflow = if hidden_count == 0 {
        TopbarOverflow::None
    } else {
        TopbarOverflow::Collapsed {
            hidden_count,
            bounds: DipRect::new(
                surface.x + surface.width - metrics.padding - metrics.overflow_width,
                surface.y + metrics.y,
                metrics.overflow_width,
                metrics.height,
            ),
            intent: Popover::SystemMenu,
        }
    };
    TopbarLayout { items, overflow }
}

fn module_width(visual: &TopbarModuleVisual, density: TopbarDensity) -> f32 {
    let base = match density {
        TopbarDensity::Compact => 38.0,
        TopbarDensity::Comfortable => 48.0,
    };
    let text_width = visual.text().chars().count() as f32 * 7.0;
    (base + text_width).clamp(base, 148.0)
}

const fn metrics(density: TopbarDensity) -> Metrics {
    match density {
        TopbarDensity::Compact => Metrics {
            padding: 8.0,
            gap: 4.0,
            y: 4.0,
            height: 24.0,
            overflow_reserve: 28.0,
            overflow_width: 26.0,
        },
        TopbarDensity::Comfortable => Metrics {
            padding: 10.0,
            gap: 6.0,
            y: 5.0,
            height: 30.0,
            overflow_reserve: 34.0,
            overflow_width: 30.0,
        },
    }
}

#[derive(Clone, Copy)]
struct Metrics {
    padding: f32,
    gap: f32,
    y: f32,
    height: f32,
    overflow_reserve: f32,
    overflow_width: f32,
}

fn contains(bounds: DipRect, point: DipPoint) -> bool {
    point.x >= bounds.x
        && point.x <= bounds.x + bounds.width
        && point.y >= bounds.y
        && point.y <= bounds.y + bounds.height
}
