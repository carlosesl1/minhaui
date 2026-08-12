use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};

use crate::native_liquid_glass::LiquidGlassResources;
use crate::native_showcase_primitives::{draw_text, draw_text_clipped, fill_round, rect};
use crate::native_showcase_resources::ShowcaseFormats;
use crate::{DipRect, TopbarModuleStatus, TopbarOverflow, TopbarScene, layout_topbar_scene};

pub(crate) struct TopbarBrushes<'a> {
    pub hover: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
    pub focus: &'a ID2D1SolidColorBrush,
    pub warning: &'a ID2D1SolidColorBrush,
    pub contrast_shadow: &'a ID2D1SolidColorBrush,
    pub liquid_glass: Option<&'a LiquidGlassResources>,
}

pub(crate) fn draw_functional_topbar(
    context: &ID2D1DeviceContext,
    formats: ShowcaseFormats<'_>,
    surface: DipRect,
    scene: &TopbarScene,
    brushes: TopbarBrushes<'_>,
) {
    let layout = layout_topbar_scene(scene, surface);
    for item in layout.visible_items() {
        let bounds = item.bounds();
        let plan = topbar_item_render_plan(
            item.focused(),
            item.hovered(),
            item.pressed(),
            item.active(),
        );
        if plan.draw_hover {
            fill_round(
                context,
                rect(
                    bounds.x,
                    bounds.y,
                    bounds.x + bounds.width,
                    bounds.y + bounds.height,
                    8.0,
                ),
                brushes.hover,
            );
        }
        if plan.draw_liquid_glass
            && let Some(liquid_glass) = brushes.liquid_glass
        {
            liquid_glass.draw(context, bounds, None, 0.0);
        }
        if plan.draw_focus_indicator {
            fill_round(
                context,
                rect(
                    bounds.x + 6.0,
                    bounds.y + bounds.height - 2.0,
                    bounds.x + bounds.width - 6.0,
                    bounds.y + bounds.height - 1.0,
                    0.5,
                ),
                brushes.focus,
            );
        }
        let icon_bounds = D2D_RECT_F {
            left: bounds.x + 3.0,
            top: bounds.y,
            right: bounds.x + 23.0,
            bottom: bounds.y + bounds.height,
        };
        let text_bounds = D2D_RECT_F {
            left: bounds.x + 26.0,
            top: bounds.y,
            right: bounds.x + bounds.width - 7.0,
            bottom: bounds.y + bounds.height,
        };
        if plan.content_passes == 2 {
            draw_text(
                context,
                item.icon(),
                formats.icon,
                offset_y(icon_bounds, 1.0),
                brushes.contrast_shadow,
            );
            draw_text_clipped(
                context,
                item.text(),
                formats.text,
                offset_y(text_bounds, 1.0),
                brushes.contrast_shadow,
            );
        }
        draw_text(
            context,
            item.icon(),
            formats.icon,
            icon_bounds,
            status_brush(item.status(), &brushes),
        );
        draw_text_clipped(
            context,
            item.text(),
            formats.text,
            text_bounds,
            brushes.primary,
        );
    }
    if let TopbarOverflow::Collapsed {
        hidden_count,
        bounds,
        ..
    } = layout.overflow()
    {
        fill_round(
            context,
            rect(
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
                8.0,
            ),
            brushes.hover,
        );
        draw_text(
            context,
            &format!("+{hidden_count}"),
            formats.text,
            D2D_RECT_F {
                left: bounds.x + 7.0,
                top: bounds.y,
                right: bounds.x + bounds.width,
                bottom: bounds.y + bounds.height,
            },
            brushes.secondary,
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TopbarItemRenderPlan {
    draw_hover: bool,
    draw_liquid_glass: bool,
    draw_focus_indicator: bool,
    content_passes: usize,
}

const fn topbar_item_render_plan(
    focused: bool,
    hovered: bool,
    pressed: bool,
    active: bool,
) -> TopbarItemRenderPlan {
    TopbarItemRenderPlan {
        draw_hover: hovered && !pressed && !active,
        draw_liquid_glass: pressed || active,
        draw_focus_indicator: focused,
        content_passes: 2,
    }
}

const fn offset_y(bounds: D2D_RECT_F, offset: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: bounds.left,
        top: bounds.top + offset,
        right: bounds.right,
        bottom: bounds.bottom + offset,
    }
}

fn status_brush<'a>(
    status: TopbarModuleStatus,
    brushes: &'a TopbarBrushes<'a>,
) -> &'a ID2D1SolidColorBrush {
    match status {
        TopbarModuleStatus::Neutral => brushes.secondary,
        TopbarModuleStatus::Good => brushes.accent,
        TopbarModuleStatus::Warning => brushes.warning,
    }
}

#[cfg(test)]
mod tests {
    use super::topbar_item_render_plan;

    #[test]
    fn topbar_item_render_plan_separates_focus_from_liquid_glass_and_protects_content() {
        let focused = topbar_item_render_plan(true, false, false, false);
        assert!(focused.draw_focus_indicator);
        assert!(!focused.draw_hover);
        assert!(!focused.draw_liquid_glass);
        assert_eq!(focused.content_passes, 2);

        let hovered = topbar_item_render_plan(false, true, false, false);
        assert!(hovered.draw_hover);
        assert!(!hovered.draw_liquid_glass);

        let pressed = topbar_item_render_plan(false, true, true, false);
        assert!(!pressed.draw_focus_indicator);
        assert!(!pressed.draw_hover);
        assert!(pressed.draw_liquid_glass);
        assert_eq!(pressed.content_passes, 2);

        let active = topbar_item_render_plan(false, true, false, true);
        assert!(!active.draw_focus_indicator);
        assert!(!active.draw_hover);
        assert!(active.draw_liquid_glass);
        assert_eq!(active.content_passes, 2);
    }
}
