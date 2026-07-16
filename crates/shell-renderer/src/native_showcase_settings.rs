use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

use crate::SettingsScene;
use crate::native_showcase_primitives::{draw_text, fill_round, rect};

pub(crate) struct SettingsBrushes<'a> {
    pub raised: &'a ID2D1SolidColorBrush,
    pub hover: &'a ID2D1SolidColorBrush,
    pub selected: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
}

pub(crate) fn draw_functional_settings(
    context: &ID2D1DeviceContext,
    format: &IDWriteTextFormat,
    width: f32,
    scene: &SettingsScene,
    brushes: SettingsBrushes<'_>,
) {
    draw_text(
        context,
        scene.title(),
        format,
        D2D_RECT_F {
            left: 24.0,
            top: 16.0,
            right: width - 24.0,
            bottom: 48.0,
        },
        brushes.primary,
    );
    fill_round(
        context,
        rect(24.0, 58.0, 224.0, 492.0, 10.0),
        brushes.raised,
    );
    fill_round(
        context,
        rect(244.0, 58.0, width - 24.0, 492.0, 10.0),
        brushes.raised,
    );
    for (index, row) in scene.rows().iter().enumerate() {
        let top = 70.0 + index as f32 * 38.0;
        let focused = scene.focused() == Some(index);
        fill_round(
            context,
            rect(36.0, top, 212.0, top + 30.0, 6.0),
            if focused {
                brushes.selected
            } else {
                brushes.hover
            },
        );
        if focused {
            fill_round(
                context,
                rect(42.0, top + 7.0, 46.0, top + 23.0, 2.0),
                brushes.accent,
            );
        }
        draw_text(
            context,
            row.label(),
            format,
            D2D_RECT_F {
                left: 54.0,
                top,
                right: 204.0,
                bottom: top + 30.0,
            },
            brushes.primary,
        );
        if index < 6 {
            draw_text(
                context,
                row.detail(),
                format,
                D2D_RECT_F {
                    left: 264.0,
                    top: 76.0 + index as f32 * 56.0,
                    right: width - 44.0,
                    bottom: 112.0 + index as f32 * 56.0,
                },
                brushes.secondary,
            );
        }
    }
}
