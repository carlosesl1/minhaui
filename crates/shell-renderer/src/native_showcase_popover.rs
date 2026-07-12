use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

use crate::native_showcase::{draw_text, fill_round, rect};
use crate::{DipRect, PopoverContentState, PopoverScene, layout_popover_scene};

pub(crate) struct PopoverBrushes<'a> {
    pub raised: &'a ID2D1SolidColorBrush,
    pub hover: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
    pub error: &'a ID2D1SolidColorBrush,
}

pub(crate) fn draw_functional_popover(
    context: &ID2D1DeviceContext,
    format: &IDWriteTextFormat,
    width: f32,
    height: f32,
    scene: &PopoverScene,
    brushes: PopoverBrushes<'_>,
) {
    draw_text(
        context,
        scene.title(),
        format,
        D2D_RECT_F {
            left: 16.0,
            top: 10.0,
            right: width - 16.0,
            bottom: 36.0,
        },
        brushes.primary,
    );
    let status = status_text(scene.state());
    draw_text(
        context,
        status,
        format,
        D2D_RECT_F {
            left: 16.0,
            top: height - 30.0,
            right: width - 16.0,
            bottom: height - 8.0,
        },
        status_brush(scene.state(), &brushes),
    );
    let layout = layout_popover_scene(scene, DipRect::new(0.0, 0.0, width, height));
    for item in layout.rows() {
        let row = &scene.rows()[item.index()];
        let bounds = item.bounds();
        fill_round(
            context,
            rect(
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
                7.0,
            ),
            if item.focused() {
                brushes.hover
            } else {
                brushes.raised
            },
        );
        draw_text(
            context,
            row.label(),
            format,
            D2D_RECT_F {
                left: bounds.x + 10.0,
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
            format,
            D2D_RECT_F {
                left: bounds.x + bounds.width * 0.55,
                top: bounds.y,
                right: bounds.x + bounds.width - 10.0,
                bottom: bounds.y + bounds.height,
            },
            brushes.secondary,
        );
    }
}

const fn status_text(state: PopoverContentState) -> &'static str {
    match state {
        PopoverContentState::Loading => "Loading",
        PopoverContentState::Ready => "Ready",
        PopoverContentState::Empty => "Empty",
        PopoverContentState::Error => "Unavailable",
        PopoverContentState::Offline => "Offline",
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
