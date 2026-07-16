use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_DRAW_TEXT_OPTIONS_NONE,
    D2D1_ROUNDED_RECT, ID2D1Brush, ID2D1DeviceContext, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{DWRITE_MEASURING_MODE_NATURAL, IDWriteTextFormat};

pub(crate) fn fill_round(
    context: &ID2D1DeviceContext,
    geometry: D2D1_ROUNDED_RECT,
    brush: &ID2D1Brush,
) {
    // SAFETY: Category 8 (FFI boundary). Geometry is finite and the brush is live
    // for the synchronous call inside the active draw scope.
    unsafe { context.FillRoundedRectangle(&geometry, brush) };
}

pub(crate) fn draw_text(
    context: &ID2D1DeviceContext,
    text: &str,
    format: &IDWriteTextFormat,
    layout: D2D_RECT_F,
    brush: &ID2D1SolidColorBrush,
) {
    draw_text_with_options(
        context,
        text,
        format,
        layout,
        brush,
        D2D1_DRAW_TEXT_OPTIONS_NONE,
    );
}

pub(crate) fn draw_text_clipped(
    context: &ID2D1DeviceContext,
    text: &str,
    format: &IDWriteTextFormat,
    layout: D2D_RECT_F,
    brush: &ID2D1SolidColorBrush,
) {
    draw_text_with_options(
        context,
        text,
        format,
        layout,
        brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP,
    );
}

fn draw_text_with_options(
    context: &ID2D1DeviceContext,
    text: &str,
    format: &IDWriteTextFormat,
    layout: D2D_RECT_F,
    brush: &ID2D1SolidColorBrush,
    options: D2D1_DRAW_TEXT_OPTIONS,
) {
    let text = text.encode_utf16().collect::<Vec<_>>();
    // SAFETY: Category 8 (FFI boundary). Text storage, layout, format, and brush
    // remain live for the synchronous draw call.
    unsafe {
        context.DrawText(
            &text,
            format,
            &layout,
            brush,
            options,
            DWRITE_MEASURING_MODE_NATURAL,
        )
    };
}

pub(crate) const fn rect(
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left,
            top,
            right,
            bottom,
        },
        radiusX: radius,
        radiusY: radius,
    }
}
