use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::Graphics::DirectWrite::IDWriteFactory;
use windows::core::Result;

use crate::native::{ShellScenes, ShowcaseRole};
use crate::native_icons::NativeIconCache;
use crate::native_showcase_context_menu::{
    ContextMenuBrushes, INSET_ALPHA_PROFILE, MENU_BODY_ALPHA, SHADOW_ALPHA_PROFILE,
    draw_context_menu,
};
use crate::native_showcase_dock::draw_functional_dock;
use crate::native_showcase_dock_states::draw_dock_states;
use crate::native_showcase_material::{MaterialBrushes, MaterialSurface, draw_shell_material};
use crate::native_showcase_popover::{PopoverBrushes, PopoverFormats, draw_functional_popover};
use crate::native_showcase_preview::{
    PREVIEW_CARD_FILL, PREVIEW_CLOSE_FILL, PREVIEW_CLOSE_HOVER_FILL, PREVIEW_CLOSE_HOVER_GLYPH,
    PREVIEW_CLOSE_HOVER_RIM, PREVIEW_CLOSE_RIM, PREVIEW_HOVER_RIM, PREVIEW_PANEL_FILL,
    PREVIEW_PANEL_SOLID_FILL, PreviewBrushes, draw_window_preview,
};
use crate::native_showcase_primitives::{draw_text, fill_round, rect};
use crate::native_showcase_resources::{
    DockBrushes, DockInsetBitmap, DockRenderResources, ShowcaseFormats, create_brush,
    create_detail_format, create_icon_format, create_text_format,
};
use crate::native_showcase_settings::{SettingsBrushes, draw_functional_settings};
use crate::native_showcase_topbar::{TopbarBrushes, draw_functional_topbar};
use crate::{DipRect, Rgba8, ShowcaseTokens, TopbarScene};

pub(crate) struct ShowcaseStyle<'a> {
    role: ShowcaseRole,
    solid_material: bool,
    dock_inset: Option<&'a DockInsetBitmap>,
}

impl<'a> ShowcaseStyle<'a> {
    pub(crate) const fn new(
        role: ShowcaseRole,
        solid_material: bool,
        dock_inset: Option<&'a DockInsetBitmap>,
    ) -> Self {
        Self {
            role,
            solid_material,
            dock_inset,
        }
    }
}

