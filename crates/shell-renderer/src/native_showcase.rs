use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT, ID2D1DeviceContext, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, IDWriteFactory, IDWriteTextFormat,
};
use windows::core::{Result, w};

use crate::native::{ShellScenes, ShowcaseRole};
use crate::native_showcase_dock::{draw_dock_states, draw_functional_dock};
use crate::native_showcase_popover::{PopoverBrushes, draw_functional_popover};
use crate::native_showcase_settings::{SettingsBrushes, draw_functional_settings};
use crate::native_showcase_topbar::{TopbarBrushes, draw_functional_topbar};
use crate::{Rgba8, ShowcaseTokens};

pub(crate) fn draw_showcase(
    context: &ID2D1DeviceContext,
    dwrite: &IDWriteFactory,
    role: ShowcaseRole,
    width: f32,
    height: f32,
    scenes: ShellScenes<'_>,
) -> Result<()> {
    let tokens = ShowcaseTokens::obsidian_glass();
    let base = create_brush(context, tokens.surface_base)?;
    let raised = create_brush(context, tokens.surface_raised)?;
    let hover = create_brush(context, tokens.surface_hover)?;
    let pressed = create_brush(context, tokens.surface_pressed)?;
    let selected = create_brush(context, tokens.surface_selected)?;
    let primary = create_brush(context, tokens.text_primary)?;
    let secondary = create_brush(context, tokens.text_secondary)?;
    let disabled = create_brush(context, tokens.text_disabled)?;
    let accent = create_brush(context, tokens.accent)?;
    let focus = create_brush(context, tokens.focus_outer)?;
    let error = create_brush(context, tokens.error)?;
    let text_format = create_text_format(dwrite, role)?;
    let radius = match role {
        ShowcaseRole::Dock => tokens.dock_radius,
        ShowcaseRole::Topbar => 12.0,
        ShowcaseRole::Popover => tokens.popover_radius,
        ShowcaseRole::Settings => tokens.popover_radius,
    };
    // SAFETY: Category 8 (FFI boundary). A live target is installed and all draw
    // calls finish before the owned brushes are dropped.
    unsafe { context.BeginDraw() };
    // SAFETY: Category 8 (FFI boundary). Null clears the target to transparent.
    unsafe { context.Clear(None) };
    fill_round(
        context,
        rect(0.5, 0.5, width - 0.5, height - 0.5, radius),
        &base,
    );
    if role == ShowcaseRole::Topbar {
        if let Some(scene) = scenes.topbar {
            draw_functional_topbar(
                context,
                &text_format,
                width,
                height,
                scene,
                TopbarBrushes {
                    raised: &raised,
                    hover: &hover,
                    primary: &primary,
                    secondary: &secondary,
                    accent: &accent,
                    error: &error,
                },
            );
        } else {
            fill_round(context, rect(16.0, 13.0, 22.0, 19.0, 3.0), &accent);
            draw_text(
                context,
                "Obsidian Glass  /  Native shell  /  Resource ready",
                &text_format,
                D2D_RECT_F {
                    left: 32.0,
                    top: 0.0,
                    right: width - 16.0,
                    bottom: height,
                },
                &primary,
            );
        }
    } else if role == ShowcaseRole::Popover {
        if let Some(scene) = scenes.popover {
            draw_functional_popover(
                context,
                &text_format,
                width,
                height,
                scene,
                PopoverBrushes {
                    raised: &raised,
                    hover: &hover,
                    primary: &primary,
                    secondary: &secondary,
                    accent: &accent,
                    error: &error,
                },
            );
        }
    } else if role == ShowcaseRole::Settings {
        if let Some(scene) = scenes.settings {
            draw_functional_settings(
                context,
                &text_format,
                width,
                scene,
                SettingsBrushes {
                    raised: &raised,
                    hover: &hover,
                    selected: &selected,
                    primary: &primary,
                    secondary: &secondary,
                    accent: &accent,
                },
            );
        }
    } else if let Some(scene) = scenes.dock {
        draw_functional_dock(
            context,
            &text_format,
            width,
            height,
            scene,
            &raised,
            &hover,
            &selected,
            &primary,
            &secondary,
            &accent,
            &error,
            tokens.control_radius,
        );
    } else {
        draw_dock_states(
            context,
            &text_format,
            width,
            &raised,
            &hover,
            &pressed,
            &selected,
            &primary,
            &secondary,
            &disabled,
            &accent,
            &focus,
            &error,
            tokens.control_radius,
            tokens.popover_radius,
        );
    }
    // SAFETY: Category 8 (FFI boundary). This pairs BeginDraw and propagates
    // D2DERR_RECREATE_TARGET to device-resource recovery.
    unsafe { context.EndDraw(None, None) }
}

fn create_brush(context: &ID2D1DeviceContext, color: Rgba8) -> Result<ID2D1SolidColorBrush> {
    let color = D2D1_COLOR_F {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: f32::from(color.a) / 255.0,
    };
    // SAFETY: Category 8 (FFI boundary). Token channels are finite normalized values
    // and the live context returns an owned brush.
    unsafe { context.CreateSolidColorBrush(&color, None) }
}

fn create_text_format(dwrite: &IDWriteFactory, role: ShowcaseRole) -> Result<IDWriteTextFormat> {
    // SAFETY: Category 8 (FFI boundary). Static family and locale strings remain
    // valid for the call and the factory returns an owned interface.
    unsafe {
        dwrite.CreateTextFormat(
            w!("Segoe UI Variable"),
            None,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            if role == ShowcaseRole::Topbar {
                13.0
            } else {
                12.0
            },
            w!("en-US"),
        )
    }
}

pub(crate) fn fill_round(
    context: &ID2D1DeviceContext,
    geometry: D2D1_ROUNDED_RECT,
    brush: &ID2D1SolidColorBrush,
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
    let text = text.encode_utf16().collect::<Vec<_>>();
    // SAFETY: Category 8 (FFI boundary). Text storage, layout, format, and brush
    // remain live for the synchronous draw call.
    unsafe {
        context.DrawText(
            &text,
            format,
            &layout,
            brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
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
