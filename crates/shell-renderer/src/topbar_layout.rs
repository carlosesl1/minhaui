use shell_core::{TopbarIntent, TopbarModuleKind};

use crate::{DipPoint, DipRect, TopbarDensity, TopbarModuleVisual, TopbarScene};

#[derive(Clone, Debug, PartialEq)]
pub struct TopbarLaidOutItem {
    visual: TopbarModuleVisual,
    bounds: DipRect,
    focused: bool,
    hovered: bool,
    pressed: bool,
    active: bool,
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
    pub const fn hovered(&self) -> bool {
        self.hovered
    }

    #[must_use]
    pub const fn pressed(&self) -> bool {
        self.pressed
    }

    #[must_use]
    pub const fn active(&self) -> bool {
        self.active
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
    pub const fn intent(&self) -> Option<TopbarIntent> {
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
        focused: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopbarLayout {
    items: Vec<TopbarLaidOutItem>,
    hidden_modules: Vec<TopbarModuleVisual>,
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

    /// Modules omitted from the surface, in their configured visual order.
    /// Consumers can expose these exact modules through an overflow affordance
    /// without substituting an unrelated command.
    #[must_use]
    pub fn hidden_modules(&self) -> &[TopbarModuleVisual] {
        &self.hidden_modules
    }

    #[must_use]
    pub const fn overflow_bounds(&self) -> Option<DipRect> {
        match self.overflow {
            TopbarOverflow::None => None,
            TopbarOverflow::Collapsed { bounds, .. } => Some(bounds),
        }
    }

    #[must_use]
    pub fn overflow_at(&self, point: DipPoint) -> bool {
        self.overflow_bounds()
            .is_some_and(|bounds| contains(bounds, point))
    }

    #[must_use]
    pub fn item_at(&self, point: DipPoint) -> Option<&TopbarLaidOutItem> {
        self.items
            .iter()
            .find(|item| contains(item.bounds(), point))
    }

    #[must_use]
    pub fn module_bounds(&self, kind: TopbarModuleKind) -> Option<DipRect> {
        self.items
            .iter()
            .find(|item| item.kind() == kind)
            .map(TopbarLaidOutItem::bounds)
    }

    #[must_use]
    pub fn hit_test(&self, point: DipPoint) -> Option<TopbarIntent> {
        self.item_at(point).and_then(TopbarLaidOutItem::intent)
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
        .filter(|visual| is_leading(visual.kind()))
    {
        let width = module_width(visual, scene.density(), scene.text_scale());
        items.push(TopbarLaidOutItem {
            visual: visual.clone(),
            bounds: DipRect::new(left_x, surface.y + metrics.y, width, metrics.height),
            focused: scene.focused_module() == Some(visual.kind()),
            hovered: scene.hovered_module() == Some(visual.kind()),
            pressed: scene.pressed_module() == Some(visual.kind()),
            active: scene.active_module() == Some(visual.kind()),
        });
        left_x += width + metrics.gap;
    }

    let status = scene
        .modules()
        .iter()
        .filter(|visual| !is_leading(visual.kind()))
        .collect::<Vec<_>>();
    let status_width = status
        .iter()
        .map(|visual| module_width(visual, scene.density(), scene.text_scale()))
        .sum::<f32>()
        + status.len().saturating_sub(1) as f32 * metrics.gap;
    let right_edge = surface.x + surface.width - metrics.trailing_padding;
    let right_start = right_edge - status_width;
    let mut min_status_x = left_x + metrics.spacer;
    let mut hidden_count = 0;
    let mut hidden_modules = Vec::new();

    if !status.is_empty() && right_start < min_status_x {
        left_x = shrink_leading_for_overflow(
            &mut items,
            scene.density(),
            metrics,
            right_edge - metrics.overflow_width - metrics.gap,
        );
        min_status_x = left_x + metrics.spacer;
    }

    if right_start >= min_status_x {
        let mut x = right_start;
        for visual in status {
            let width = module_width(visual, scene.density(), scene.text_scale());
            items.push(TopbarLaidOutItem {
                visual: visual.clone(),
                bounds: DipRect::new(x, surface.y + metrics.y, width, metrics.height),
                focused: scene.focused_module() == Some(visual.kind()),
                hovered: scene.hovered_module() == Some(visual.kind()),
                pressed: scene.pressed_module() == Some(visual.kind()),
                active: scene.active_module() == Some(visual.kind()),
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
                hidden_modules.push(visual.clone());
                continue;
            }
            items.push(TopbarLaidOutItem {
                visual: visual.clone(),
                bounds: DipRect::new(candidate, surface.y + metrics.y, width, metrics.height),
                focused: scene.focused_module() == Some(visual.kind()),
                hovered: scene.hovered_module() == Some(visual.kind()),
                pressed: scene.pressed_module() == Some(visual.kind()),
                active: scene.active_module() == Some(visual.kind()),
            });
            x = candidate - metrics.gap;
        }
        items.sort_by_key(|item| module_order(scene, item.kind()));
        hidden_modules.sort_by_key(|visual| module_order(scene, visual.kind()));
    }

    let leading_end = items
        .iter()
        .filter(|item| is_leading(item.kind()))
        .map(|item| item.bounds().x + item.bounds().width)
        .reduce(f32::max)
        .unwrap_or(surface.x + metrics.leading_padding);
    let overflow = if hidden_count == 0 {
        TopbarOverflow::None
    } else {
        let mut overflow_x =
            first_status_x(&items, right_edge) - metrics.gap - metrics.overflow_width;
        while overflow_x < leading_end {
            let Some((index, _)) = items
                .iter()
                .enumerate()
                .filter(|(_, item)| !is_leading(item.kind()))
                .min_by(|(_, left), (_, right)| left.bounds().x.total_cmp(&right.bounds().x))
            else {
                break;
            };
            let removed = items.remove(index);
            hidden_modules.push(removed.visual);
            hidden_count += 1;
            overflow_x = first_status_x(&items, right_edge) - metrics.gap - metrics.overflow_width;
        }
        hidden_modules.sort_by_key(|visual| module_order(scene, visual.kind()));
        overflow_x = overflow_x
            .max(leading_end)
            .min(right_edge - metrics.overflow_width);
        TopbarOverflow::Collapsed {
            hidden_count,
            bounds: DipRect::new(
                overflow_x,
                surface.y + metrics.y,
                metrics.overflow_width,
                metrics.height,
            ),
            focused: scene.focused_overflow(),
        }
    };
    TopbarLayout {
        items,
        hidden_modules,
        overflow,
    }
}

fn first_status_x(items: &[TopbarLaidOutItem], right_edge: f32) -> f32 {
    items
        .iter()
        .filter(|item| !is_leading(item.kind()))
        .map(|item| item.bounds().x)
        .reduce(f32::min)
        .unwrap_or(right_edge)
}

const fn is_leading(kind: TopbarModuleKind) -> bool {
    matches!(
        kind,
        TopbarModuleKind::SystemMenu | TopbarModuleKind::AppIdentity | TopbarModuleKind::Search
    )
}

fn module_order(scene: &TopbarScene, kind: TopbarModuleKind) -> usize {
    scene
        .modules()
        .iter()
        .position(|visual| visual.kind() == kind)
        .unwrap_or(usize::MAX)
}

fn module_width(visual: &TopbarModuleVisual, density: TopbarDensity, text_scale: f32) -> f32 {
    let base = minimum_module_width(density);
    let text_width = visual.text().chars().count() as f32 * 6.0 * text_scale;
    (base + text_width).clamp(base, 120.0 * text_scale)
}

fn shrink_leading_for_overflow(
    items: &mut [TopbarLaidOutItem],
    density: TopbarDensity,
    metrics: Metrics,
    target_end: f32,
) -> f32 {
    let current_end = items
        .iter()
        .filter(|item| is_leading(item.kind()))
        .map(|item| item.bounds.x + item.bounds.width)
        .reduce(f32::max)
        .unwrap_or(target_end);
    let mut deficit = (current_end - target_end).max(0.0);
    let minimum = minimum_module_width(density);

    for kind in [
        TopbarModuleKind::AppIdentity,
        TopbarModuleKind::Search,
        TopbarModuleKind::SystemMenu,
    ] {
        let Some(item) = items.iter_mut().find(|item| item.kind() == kind) else {
            continue;
        };
        let reduction = deficit.min((item.bounds.width - minimum).max(0.0));
        item.bounds.width -= reduction;
        deficit -= reduction;
        if deficit <= f32::EPSILON {
            break;
        }
    }

    let mut x = items
        .iter()
        .find(|item| is_leading(item.kind()))
        .map(|item| item.bounds.x)
        .unwrap_or(target_end);
    for item in items.iter_mut().filter(|item| is_leading(item.kind())) {
        item.bounds.x = x;
        x += item.bounds.width + metrics.gap;
    }
    x
}

const fn minimum_module_width(density: TopbarDensity) -> f32 {
    match density {
        TopbarDensity::Compact => 38.0,
        TopbarDensity::Comfortable => 48.0,
    }
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
