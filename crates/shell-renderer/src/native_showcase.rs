use std::collections::HashMap;
use std::hash::Hash;

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    IDWriteFactory, IDWriteTextFormat,
};
use windows::core::Result;

use crate::native::{ShellScenes, ShowcaseRole};
use crate::native_desktop_capture::DesktopBlurCapture;
use crate::native_icons::NativeIconCache;
use crate::native_liquid_glass::LiquidGlassResources;
use crate::native_showcase_context_menu::{
    ContextMenuBrushes, INSET_ALPHA_PROFILE, MENU_BODY_ALPHA, SHADOW_ALPHA_PROFILE,
    draw_context_menu,
};
use crate::native_showcase_dock::draw_functional_dock;
use crate::native_showcase_dock_states::draw_dock_states;
use crate::native_showcase_material::{
    MaterialBrushes, MaterialSurface, draw_shell_material, panel_base_color, panel_luminance_color,
    panel_reflection_color, panel_veil_color,
};
use crate::native_showcase_popover::{PopoverBrushes, PopoverFormats, draw_functional_popover};
use crate::native_showcase_preview::{
    PREVIEW_CARD_FILL, PREVIEW_CLOSE_FILL, PREVIEW_CLOSE_HOVER_FILL, PREVIEW_CLOSE_HOVER_GLYPH,
    PREVIEW_CLOSE_HOVER_RIM, PREVIEW_CLOSE_RIM, PREVIEW_HOVER_RIM, PREVIEW_PANEL_FILL,
    PREVIEW_PANEL_SOLID_FILL, PreviewBrushes, draw_window_preview,
};
use crate::native_showcase_primitives::{draw_text, fill_round, rect};
use crate::native_showcase_quick_settings::{
    QuickSettingsBrushes, QuickSettingsFormats, draw_quick_settings,
};
use crate::native_showcase_resources::{
    DockBrushes, DockInsetBitmap, DockRenderResources, ShowcaseFormats, create_brush,
    create_detail_format, create_icon_format, create_popover_title_format,
    create_quick_settings_detail_format, create_quick_settings_label_format, create_text_format,
};
use crate::native_showcase_settings::{SettingsBrushes, SettingsFormats, draw_functional_settings};
use crate::native_showcase_topbar::{TopbarBrushes, draw_functional_topbar};
use crate::{
    DipRect, PopoverLayoutStyle, QUICK_SETTINGS_BODY_TOP, Rgba8, ShowcaseTokens, TopbarScene,
    VisualPreferences,
};

struct ReusableResourceMap<K, V> {
    values: HashMap<K, V>,
}

impl<K, V> Default for ReusableResourceMap<K, V> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
}

