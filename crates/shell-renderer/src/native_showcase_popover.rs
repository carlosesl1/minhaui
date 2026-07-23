use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

use crate::native_icons::NativeIconCache;
use crate::native_showcase_primitives::{draw_text, fill_round, rect};
use crate::{DipRect, PopoverContentState, PopoverScene, layout_popover_scene};

pub(crate) struct PopoverBrushes<'a> {
    pub hover: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
    pub error: &'a ID2D1SolidColorBrush,
}

pub(crate) struct PopoverFormats<'a> {
    pub label: &'a IDWriteTextFormat,
    pub detail: &'a IDWriteTextFormat,
    pub icon: &'a IDWriteTextFormat,
}

pub(crate) fn draw_functional_popover(
    context: &ID2D1DeviceContext,
    icons: &mut NativeIconCache,
    formats: PopoverFormats<'_>,
    surface: DipRect,
    scene: &PopoverScene,
    brushes: PopoverBrushes<'_>,
) {
    let width = surface.width;
    let height = surface.height;
    let layout = layout_popover_scene(scene, DipRect::new(0.0, 0.0, width, height));
    if layout.rows().is_empty() {
        draw_text(
            context,
            scene.status_text(),
            formats.label,
            D2D_RECT_F {
                left: 16.0,
                top: 5.0,
                right: width - 16.0,
                bottom: height - 5.0,
            },
            status_brush(scene.state(), &brushes),
        );
    }
    for item in layout.rows() {
        let row = &scene.rows()[item.index()];
        let bounds = item.bounds();
        if item.focused() {
            fill_round(
                context,
                rect(
                    bounds.x - 7.0,
                    bounds.y,
                    bounds.x + bounds.width + 7.0,
                    bounds.y + bounds.height,
                    6.0,
                ),
                brushes.hover,
            );
        }
        let label_left = if let Some(source) = row.icon_source() {
            let icon_bounds = D2D_RECT_F {
                left: bounds.x,
                top: bounds.y + 3.0,
                right: bounds.x + 18.0,
                bottom: bounds.y + 21.0,
            };
            if !icons.draw(source, icon_bounds) {
                draw_text(
                    context,
                    "\u{ECAA}",
                    formats.icon,
                    icon_bounds,
                    brushes.primary,
                );
            }
            bounds.x + 26.0
        } else {
            bounds.x
        };
        draw_text(
            context,
            row.label(),
            formats.label,
            D2D_RECT_F {
                left: label_left,
                top: bounds.y,
                right: bounds.x + bounds.width * 0.55,
                bottom: bounds.y + bounds.height,
            },
            if row.enabled() {
                brushes.primary
            } else {
                brushes.secondary
            },
        );
        draw_text(
            context,
            row.detail(),
            formats.detail,
            D2D_RECT_F {
                left: bounds.x + bounds.width * 0.55,
                top: bounds.y,
                right: bounds.x + bounds.width,
                bottom: bounds.y + bounds.height,
            },
            brushes.secondary,
        );
    }
}

fn status_brush<'a>(
    state: PopoverContentState,
    brushes: &'a PopoverBrushes<'a>,
) -> &'a ID2D1SolidColorBrush {
    match state {
        PopoverContentState::Loading | PopoverContentState::Ready => brushes.accent,
        PopoverContentState::Empty | PopoverContentState::Offline => brushes.secondary,
        PopoverContentState::Error => brushes.error,
    }
}
