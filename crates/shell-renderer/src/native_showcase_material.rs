use crate::native::ShowcaseRole;
use crate::native_liquid_glass::LiquidGlassResources;
use crate::native_showcase_primitives::{fill_round, rect};
use crate::native_showcase_resources::DockInsetBitmap;
use crate::{DipRect, Rgba8};
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};

#[derive(Clone, Copy)]
pub(crate) struct MaterialSurface {
    pub role: ShowcaseRole,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub solid: bool,
    pub motion_strength: f32,
    pub hover_position_x: Option<f32>,
    pub content_bounds: Option<DipRect>,
}

pub(crate) struct MaterialBrushes<'a> {
    pub base: &'a ID2D1SolidColorBrush,
    pub luminance: &'a ID2D1SolidColorBrush,
    pub veil: &'a ID2D1SolidColorBrush,
    pub reflection: &'a ID2D1SolidColorBrush,
    pub topbar_tint: &'a ID2D1SolidColorBrush,
    pub rim_outer: &'a ID2D1SolidColorBrush,
    pub panel_rim: &'a ID2D1SolidColorBrush,
    pub dock_inset: Option<&'a DockInsetBitmap>,
    pub liquid_glass: Option<&'a LiquidGlassResources>,
}

pub(crate) fn draw_shell_material(
    context: &ID2D1DeviceContext,
    surface: MaterialSurface,
    brushes: MaterialBrushes<'_>,
) {
    match surface.role {
        ShowcaseRole::Dock => draw_dock_material(context, surface, brushes),
        ShowcaseRole::Topbar => draw_topbar_material(context, surface, brushes),
        ShowcaseRole::Popover => draw_panel_material(context, surface, brushes),
        ShowcaseRole::Preview | ShowcaseRole::Settings => {}
    }
}

pub(crate) const fn panel_base_color(solid: bool) -> Rgba8 {
    if solid {
        Rgba8::new(0x1C, 0x21, 0x29, 0xFF)
    } else {
        Rgba8::new(0xD9, 0xD9, 0xD9, 0x8C)
    }
}

pub(crate) const fn panel_luminance_color(solid: bool) -> Rgba8 {
    if solid {
        Rgba8::new(0x00, 0x00, 0x00, 0x00)
    } else {
        Rgba8::new(0xD9, 0xD9, 0xD9, 0x66)
    }
}

pub(crate) const fn panel_veil_color(_solid: bool) -> Rgba8 {
    Rgba8::new(0xD9, 0xD9, 0xD9, 0x00)
}

