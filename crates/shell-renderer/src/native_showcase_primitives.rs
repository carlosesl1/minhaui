use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_DRAW_TEXT_OPTIONS_NONE,
    D2D1_ROUNDED_RECT, ID2D1Brush, ID2D1DeviceContext, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{DWRITE_MEASURING_MODE_NATURAL, IDWriteTextFormat};
use windows::core::Error;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct D2DPoint2F {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

// windows 0.62 projects D2D_POINT_2F as a transitive windows-numerics type,
// so keep the native typedef name at this FFI boundary without a new dependency.
use D2DPoint2F as D2D_POINT_2F;

pub(crate) fn fill_round(
    context: &ID2D1DeviceContext,
    geometry: D2D1_ROUNDED_RECT,
    brush: &ID2D1Brush,
) {
    // SAFETY: Category 8 (FFI boundary). Geometry is finite and the brush is live
    // for the synchronous call inside the active draw scope.
    unsafe { context.FillRoundedRectangle(&geometry, brush) };
}

#[allow(
    clippy::missing_transmute_annotations,
    reason = "windows projects the destination as a transitive windows-numerics type; the repr(C) source is asserted at this isolated FFI boundary"
)]
pub(crate) fn fill_triangle(
    context: &ID2D1DeviceContext,
    points: [D2D_POINT_2F; 3],
    brush: &ID2D1Brush,
) -> windows::core::Result<()> {
    // SAFETY: Category 8 (FFI boundary). The context and brush remain live for
    // these synchronous calls. D2DPoint2F is repr(C) and layout-compatible with
    // the generated Direct2D Vector2 parameter; geometry and sink are kept live
    // until the closed path has been filled.
    unsafe {
        let factory = context
            .GetFactory()
            .map_err(|_| Error::from_hresult(E_FAIL))?;
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;
        sink.BeginFigure(
            core::mem::transmute::<D2DPoint2F, _>(points[0]),
            D2D1_FIGURE_BEGIN_FILLED,
        );
        sink.AddLine(core::mem::transmute::<D2DPoint2F, _>(points[1]));
        sink.AddLine(core::mem::transmute::<D2DPoint2F, _>(points[2]));
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        context.FillGeometry(&geometry, brush, None::<&ID2D1Brush>);
    }
    Ok(())
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
