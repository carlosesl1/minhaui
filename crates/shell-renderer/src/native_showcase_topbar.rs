use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};

use crate::native_showcase_primitives::{draw_text, draw_text_clipped, fill_round, rect};
use crate::native_showcase_resources::ShowcaseFormats;
use crate::{DipRect, TopbarModuleStatus, TopbarOverflow, TopbarScene, layout_topbar_scene};

pub(crate) struct TopbarBrushes<'a> {
    pub hover: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
    pub warning: &'a ID2D1SolidColorBrush,
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
        if item.focused() {
            fill_round(
                context,
                rect(
                    bounds.x,
                    bounds.y,
                    bounds.x + bounds.width,
                    bounds.y + bounds.height,
                    4.0,
                ),
                brushes.hover,
            );
            fill_round(
                context,
                rect(
                    bounds.x + 6.0,
                    bounds.y + bounds.height - 2.0,
                    bounds.x + bounds.width - 6.0,
                    bounds.y + bounds.height,
                    1.0,
                ),
                brushes.accent,
            );
        }
        draw_text(
            context,
            item.icon(),
            formats.icon,
            D2D_RECT_F {
                left: bounds.x + 3.0,
                top: bounds.y,
                right: bounds.x + 23.0,
                bottom: bounds.y + bounds.height,
            },
            status_brush(item.status(), &brushes),
        );
        draw_text_clipped(
            context,
            item.text(),
            formats.text,
            D2D_RECT_F {
                left: bounds.x + 26.0,
                top: bounds.y,
                right: bounds.x + bounds.width - 7.0,
                bottom: bounds.y + bounds.height,
            },
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
