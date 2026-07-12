use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

use crate::native_showcase::{draw_text, fill_round, rect};
use crate::{DipRect, TopbarModuleStatus, TopbarOverflow, TopbarScene, layout_topbar_scene};

pub(crate) struct TopbarBrushes<'a> {
    pub raised: &'a ID2D1SolidColorBrush,
    pub hover: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
    pub error: &'a ID2D1SolidColorBrush,
}

pub(crate) fn draw_functional_topbar(
    context: &ID2D1DeviceContext,
    format: &IDWriteTextFormat,
    width: f32,
    height: f32,
    scene: &TopbarScene,
    brushes: TopbarBrushes<'_>,
) {
    let layout = layout_topbar_scene(scene, DipRect::new(0.0, 0.0, width, height));
    for item in layout.visible_items() {
        let bounds = item.bounds();
        if item.focused() {
            fill_round(
                context,
                rect(
                    bounds.x - 3.0,
                    bounds.y - 3.0,
                    bounds.x + bounds.width + 3.0,
                    bounds.y + bounds.height + 3.0,
                    10.0,
                ),
                brushes.accent,
            );
        }
        fill_round(
            context,
            rect(
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
                8.0,
            ),
            brushes.raised,
        );
        fill_round(
            context,
            rect(
                bounds.x + 8.0,
                bounds.y + 9.0,
                bounds.x + 14.0,
                bounds.y + 15.0,
                3.0,
            ),
            status_brush(item.status(), &brushes),
        );
        draw_text(
            context,
            item.text(),
            format,
            D2D_RECT_F {
                left: bounds.x + 19.0,
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
            format,
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
        TopbarModuleStatus::Warning => brushes.error,
    }
}
