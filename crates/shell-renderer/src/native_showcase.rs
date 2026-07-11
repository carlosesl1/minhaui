use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT, ID2D1DeviceContext, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, IDWriteFactory,
};
use windows::core::{Result, w};

use crate::native::ShowcaseRole;

pub(crate) fn draw_showcase(
    context: &ID2D1DeviceContext,
    dwrite: &IDWriteFactory,
    role: ShowcaseRole,
    width: f32,
    height: f32,
) -> Result<()> {
    // Renderer replacement seam: these primitives deliberately make the native
    // alpha/composition path inspectable before application content is connected.
    let surface = D2D1_COLOR_F {
        r: 0.067,
        g: 0.082,
        b: 0.106,
        a: 0.94,
    };
    let accent = D2D1_COLOR_F {
        r: 0.20,
        g: 0.78,
        b: 0.92,
        a: 1.0,
    };
    let foreground = D2D1_COLOR_F {
        r: 0.91,
        g: 0.94,
        b: 0.97,
        a: 1.0,
    };
    // SAFETY: Category 8 (FFI boundary). Colors are finite and the context owns
    // the returned brushes until these local COM handles are dropped.
    let surface_brush: ID2D1SolidColorBrush =
        unsafe { context.CreateSolidColorBrush(&surface, None) }?;
    // SAFETY: Category 8 (FFI boundary). Same invariant as `surface_brush`.
    let accent_brush: ID2D1SolidColorBrush =
        unsafe { context.CreateSolidColorBrush(&accent, None) }?;
    // SAFETY: Category 8 (FFI boundary). Same invariant as `surface_brush`.
    let foreground_brush: ID2D1SolidColorBrush =
        unsafe { context.CreateSolidColorBrush(&foreground, None) }?;
    // SAFETY: Category 8 (FFI boundary). Font family and locale are static,
    // null-terminated strings and the factory returns an owned COM interface.
    let text_format = unsafe {
        dwrite.CreateTextFormat(
            w!("Segoe UI Variable"),
            None,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            if role == ShowcaseRole::Topbar {
                13.0
            } else {
                15.0
            },
            w!("en-US"),
        )
    }?;
    let radius = if role == ShowcaseRole::Topbar {
        12.0
    } else {
        22.0
    };
    let outer = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: 0.5,
            top: 0.5,
            right: width - 0.5,
            bottom: height - 0.5,
        },
        radiusX: radius,
        radiusY: radius,
    };
    let indicator = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: 16.0,
            top: height / 2.0 - 3.0,
            right: 22.0,
            bottom: height / 2.0 + 3.0,
        },
        radiusX: 3.0,
        radiusY: 3.0,
    };
    // SAFETY: Category 8 (FFI boundary). Draw calls occur only between BeginDraw
    // and EndDraw with a live target bitmap installed by the surface owner.
    unsafe { context.BeginDraw() };
    // SAFETY: Category 8 (FFI boundary). A null clear color is the documented
    // transparent clear operation for the current render target.
    unsafe { context.Clear(None) };
    // SAFETY: Category 8 (FFI boundary). Geometry and brush values are valid for
    // the active target and both brushes remain live for the call.
    unsafe { context.FillRoundedRectangle(&outer, &surface_brush) };
    // SAFETY: Category 8 (FFI boundary). Same active-target invariant as above.
    unsafe { context.FillRoundedRectangle(&indicator, &accent_brush) };
    let label = match role {
        ShowcaseRole::Topbar => "MINHA UI  /  SYSTEM READY",
        ShowcaseRole::Dock => "LAUNCH   SEARCH   SPACES   CONTROL",
    };
    let text = label.encode_utf16().collect::<Vec<_>>();
    let layout = D2D_RECT_F {
        left: 32.0,
        top: 0.0,
        right: width - 16.0,
        bottom: height,
    };
    // SAFETY: Category 8 (FFI boundary). The UTF-16 slice, format, layout, and
    // brush remain live for this synchronous draw within the active draw scope.
    unsafe {
        context.DrawText(
            &text,
            &text_format,
            &layout,
            &foreground_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        )
    };
    // SAFETY: Category 8 (FFI boundary). This pairs the single BeginDraw call and
    // validates the target state through the returned HRESULT.
    unsafe { context.EndDraw(None, None) }
}
