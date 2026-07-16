use shell_core::{Popover, TopbarModuleKind};

use crate::{DipPoint, DipRect, TopbarDensity, TopbarModuleVisual, TopbarScene};

#[derive(Clone, Debug, PartialEq)]
pub struct TopbarLaidOutItem {
    visual: TopbarModuleVisual,
    bounds: DipRect,
    focused: bool,
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
    pub const fn focused(&self) -> bool {
        self.focused
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

    #[must_use]
    pub fn accessible_name(&self) -> &str {
        self.visual.text()
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
    let metrics = metrics(scene.density(), scene.text_scale());
    let mut left_x = surface.x + metrics.leading_padding;
    let mut items = Vec::new();
    for visual in scene
        .modules()
        .iter()
        .filter(|visual| visual.kind() == TopbarModuleKind::SystemMenu)
    {
        let width = module_width(visual, scene.density(), scene.text_scale());
        items.push(TopbarLaidOutItem {
            visual: visual.clone(),
            bounds: DipRect::new(left_x, surface.y + metrics.y, width, metrics.height),
            focused: scene.focused_module() == Some(visual.kind()),
        });
        left_x += width + metrics.gap;
    }

    let status = scene
        .modules()
        .iter()
        .filter(|visual| visual.kind() != TopbarModuleKind::SystemMenu)
        .collect::<Vec<_>>();
    let status_width = status
        .iter()
        .map(|visual| module_width(visual, scene.density(), scene.text_scale()))
        .sum::<f32>()
        + status.len().saturating_sub(1) as f32 * metrics.gap;
    let right_edge = surface.x + surface.width - metrics.trailing_padding;
    let right_start = right_edge - status_width;
    let min_status_x = left_x + metrics.spacer;
    let mut hidden_count = 0;

    if right_start >= min_status_x {
        let mut x = right_start;
        for visual in status {
            let width = module_width(visual, scene.density(), scene.text_scale());
            items.push(TopbarLaidOutItem {
                visual: visual.clone(),
                bounds: DipRect::new(x, surface.y + metrics.y, width, metrics.height),
                focused: scene.focused_module() == Some(visual.kind()),
            });
            x += width + metrics.gap;
        }
    } else {
        let reserve = metrics.overflow_width + metrics.gap;
        let minimum_candidate = left_x + metrics.spacer + reserve;
        let mut x = right_edge;
        for visual in status.into_iter().rev() {
            let width = module_width(visual, scene.density(), scene.text_scale());
            let candidate = x - width;
            if candidate < minimum_candidate {
                hidden_count += 1;
                continue;
            }
            items.push(TopbarLaidOutItem {
                visual: visual.clone(),
                bounds: DipRect::new(candidate, surface.y + metrics.y, width, metrics.height),
                focused: scene.focused_module() == Some(visual.kind()),
            });
            x = candidate - metrics.gap;
        }
        items.sort_by_key(|item| module_order(scene, item.kind()));
    }

    let overflow = if hidden_count == 0 {
        TopbarOverflow::None
    } else {
        let first_status_x = items
            .iter()
            .filter(|item| item.kind() != TopbarModuleKind::SystemMenu)
            .map(|item| item.bounds().x)
            .reduce(f32::min)
            .unwrap_or(right_edge);
        TopbarOverflow::Collapsed {
            hidden_count,
            bounds: DipRect::new(
                first_status_x - metrics.gap - metrics.overflow_width,
                surface.y + metrics.y,
                metrics.overflow_width,
                metrics.height,
            ),
            intent: Popover::SystemMenu,
        }
    };
    TopbarLayout { items, overflow }
}

fn module_order(scene: &TopbarScene, kind: TopbarModuleKind) -> usize {
    scene
        .modules()
        .iter()
        .position(|visual| visual.kind() == kind)
        .unwrap_or(usize::MAX)
}

fn module_width(visual: &TopbarModuleVisual, density: TopbarDensity, text_scale: f32) -> f32 {
    let base = match density {
        TopbarDensity::Compact => 38.0,
        TopbarDensity::Comfortable => 48.0,
    };
    let text_width = visual.text().chars().count() as f32 * 6.0 * text_scale;
    (base + text_width).clamp(base, 120.0 * text_scale)
}

fn metrics(density: TopbarDensity, text_scale: f32) -> Metrics {
    let item_height = 24.0 * text_scale.clamp(1.0, 2.5);
    match density {
        TopbarDensity::Compact => Metrics {
            leading_padding: 10.0,
            trailing_padding: 20.0,
            gap: 0.0,
            y: 4.0,
            height: item_height,
            overflow_width: 26.0,
            spacer: 40.0,
        },
        TopbarDensity::Comfortable => Metrics {
            leading_padding: 10.0,
            trailing_padding: 20.0,
            gap: 0.0,
            y: 4.0,
            height: item_height,
            overflow_width: 30.0,
            spacer: 40.0,
        },
    }
}

#[derive(Clone, Copy)]
struct Metrics {
    leading_padding: f32,
    trailing_padding: f32,
    gap: f32,
    y: f32,
    height: f32,
    overflow_width: f32,
    spacer: f32,
}

fn contains(bounds: DipRect, point: DipPoint) -> bool {
    point.x >= bounds.x
        && point.x <= bounds.x + bounds.width
        && point.y >= bounds.y
        && point.y <= bounds.y + bounds.height
}
