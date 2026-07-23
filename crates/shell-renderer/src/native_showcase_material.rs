use crate::DipRect;
use crate::native::ShowcaseRole;
use crate::native_liquid_glass::DockLiquidGlassResources;
use crate::native_showcase_primitives::{fill_round, rect};
use crate::native_showcase_resources::DockInsetBitmap;
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
    pub luminance: &'a ID2D1SolidColorBrush,
    pub veil: &'a ID2D1SolidColorBrush,
    pub reflection: &'a ID2D1SolidColorBrush,
    pub topbar_tint: &'a ID2D1SolidColorBrush,
    pub rim_outer: &'a ID2D1SolidColorBrush,
    pub dock_inset: Option<&'a DockInsetBitmap>,
    pub liquid_glass: Option<&'a DockLiquidGlassResources>,
}

pub(crate) fn draw_shell_material(
    context: &ID2D1DeviceContext,
    surface: MaterialSurface,
    brushes: MaterialBrushes<'_>,
) {
    match surface.role {
        ShowcaseRole::Dock => draw_dock_material(context, surface, brushes),
        ShowcaseRole::Topbar => draw_topbar_material(context, surface, brushes),
        ShowcaseRole::Popover | ShowcaseRole::Preview | ShowcaseRole::Settings => {}
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
    fill_round(
        context,
        rect(0.0, 0.0, width, height, 0.0),
        brushes.topbar_tint,
    );
    if solid {
        return;
    }
    fill_round(context, rect(0.0, 0.0, width, 1.0, 0.0), brushes.reflection);
    fill_round(
        context,
        rect(0.0, height - 1.0, width, height, 0.0),
        brushes.rim_outer,
    );
}

#[cfg(test)]
mod tests {
    use super::dock_material_geometry;

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
    fn dock_material_geometry_sanitizes_non_finite_motion() {
        let geometry = dock_material_geometry(crate::DipRect::new(0.0, 0.0, 120.0, 55.0), f32::NAN);

        assert!(geometry.bounds.x.is_finite());
        assert!(geometry.bounds.y.is_finite());
    }
}
