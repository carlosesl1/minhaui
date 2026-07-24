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
    if let Some(controls) = scene.quick_controls() {
        draw_text(
            context,
            "Quick Controls",
            format,
            D2D_RECT_F {
                left: 264.0,
                top: 70.0,
                right: width - 44.0,
                bottom: 102.0,
            },
            brushes.primary,
        );
        draw_text(
            context,
            "Only controls supported by this PC are shown. Space toggles visibility; Left/Right changes order.",
            format,
            D2D_RECT_F {
                left: 264.0,
                top: 104.0,
                right: width - 44.0,
                bottom: 146.0,
            },
            brushes.secondary,
        );
        if controls.is_empty() {
            draw_text(
                context,
                "No compatible controls are currently available.",
                format,
                D2D_RECT_F {
                    left: 264.0,
                    top: 164.0,
                    right: width - 44.0,
                    bottom: 206.0,
                },
                brushes.secondary,
            );
        }
        for (index, control) in controls.iter().enumerate() {
            let top = 154.0 + index as f32 * 38.0;
            fill_round(
                context,
                rect(264.0, top, width - 44.0, top + 32.0, 7.0),
                if scene.focused() == Some(index) {
                    brushes.selected
                } else {
                    brushes.hover
                },
            );
            fill_round(
                context,
                rect(276.0, top + 7.0, 294.0, top + 25.0, 5.0),
                if control.visible() {
                    brushes.accent
                } else {
                    brushes.raised
                },
            );
            draw_text(
                context,
                if control.visible() { "✓" } else { "" },
                format,
                D2D_RECT_F {
                    left: 276.0,
                    top: top + 4.0,
                    right: 294.0,
                    bottom: top + 28.0,
                },
                brushes.primary,
            );
            draw_text(
                context,
                control.label(),
                format,
                D2D_RECT_F {
                    left: 306.0,
                    top,
                    right: width - 110.0,
                    bottom: top + 32.0,
                },
                brushes.primary,
            );
            draw_text(
                context,
                match (control.can_move_up(), control.can_move_down()) {
                    (true, true) => "‹  ›",
                    (true, false) => "‹",
                    (false, true) => "›",
                    (false, false) => "",
                },
                format,
                D2D_RECT_F {
                    left: width - 104.0,
                    top,
                    right: width - 54.0,
                    bottom: top + 32.0,
                },
                brushes.secondary,
            );
        }
        return;
    }
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