pub(crate) fn draw_showcase(
    context: &ID2D1DeviceContext,
    dwrite: &IDWriteFactory,
    icons: &mut NativeIconCache,
    style: ShowcaseStyle<'_>,
    surface: DipRect,
    scenes: ShellScenes<'_>,
) -> Result<()> {
    let role = style.role;
    let solid_material = style.solid_material;
    let dock_inset = style.dock_inset;
    let width = surface.width;
    let height = surface.height;
    let tokens = if solid_material {
        ShowcaseTokens::solid_fallback()
    } else {
        ShowcaseTokens::obsidian_glass()
    };
    let base_color = if role == ShowcaseRole::Preview {
        if solid_material {
            PREVIEW_PANEL_SOLID_FILL
        } else {
            PREVIEW_PANEL_FILL
        }
    } else {
        tokens.surface_base
    };
    let base = create_brush(context, base_color)?;
    let raised = create_brush(context, tokens.surface_raised)?;
    let hover = create_brush(context, tokens.surface_hover)?;
    let pressed = create_brush(context, tokens.surface_pressed)?;
    let selected = create_brush(context, tokens.surface_selected)?;
    let dock_luminance = create_brush(context, tokens.dock_luminance)?;
    let dock_veil = create_brush(context, tokens.dock_veil)?;
    let dock_reflection = create_brush(context, tokens.dock_reflection)?;
    let topbar_tint = create_brush(context, tokens.topbar_tint)?;
    let primary = create_brush(context, tokens.text_primary)?;
    let secondary = create_brush(context, tokens.text_secondary)?;
    let disabled = create_brush(context, tokens.text_disabled)?;
    let accent = create_brush(context, tokens.accent)?;
    let focus = create_brush(context, tokens.focus_outer)?;
    let rim_outer = create_brush(context, tokens.rim_outer)?;
    let rim_inner = create_brush(context, tokens.rim_inner)?;
    let warning = create_brush(context, tokens.warning)?;
    let error = create_brush(context, tokens.error)?;
    let text_scale = scenes.topbar.map_or(1.0, TopbarScene::text_scale);
    let text_format = create_text_format(dwrite, role, text_scale)?;
    let detail_format = create_detail_format(dwrite, role, text_scale)?;
    let icon_format = create_icon_format(dwrite, role)?;
    let formats = ShowcaseFormats {
        text: &text_format,
        icon: &icon_format,
    };
    let radius = match role {
        ShowcaseRole::Dock => tokens.dock_radius,
        ShowcaseRole::Topbar => 0.0,
        ShowcaseRole::Popover => tokens.popover_radius,
        ShowcaseRole::Preview => tokens.popover_radius,
        ShowcaseRole::Settings => tokens.popover_radius,
    };
    // SAFETY: Category 8 (FFI boundary). A live target is installed and all draw
    // calls finish before the owned brushes are dropped.
    unsafe { context.BeginDraw() };
    // SAFETY: Category 8 (FFI boundary). Null clears the target to transparent.
    unsafe { context.Clear(None) };
    if matches!(role, ShowcaseRole::Dock | ShowcaseRole::Topbar) {
        let motion_strength = scenes
            .dock
            .map_or(0.0, crate::DockScene::material_motion_strength);
        let content_bounds = scenes
            .dock
            .map(|scene| crate::dock_material_bounds(scene, DipRect::new(0.0, 0.0, width, height)));
        draw_shell_material(
            context,
            MaterialSurface {
                role,
                width,
                height,
                radius,
                solid: solid_material,
                motion_strength,
                content_bounds,
            },
            MaterialBrushes {
                luminance: &dock_luminance,
                veil: &dock_veil,
                reflection: &dock_reflection,
                topbar_tint: &topbar_tint,
                rim_outer: &rim_outer,
                dock_inset,
            },
        );
    } else if role != ShowcaseRole::Popover || scenes.context_menu.is_none() {
        fill_round(
            context,
            rect(0.5, 0.5, width - 0.5, height - 0.5, radius),
            &rim_outer,
        );
        fill_round(
            context,
            rect(1.5, 1.5, width - 1.5, height - 1.5, radius - 1.0),
            &base,
        );
        if role != ShowcaseRole::Preview {
            fill_round(context, rect(2.0, 2.0, width - 2.0, 4.0, 1.0), &rim_inner);
        }
    }
    if role == ShowcaseRole::Topbar {
        if let Some(scene) = scenes.topbar {
            draw_functional_topbar(
                context,
                formats,
                DipRect::new(0.0, 0.0, width, height),
                scene,
                TopbarBrushes {
                    hover: &hover,
                    primary: &primary,
                    secondary: &secondary,
                    accent: &accent,
                    warning: &warning,
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
        if let Some(scene) = scenes.context_menu {
            let menu_shadows = SHADOW_ALPHA_PROFILE
                .map(|alpha| create_brush(context, Rgba8::new(0, 0, 0, alpha)))
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            let menu_rim = create_brush(context, Rgba8::new(219, 219, 219, 168))?;
            let menu_body = create_brush(context, Rgba8::new(217, 217, 217, MENU_BODY_ALPHA))?;
            let menu_insets = INSET_ALPHA_PROFILE
                .map(|alpha| create_brush(context, Rgba8::new(13, 13, 13, alpha)))
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            let menu_hover = create_brush(context, Rgba8::new(111, 99, 84, 56))?;
            let menu_primary = create_brush(context, Rgba8::new(18, 18, 18, 255))?;
            let menu_disabled = create_brush(context, Rgba8::new(112, 105, 96, 150))?;
            let menu_separator = create_brush(context, Rgba8::new(90, 84, 76, 54))?;
            draw_context_menu(
                context,
                &text_format,
                DipRect::new(0.0, 0.0, width, height),
                scene,
                ContextMenuBrushes {
                    shadows: &menu_shadows,
                    rim: &menu_rim,
                    body: &menu_body,
                    insets: &menu_insets,
                    hover: &menu_hover,
                    primary: &menu_primary,
                    disabled: &menu_disabled,
                    separator: &menu_separator,
                },
            );
        } else if let Some(scene) = scenes.popover {
            draw_functional_popover(
                context,
                PopoverFormats {
                    label: &text_format,
                    detail: &detail_format,
                },
                width,
                height,
                scene,
                PopoverBrushes {
                    hover: &hover,
                    primary: &primary,
                    secondary: &secondary,
                    accent: &accent,
                    error: &error,
                },
            );
        }
    } else if role == ShowcaseRole::Preview {
        if let Some(scene) = scenes.preview {
            let preview_card = create_brush(context, PREVIEW_CARD_FILL)?;
            let preview_hover_rim = create_brush(context, PREVIEW_HOVER_RIM)?;
            let preview_close = create_brush(context, PREVIEW_CLOSE_FILL)?;
            let preview_close_rim = create_brush(context, PREVIEW_CLOSE_RIM)?;
            let preview_close_hover = create_brush(context, PREVIEW_CLOSE_HOVER_FILL)?;
            let preview_close_hover_rim = create_brush(context, PREVIEW_CLOSE_HOVER_RIM)?;
            let preview_close_hover_glyph = create_brush(context, PREVIEW_CLOSE_HOVER_GLYPH)?;
            draw_window_preview(
                context,
                formats,
                DipRect::new(0.0, 0.0, width, height),
                scene,
                PreviewBrushes {
                    card: &preview_card,
                    hover_rim: &preview_hover_rim,
                    close: &preview_close,
                    close_rim: &preview_close_rim,
                    close_hover: &preview_close_hover,
                    close_hover_rim: &preview_close_hover_rim,
                    close_hover_glyph: &preview_close_hover_glyph,
                    primary: &primary,
                    secondary: &secondary,
                    warning: &warning,
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
            DockRenderResources { formats, icons },
            DipRect::new(0.0, 0.0, width, height),
            scene,
            DockBrushes {
                pressed: &pressed,
                primary: &primary,
                secondary: &secondary,
                accent: &accent,
                focus: &focus,
            },
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