pub(crate) const fn panel_reflection_color(_solid: bool) -> Rgba8 {
    Rgba8::new(0xD9, 0xD9, 0xD9, 0x00)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PanelMaterialGeometry {
    rim_bounds: DipRect,
    body_bounds: DipRect,
    body_radius: f32,
}

fn panel_material_geometry(bounds: DipRect, radius: f32) -> PanelMaterialGeometry {
    PanelMaterialGeometry {
        rim_bounds: DipRect::new(
            bounds.x + 0.5,
            bounds.y + 0.5,
            (bounds.width - 1.0).max(1.0),
            (bounds.height - 1.0).max(1.0),
        ),
        body_bounds: DipRect::new(
            bounds.x + 1.0,
            bounds.y + 1.0,
            (bounds.width - 2.0).max(1.0),
            (bounds.height - 2.0).max(1.0),
        ),
        body_radius: (radius - 0.5).max(0.0),
    }
}

fn draw_panel_material(
    context: &ID2D1DeviceContext,
    surface: MaterialSurface,
    brushes: MaterialBrushes<'_>,
) {
    let MaterialSurface {
        width,
        height,
        radius,
        solid,
        content_bounds,
        ..
    } = surface;
    let bounds = content_bounds.unwrap_or_else(|| DipRect::new(0.0, 0.0, width, height));
    let geometry = panel_material_geometry(bounds, radius);

    fill_round(
        context,
        rounded_bounds(geometry.rim_bounds, radius),
        brushes.panel_rim,
    );
    let body = rounded_bounds(geometry.body_bounds, geometry.body_radius);
    fill_round(context, body, brushes.base);
    if solid {
        return;
    }

    fill_round(context, body, brushes.luminance);
    fill_round(context, body, brushes.veil);
    fill_round(context, body, brushes.reflection);
    if let Some(liquid_glass) = brushes.liquid_glass {
        liquid_glass.draw(context, bounds, None, 0.0);
    }
}

fn draw_dock_material(
    context: &ID2D1DeviceContext,
    surface: MaterialSurface,
    brushes: MaterialBrushes<'_>,
) {
    let MaterialSurface {
        width,
        height,
        radius,
        solid,
        motion_strength,
        hover_position_x,
        content_bounds,
        ..
    } = surface;
    let geometry = dock_material_geometry(
        content_bounds.unwrap_or_else(|| DipRect::new(0.0, 0.0, width, height)),
        motion_strength,
    );
    let bounds = geometry.bounds;
    if solid {
        fill_round(context, rounded_bounds(bounds, radius), brushes.luminance);
        return;
    }
    let right = bounds.x + bounds.width;
    let bottom = bounds.y + bounds.height;
    fill_round(
        context,
        rect(
            bounds.x + 0.5,
            bounds.y + 0.5,
            right - 0.5,
            bottom - 0.5,
            radius,
        ),
        brushes.rim_outer,
    );
    let inner_radius = (radius - 0.5).max(0.0);
    let body = rect(
        bounds.x + 1.0,
        bounds.y + 1.0,
        right - 1.0,
        bottom - 1.0,
        inner_radius,
    );
    fill_round(context, body, brushes.luminance);
    fill_round(context, body, brushes.veil);
    fill_round(context, body, brushes.reflection);
    if let Some(liquid_glass) = brushes.liquid_glass {
        liquid_glass.draw(context, bounds, hover_position_x, motion_strength);
    }
    if let Some(inset) = brushes.dock_inset {
        inset.draw_at(context, bounds);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DockMaterialGeometry {
    bounds: DipRect,
}

fn dock_material_geometry(base: DipRect, _motion_strength: f32) -> DockMaterialGeometry {
    let top = base.y + 2.0;
    let bottom = base.y + base.height;
    let bounds = DipRect::new(base.x, top, base.width.max(1.0), (bottom - top).max(1.0));
    DockMaterialGeometry { bounds }
}

fn rounded_bounds(
    bounds: DipRect,
    radius: f32,
) -> windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
    rect(
        bounds.x,
        bounds.y,
        bounds.x + bounds.width,
        bounds.y + bounds.height,
        radius,
    )
}

fn draw_topbar_material(
    context: &ID2D1DeviceContext,
    surface: MaterialSurface,
    brushes: MaterialBrushes<'_>,
) {
    let MaterialSurface {
        width,
        height,
        solid,
        ..
    } = surface;
    if topbar_material_plan(solid).draw_full_surface {
        fill_round(
            context,
            rect(0.0, 0.0, width, height, 0.0),
            brushes.topbar_tint,
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TopbarMaterialPlan {
    draw_full_surface: bool,
}

const fn topbar_material_plan(solid: bool) -> TopbarMaterialPlan {
    TopbarMaterialPlan {
        draw_full_surface: solid,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dock_material_geometry, panel_base_color, panel_luminance_color, panel_material_geometry,
        panel_reflection_color, panel_veil_color, topbar_material_plan,
    };

    #[test]
    fn balanced_glass_uses_the_figma_neutral_tint_at_seventy_three_percent() {
        let tokens = crate::ShowcaseTokens::obsidian_glass();
        assert_eq!(
            panel_base_color(false),
            crate::Rgba8::new(0xD9, 0xD9, 0xD9, 0x8C)
        );
        assert_eq!(
            panel_luminance_color(false),
            crate::Rgba8::new(0xD9, 0xD9, 0xD9, 0x66)
        );
        assert_eq!(panel_veil_color(false).a, 0x00);
        assert_eq!(panel_reflection_color(false).a, 0x00);
        assert_eq!(
            tokens.dock_luminance,
            crate::Rgba8::new(0x4D, 0x4D, 0x4D, 0x4D)
        );
        let opacity = [
            panel_base_color(false).a,
            panel_luminance_color(false).a,
            panel_veil_color(false).a,
            panel_reflection_color(false).a,
        ]
        .into_iter()
        .fold(0.0, |composite, alpha| {
            let source = f32::from(alpha) / 255.0;
            source + composite * (1.0 - source)
        });

        assert!((opacity - 0.73).abs() <= 0.015, "opacity={opacity}");
        assert_eq!(panel_base_color(true).a, 0xFF);
    }

    #[test]
    fn dock_panel_stays_fixed_while_icons_animate_on_hover() {
        let base = crate::DipRect::new(3.0, 0.0, 114.0, 55.0);
        let resting = dock_material_geometry(base, 0.0);
        let active = dock_material_geometry(base, 1.0);

        assert_eq!(resting.bounds.x, 3.0);
        assert_eq!(resting.bounds.y, 2.0);
        assert_eq!(resting.bounds.width, 114.0);
        assert_eq!(resting.bounds.height, 53.0);
        assert_eq!(active.bounds.x, resting.bounds.x);
        assert_eq!(active.bounds.y, resting.bounds.y);
        assert_eq!(active.bounds.width, resting.bounds.width);
        assert_eq!(active.bounds.height, resting.bounds.height);
    }

    #[test]
    fn panel_uses_a_half_dip_micro_rim_instead_of_a_heavy_outline() {
        let geometry = panel_material_geometry(crate::DipRect::new(0.0, 0.0, 288.0, 356.0), 12.0);

        assert_eq!(
            geometry.rim_bounds,
            crate::DipRect::new(0.5, 0.5, 287.0, 355.0)
        );
        assert_eq!(
            geometry.body_bounds,
            crate::DipRect::new(1.0, 1.0, 286.0, 354.0)
        );
        assert_eq!(geometry.body_radius, 11.5);
    }

    #[test]
    fn dock_material_geometry_sanitizes_non_finite_motion() {
        let geometry = dock_material_geometry(crate::DipRect::new(0.0, 0.0, 120.0, 55.0), f32::NAN);

        assert!(geometry.bounds.x.is_finite());
        assert!(geometry.bounds.y.is_finite());
    }

    #[test]
    fn topbar_material_plan_only_draws_full_surface_for_solid_fallback() {
        assert!(!topbar_material_plan(false).draw_full_surface);
        assert!(topbar_material_plan(true).draw_full_surface);
    }
}
