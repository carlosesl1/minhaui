use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

use crate::native_showcase::{draw_text, fill_round, rect};

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_dock_states(
    context: &ID2D1DeviceContext,
    format: &IDWriteTextFormat,
    width: f32,
    raised: &ID2D1SolidColorBrush,
    hover: &ID2D1SolidColorBrush,
    pressed: &ID2D1SolidColorBrush,
    selected: &ID2D1SolidColorBrush,
    primary: &ID2D1SolidColorBrush,
    secondary: &ID2D1SolidColorBrush,
    disabled: &ID2D1SolidColorBrush,
    accent: &ID2D1SolidColorBrush,
    focus: &ID2D1SolidColorBrush,
    error: &ID2D1SolidColorBrush,
    control_radius: f32,
    popover_radius: f32,
) {
    draw_text(
        context,
        "Interaction states",
        format,
        D2D_RECT_F {
            left: 16.0,
            top: 10.0,
            right: 220.0,
            bottom: 34.0,
        },
        secondary,
    );
    let states = [
        ("Rest", raised),
        ("Hover", hover),
        ("Pressed", pressed),
        ("Active", selected),
        ("Focus visible", focus),
        ("Unavailable", raised),
        ("Error", raised),
    ];
    let cell_width = (width - 44.0) / states.len() as f32;
    for (index, (label, brush)) in states.iter().enumerate() {
        let left = 16.0 + index as f32 * (cell_width + 2.0);
        fill_round(
            context,
            rect(left, 34.0, left + cell_width, 78.0, control_radius),
            brush,
        );
        if *label == "Active" {
            fill_round(
                context,
                rect(left + 10.0, 69.0, left + cell_width - 10.0, 72.0, 1.5),
                accent,
            );
        }
        if *label == "Error" {
            fill_round(
                context,
                rect(left + 10.0, 49.0, left + 16.0, 55.0, 3.0),
                error,
            );
        }
        draw_text(
            context,
            label,
            format,
            D2D_RECT_F {
                left: left + 10.0,
                top: 34.0,
                right: left + cell_width - 6.0,
                bottom: 78.0,
            },
            if *label == "Unavailable" {
                disabled
            } else {
                primary
            },
        );
    }
    draw_text(
        context,
        "Representative primitives",
        format,
        D2D_RECT_F {
            left: 16.0,
            top: 86.0,
            right: 260.0,
            bottom: 108.0,
        },
        secondary,
    );
    let items = [
        ("Button / Action", selected),
        ("Slider ---o--", raised),
        ("Device row / Speakers", raised),
        ("Calendar cell / 11", hover),
        ("Popover / Ready", raised),
    ];
    let item_width = (width - 48.0) / items.len() as f32;
    for (index, (label, brush)) in items.iter().enumerate() {
        let left = 16.0 + index as f32 * (item_width + 4.0);
        let radius = if index == items.len() - 1 {
            popover_radius
        } else {
            control_radius
        };
        fill_round(
            context,
            rect(left, 108.0, left + item_width, 164.0, radius),
            brush,
        );
        draw_text(
            context,
            label,
            format,
            D2D_RECT_F {
                left: left + 10.0,
                top: 108.0,
                right: left + item_width - 6.0,
                bottom: 164.0,
            },
            primary,
        );
    }
}