impl<K, V> ReusableResourceMap<K, V>
where
    K: Eq + Hash,
    V: Clone,
{
    fn get_or_try_insert_with<E>(
        &mut self,
        key: K,
        create: impl FnOnce() -> std::result::Result<V, E>,
    ) -> std::result::Result<V, E> {
        if let Some(value) = self.values.get(&key) {
            return Ok(value.clone());
        }
        let value = create()?;
        self.values.insert(key, value.clone());
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum TextFormatKind {
    Text,
    Detail,
    QuickLabel,
    QuickDetail,
    QuickFooter,
    Icon,
    PopoverTitle,
    BalancedPopoverLabel,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TextFormatKey {
    kind: TextFormatKind,
    role: u8,
    text_scale_bits: u32,
}

impl TextFormatKey {
    fn new(kind: TextFormatKind, role: ShowcaseRole, text_scale: f32) -> Self {
        let role = match role {
            ShowcaseRole::Topbar => 0,
            ShowcaseRole::Dock => 1,
            ShowcaseRole::Popover => 2,
            ShowcaseRole::AppMenu => 3,
            ShowcaseRole::Preview => 4,
            ShowcaseRole::Settings => 5,
        };
        let text_scale = if role == 0 {
            if text_scale.is_finite() {
                text_scale.clamp(1.0, 2.5)
            } else {
                1.0
            }
        } else {
            1.0
        };
        Self {
            kind,
            role,
            text_scale_bits: text_scale.to_bits(),
        }
    }
}

#[derive(Default)]
pub(crate) struct ShowcaseResourceCache {
    brushes: ReusableResourceMap<u32, ID2D1SolidColorBrush>,
    formats: ReusableResourceMap<TextFormatKey, IDWriteTextFormat>,
}

impl ShowcaseResourceCache {
    fn brush(
        &mut self,
        context: &ID2D1DeviceContext,
        color: Rgba8,
    ) -> Result<ID2D1SolidColorBrush> {
        self.brushes
            .get_or_try_insert_with(color_key(color), || create_brush(context, color))
    }

    fn text(
        &mut self,
        dwrite: &IDWriteFactory,
        role: ShowcaseRole,
        text_scale: f32,
    ) -> Result<IDWriteTextFormat> {
        self.formats.get_or_try_insert_with(
            TextFormatKey::new(TextFormatKind::Text, role, text_scale),
            || create_text_format(dwrite, role, text_scale),
        )
    }

    fn detail(
        &mut self,
        dwrite: &IDWriteFactory,
        role: ShowcaseRole,
        text_scale: f32,
    ) -> Result<IDWriteTextFormat> {
        self.formats.get_or_try_insert_with(
            TextFormatKey::new(TextFormatKind::Detail, role, text_scale),
            || create_detail_format(dwrite, role, text_scale),
        )
    }

    fn quick_label(&mut self, dwrite: &IDWriteFactory) -> Result<IDWriteTextFormat> {
        self.formats.get_or_try_insert_with(
            TextFormatKey::new(TextFormatKind::QuickLabel, ShowcaseRole::Popover, 1.0),
            || create_quick_settings_label_format(dwrite),
        )
    }

    fn quick_detail(&mut self, dwrite: &IDWriteFactory) -> Result<IDWriteTextFormat> {
        self.formats.get_or_try_insert_with(
            TextFormatKey::new(TextFormatKind::QuickDetail, ShowcaseRole::Popover, 1.0),
            || create_quick_settings_detail_format(dwrite),
        )
    }

    fn quick_footer(&mut self, dwrite: &IDWriteFactory) -> Result<IDWriteTextFormat> {
        self.formats.get_or_try_insert_with(
            TextFormatKey::new(TextFormatKind::QuickFooter, ShowcaseRole::Popover, 1.0),
            || {
                let format = create_quick_settings_detail_format(dwrite)?;
                // SAFETY: This cache entry is dedicated to centered footer text
                // and remains immutable after its one-time initialization.
                unsafe { format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)? };
                Ok(format)
            },
        )
    }

    fn icon(&mut self, dwrite: &IDWriteFactory, role: ShowcaseRole) -> Result<IDWriteTextFormat> {
        self.formats
            .get_or_try_insert_with(TextFormatKey::new(TextFormatKind::Icon, role, 1.0), || {
                create_icon_format(dwrite, role)
            })
    }

    fn popover_title(&mut self, dwrite: &IDWriteFactory) -> Result<IDWriteTextFormat> {
        self.formats.get_or_try_insert_with(
            TextFormatKey::new(TextFormatKind::PopoverTitle, ShowcaseRole::Popover, 1.0),
            || create_popover_title_format(dwrite),
        )
    }

    fn balanced_popover_label(&mut self, dwrite: &IDWriteFactory) -> Result<IDWriteTextFormat> {
        self.formats.get_or_try_insert_with(
            TextFormatKey::new(
                TextFormatKind::BalancedPopoverLabel,
                ShowcaseRole::Popover,
                1.0,
            ),
            || create_balanced_apps_label_format(dwrite),
        )
    }
}

const fn color_key(color: Rgba8) -> u32 {
    u32::from_be_bytes([color.r, color.g, color.b, color.a])
}

pub(crate) struct ShowcaseStyle<'a> {
    role: ShowcaseRole,
    solid_material: bool,
    dock_inset: Option<&'a DockInsetBitmap>,
    liquid_glass: Option<&'a LiquidGlassResources>,
    desktop_blur: Option<&'a DesktopBlurCapture>,
    visual_preferences: VisualPreferences,
}

impl<'a> ShowcaseStyle<'a> {
    pub(crate) const fn new(
        role: ShowcaseRole,
        solid_material: bool,
        dock_inset: Option<&'a DockInsetBitmap>,
        liquid_glass: Option<&'a LiquidGlassResources>,
        desktop_blur: Option<&'a DesktopBlurCapture>,
        visual_preferences: VisualPreferences,
    ) -> Self {
        Self {
            role,
            solid_material,
            dock_inset,
            liquid_glass,
            desktop_blur,
            visual_preferences,
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the native draw boundary receives independent device, cache, style, geometry and scene resources"
)]
pub(crate) fn draw_showcase(
    context: &ID2D1DeviceContext,
    dwrite: &IDWriteFactory,
    icons: &mut NativeIconCache,
    resources: &mut ShowcaseResourceCache,
    style: ShowcaseStyle<'_>,
    surface: DipRect,
    scenes: ShellScenes<'_>,
) -> Result<()> {
    let role = style.role;
    let solid_material = style.solid_material;
    let dock_inset = style.dock_inset;
    let liquid_glass = style.liquid_glass;
    let desktop_blur = style.desktop_blur;
    let width = surface.width;
    let height = surface.height;
    let tokens = if solid_material {
        ShowcaseTokens::solid_fallback()
    } else {
        ShowcaseTokens::obsidian_glass()
    }
    .with_preferences(style.visual_preferences, solid_material);
    let base_color = if role == ShowcaseRole::Preview {
        if solid_material {
            PREVIEW_PANEL_SOLID_FILL
        } else {
            PREVIEW_PANEL_FILL
        }
    } else {
        tokens.surface_base
    };
    let base = resources.brush(context, base_color)?;
    let panel_color = |color| {
        if solid_material {
            color
        } else {
            style.visual_preferences.apply_background_alpha(color)
        }
    };
    let panel_base = resources.brush(context, panel_color(panel_base_color(solid_material)))?;
    let panel_luminance =
        resources.brush(context, panel_color(panel_luminance_color(solid_material)))?;
    let panel_veil = resources.brush(context, panel_color(panel_veil_color(solid_material)))?;
    let panel_reflection =
        resources.brush(context, panel_color(panel_reflection_color(solid_material)))?;
    let raised = resources.brush(context, tokens.surface_raised)?;
    let hover = resources.brush(context, tokens.surface_hover)?;
    let pressed = resources.brush(context, tokens.surface_pressed)?;
    let selected = resources.brush(context, tokens.surface_selected)?;
    let dock_luminance = resources.brush(context, tokens.dock_luminance)?;
    let dock_veil = resources.brush(context, tokens.dock_veil)?;
    let dock_reflection = resources.brush(context, tokens.dock_reflection)?;
    let topbar_tint = resources.brush(context, tokens.topbar_tint)?;
    let primary = resources.brush(context, tokens.text_primary)?;
    let secondary = resources.brush(context, tokens.text_secondary)?;
    let contrast_shadow = &topbar_tint;
    let disabled = resources.brush(context, tokens.text_disabled)?;
    let accent = resources.brush(context, tokens.accent)?;
    let focus = resources.brush(context, tokens.focus_outer)?;
    let rim_outer = resources.brush(context, tokens.rim_outer)?;
    let panel_rim = if solid_material {
        resources.brush(context, tokens.dock_luminance)?
    } else {
        resources.brush(context, Rgba8::new(0x00, 0x00, 0x00, 0x20))?
    };
    let panel_hover = resources.brush(
        context,
        if solid_material {
            tokens.surface_hover
        } else {
            Rgba8::new(0x00, 0x00, 0x00, 0x14)
        },
    )?;
    let panel_raised = resources.brush(
        context,
        if solid_material {
            tokens.surface_raised
        } else {
            Rgba8::new(0x00, 0x00, 0x00, 0x0C)
        },
    )?;
    let panel_divider = resources.brush(
        context,
        if solid_material {
            tokens.rim_inner
        } else {
            Rgba8::new(0x00, 0x00, 0x00, 0x24)
        },
    )?;
    let panel_focus = resources.brush(
        context,
        if solid_material {
            tokens.surface_selected
        } else {
            Rgba8::new(0x0A, 0x64, 0xD8, 0x26)
        },
    )?;
    let panel_notch = resources.brush(
        context,
        if solid_material {
            tokens.surface_base
        } else {
            Rgba8::new(0xD9, 0xD9, 0xD9, 0xBA)
        },
    )?;
    let panel_primary = resources.brush(
        context,
        if solid_material {
            tokens.text_primary
        } else {
            Rgba8::new(0x1A, 0x1A, 0x1A, 0xFF)
        },
    )?;
    let panel_secondary = resources.brush(
        context,
        if solid_material {
            tokens.text_secondary
        } else {
            Rgba8::new(0x45, 0x45, 0x48, 0xFF)
        },
    )?;
    let panel_accent = resources.brush(
        context,
        if solid_material {
            tokens.accent
        } else {
            Rgba8::new(0x0A, 0x64, 0xD8, 0xFF)
        },
    )?;
    let panel_error = resources.brush(
        context,
        if solid_material {
            tokens.error
        } else {
            Rgba8::new(0xB4, 0x23, 0x18, 0xFF)
        },
    )?;
    let quick_section = resources.brush(
        context,
        if solid_material {
            tokens.surface_raised
        } else {
            Rgba8::new(0xFF, 0xFF, 0xFF, 0x1E)
        },
    )?;
    let quick_tile = resources.brush(
        context,
        if solid_material {
            tokens.surface_raised
        } else {
            Rgba8::new(0xFF, 0xFF, 0xFF, 0x2A)
        },
    )?;
    let quick_hover = resources.brush(
        context,
        if solid_material {
            tokens.surface_hover
        } else {
            Rgba8::new(0xFF, 0xFF, 0xFF, 0x46)
        },
    )?;
    let quick_pressed = resources.brush(
        context,
        if solid_material {
            tokens.surface_pressed
        } else {
            Rgba8::new(0x00, 0x00, 0x00, 0x18)
        },
    )?;
    let quick_selected = resources.brush(
        context,
        if solid_material {
            tokens.surface_selected
        } else {
            Rgba8::new(0x0A, 0x64, 0xD8, 0x30)
        },
    )?;
    let quick_on_accent = resources.brush(context, Rgba8::new(0xFF, 0xFF, 0xFF, 0xF2))?;
    let rim_inner = resources.brush(context, tokens.rim_inner)?;
    let warning = resources.brush(context, tokens.warning)?;
    let error = resources.brush(context, tokens.error)?;
    let text_scale = scenes.topbar.map_or(1.0, TopbarScene::text_scale);
    let text_format = resources.text(dwrite, role, text_scale)?;
    let balanced_label_format = if scenes
        .popover
        .is_some_and(|scene| scene.layout_style() == PopoverLayoutStyle::BalancedApps)
    {
        Some(resources.balanced_popover_label(dwrite)?)
    } else {
        None
    };
    let detail_format = resources.detail(dwrite, role, text_scale)?;
    let quick_label_format = resources.quick_label(dwrite)?;
    let quick_detail_format = resources.quick_detail(dwrite)?;
    let quick_footer_format = resources.quick_footer(dwrite)?;
    let icon_format = resources.icon(dwrite, role)?;
    let formats = ShowcaseFormats {
        text: &text_format,
        icon: &icon_format,
    };
    let radius = match role {
        ShowcaseRole::Dock => tokens.dock_radius,
        ShowcaseRole::Topbar => 0.0,
        ShowcaseRole::Popover | ShowcaseRole::AppMenu => tokens.popover_radius,
        ShowcaseRole::Preview => tokens.popover_radius,
        ShowcaseRole::Settings => tokens.popover_radius,
    };
    // SAFETY: Category 8 (FFI boundary). A live target is installed and all draw
    // calls finish before the owned brushes are dropped.
    unsafe { context.BeginDraw() };
    // SAFETY: Category 8 (FFI boundary). Null clears the target to transparent.
    unsafe { context.Clear(None) };
    if let Some(desktop_blur) = desktop_blur {
        desktop_blur.draw(context, width, height, radius)?;
    }
    if matches!(role, ShowcaseRole::Dock | ShowcaseRole::Topbar) {
        let motion_strength = scenes
            .dock
            .map_or(0.0, crate::DockScene::material_motion_strength);
        let hover_position_x = scenes.dock.and_then(crate::DockScene::hover_position_x);
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
                hover_position_x,
                content_bounds,
            },
            MaterialBrushes {
                base: &panel_base,
                luminance: &dock_luminance,
                veil: &dock_veil,
                reflection: &dock_reflection,
                topbar_tint: &topbar_tint,
                rim_outer: &rim_outer,
                panel_rim: &panel_rim,
                dock_inset,
                liquid_glass,
            },
        );
    } else if role == ShowcaseRole::Popover && scenes.context_menu.is_none() {
        if scenes.quick_settings.is_some() || scenes.popover.is_some() {
            let content_bounds = if scenes.quick_settings.is_some() {
                DipRect::new(
                    0.0,
                    QUICK_SETTINGS_BODY_TOP,
                    width,
                    (height - QUICK_SETTINGS_BODY_TOP).max(1.0),
                )
            } else if let Some(popover) = scenes.popover {
                match popover.layout_style() {
                    PopoverLayoutStyle::Compact => DipRect::new(0.0, 0.0, width, height),
                    PopoverLayoutStyle::SystemPanel => {
                        DipRect::new(0.0, 8.0, width, (height - 8.0).max(1.0))
                    }
                    PopoverLayoutStyle::BalancedApps => {
                        DipRect::new(0.0, 8.0, width, (height - 8.0).max(1.0))
                    }
                }
            } else {
                DipRect::new(0.0, 0.0, width, height)
            };
            draw_shell_material(
                context,
                MaterialSurface {
                    role,
                    width,
                    height,
                    radius,
                    solid: solid_material,
                    motion_strength: 0.0,
                    hover_position_x: None,
                    content_bounds: Some(content_bounds),
                },
                MaterialBrushes {
                    base: &panel_base,
                    luminance: &panel_luminance,
                    veil: &panel_veil,
                    reflection: &panel_reflection,
                    topbar_tint: &topbar_tint,
                    rim_outer: &rim_outer,
                    panel_rim: &panel_rim,
                    dock_inset: None,
                    liquid_glass,
                },
            );
        }
    } else if !matches!(role, ShowcaseRole::Popover | ShowcaseRole::AppMenu) {
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
                    focus: &focus,
                    warning: &warning,
                    contrast_shadow,
                    liquid_glass,
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
    } else if matches!(role, ShowcaseRole::Popover | ShowcaseRole::AppMenu) {
        if let Some(scene) = scenes.context_menu {
            let menu_shadows = SHADOW_ALPHA_PROFILE
                .map(|alpha| resources.brush(context, Rgba8::new(0, 0, 0, alpha)))
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            let menu_rim = resources.brush(context, Rgba8::new(219, 219, 219, 168))?;
            let menu_body = resources.brush(context, Rgba8::new(217, 217, 217, MENU_BODY_ALPHA))?;
            let menu_insets = INSET_ALPHA_PROFILE
                .map(|alpha| resources.brush(context, Rgba8::new(13, 13, 13, alpha)))
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            let menu_hover = resources.brush(context, Rgba8::new(111, 99, 84, 56))?;
            let menu_primary = resources.brush(context, Rgba8::new(18, 18, 18, 255))?;
            let menu_disabled = resources.brush(context, Rgba8::new(112, 105, 96, 150))?;
            let menu_separator = resources.brush(context, Rgba8::new(90, 84, 76, 54))?;
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
        } else if role == ShowcaseRole::Popover
            && let Some(scene) = scenes.quick_settings
        {
            draw_quick_settings(
                context,
                icons,
                QuickSettingsFormats {
                    label: &quick_label_format,
                    detail: &quick_detail_format,
                    footer: &quick_footer_format,
                    icon: &icon_format,
                },
                DipRect::new(0.0, 0.0, width, height),
                scene,
                QuickSettingsBrushes {
                    section: &quick_section,
                    tile: &quick_tile,
                    hover: &quick_hover,
                    pressed: &quick_pressed,
                    selected: &quick_selected,
                    primary: &panel_primary,
                    secondary: &panel_secondary,
                    accent: &panel_accent,
                    on_accent: &quick_on_accent,
                    focus: &focus,
                    error: &panel_error,
                    base: &panel_notch,
                    rim: &panel_rim,
                },
            );
        } else if role == ShowcaseRole::Popover
            && let Some(scene) = scenes.popover
        {
            let title_format = resources.popover_title(dwrite)?;
            draw_functional_popover(
                context,
                icons,
                PopoverFormats {
                    label: &text_format,
                    balanced_label: balanced_label_format.as_ref().unwrap_or(&text_format),
                    detail: &detail_format,
                    icon: &icon_format,
                    title: &title_format,
                },
                DipRect::new(0.0, 0.0, width, height),
                scene,
                PopoverBrushes {
                    hover: &panel_hover,
                    raised: &panel_raised,
                    divider: &panel_divider,
                    focus: &panel_focus,
                    base: &panel_notch,
                    rim: &panel_rim,
                    primary: &panel_primary,
                    secondary: &panel_secondary,
                    accent: &panel_accent,
                    error: &panel_error,
                },
            )?;
        }
    } else if role == ShowcaseRole::Preview {
        if let Some(scene) = scenes.preview {
            let preview_card = resources.brush(context, PREVIEW_CARD_FILL)?;
            let preview_hover_rim = resources.brush(context, PREVIEW_HOVER_RIM)?;
            let preview_close = resources.brush(context, PREVIEW_CLOSE_FILL)?;
            let preview_close_rim = resources.brush(context, PREVIEW_CLOSE_RIM)?;
            let preview_close_hover = resources.brush(context, PREVIEW_CLOSE_HOVER_FILL)?;
            let preview_close_hover_rim = resources.brush(context, PREVIEW_CLOSE_HOVER_RIM)?;
            let preview_close_hover_glyph = resources.brush(context, PREVIEW_CLOSE_HOVER_GLYPH)?;
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
            let title_format = resources.popover_title(dwrite)?;
            draw_functional_settings(
                context,
                SettingsFormats {
                    title: &title_format,
                    label: &quick_label_format,
                    detail: &quick_detail_format,
                    icon: &icon_format,
                },
                DipRect::new(0.0, 0.0, width, height),
                scene,
                SettingsBrushes {
                    raised: &raised,
                    hover: &hover,
                    selected: &selected,
                    pressed: &pressed,
                    primary: &primary,
                    secondary: &secondary,
                    disabled: &disabled,
                    accent: &accent,
                    focus: &focus,
                    divider: &rim_inner,
                    on_accent: &quick_on_accent,
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

fn create_balanced_apps_label_format(dwrite: &IDWriteFactory) -> Result<IDWriteTextFormat> {
    let format = create_text_format(dwrite, ShowcaseRole::Popover, 1.0)?;
    let trimming = DWRITE_TRIMMING {
        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
        delimiter: 0,
        delimiterCount: 0,
    };
    // SAFETY: The dedicated format and ellipsis object remain live through the
    // synchronous DirectWrite setter call.
    let ellipsis = unsafe { dwrite.CreateEllipsisTrimmingSign(&format) }?;
    // SAFETY: The trimming descriptor and ellipsis sign are valid for the live
    // dedicated BalancedApps label format.
    unsafe { format.SetTrimming(&trimming, &ellipsis) }?;
    Ok(format)
}

#[cfg(test)]
mod resource_cache_tests {
    use std::cell::Cell;

    use super::ReusableResourceMap;

    #[test]
    fn repeated_render_resource_requests_reuse_the_first_created_value() {
        let creations = Cell::new(0);
        let mut resources = ReusableResourceMap::default();

        let first = resources
            .get_or_try_insert_with("dock-brush", || {
                creations.set(creations.get() + 1);
                Ok::<_, ()>(41_u32)
            })
            .unwrap();
        let second = resources
            .get_or_try_insert_with("dock-brush", || {
                creations.set(creations.get() + 1);
                Ok::<_, ()>(99_u32)
            })
            .unwrap();

        assert_eq!(first, 41);
        assert_eq!(second, 41);
        assert_eq!(creations.get(), 1);
    }
}
