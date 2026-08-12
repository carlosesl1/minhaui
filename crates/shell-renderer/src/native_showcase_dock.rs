use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;

use crate::native_showcase_primitives::{draw_text, fill_round, rect};
use crate::native_showcase_resources::{DockBrushes, DockRenderResources};
use crate::{
    DipRect, DockIcon, DockItemVisualKind, DockScene, RunningIndicator, layout_dock_scene,
};

pub(crate) fn draw_functional_dock(
    context: &ID2D1DeviceContext,
    resources: DockRenderResources<'_>,
    surface: DipRect,
    scene: &DockScene,
    brushes: DockBrushes<'_>,
    control_radius: f32,
) {
    resources.icons.retain_dock(|source| {
        scene.items().iter().any(|item| {
            matches!(
                item.icon(),
                DockIcon::WindowsExecutable(active) if active.as_ref() == source
            )
        })
    });
    let layout = layout_dock_scene(scene, surface);
    if let Some(x) = scene.drag_insertion_x() {
        fill_round(
            context,
            rect(
                x - 1.0,
                surface.y + 7.0,
                x + 1.0,
                surface.y + surface.height - 7.0,
                1.0,
            ),
            if scene.drag_target_valid() {
                brushes.accent
            } else {
                brushes.secondary
            },
        );
    }
    for dragged_pass in [false, true] {
        for item in layout
            .items()
            .iter()
            .filter(|item| item.dragged() == dragged_pass)
        {
            match item.kind() {
                DockItemVisualKind::Separator => {
                    let bounds = item.bounds();
                    fill_round(
                        context,
                        rect(
                            bounds.x + bounds.width * 0.5 - 0.5,
                            bounds.y + 3.0,
                            bounds.x + bounds.width * 0.5 + 0.5,
                            bounds.y + bounds.height - 3.0,
                            0.5,
                        ),
                        if item.dragged() {
                            brushes.accent
                        } else {
                            brushes.secondary
                        },
                    );
                }
                DockItemVisualKind::App => {
                    let bounds = item.bounds();
                    let brush = item.pressed().then_some(brushes.pressed);
                    if let Some(brush) = brush {
                        fill_round(
                            context,
                            rect(
                                bounds.x,
                                bounds.y,
                                bounds.x + bounds.width,
                                bounds.y + bounds.height,
                                control_radius,
                            ),
                            brush,
                        );
                    }
                    let icon_bounds = D2D_RECT_F {
                        left: bounds.x,
                        top: bounds.y,
                        right: bounds.x + bounds.width,
                        bottom: bounds.y + bounds.height,
                    };
                    let drew_native = match item.icon() {
                        DockIcon::WindowsExecutable(source) => {
                            resources.icons.draw_dock(source, icon_bounds)
                        }
                        DockIcon::SystemFallback => false,
                    };
                    if !drew_native {
                        draw_text(
                            context,
                            "\u{ECAA}",
                            resources.formats.icon,
                            D2D_RECT_F {
                                left: bounds.x,
                                top: bounds.y,
                                right: bounds.x + bounds.width,
                                bottom: bounds.y + bounds.height - 3.0,
                            },
                            brushes.primary,
                        );
                    }
                    if item.indicator() != RunningIndicator::Stopped || item.focused() {
                        let indicator = if item.focused() {
                            brushes.focus
                        } else if item.indicator() == RunningIndicator::Focused {
                            brushes.accent
                        } else {
                            brushes.secondary
                        };
                        fill_round(
                            context,
                            rect(
                                bounds.x + bounds.width * 0.5 - 1.5,
                                bounds.y + bounds.height + 3.0,
                                bounds.x + bounds.width * 0.5 + 1.5,
                                bounds.y + bounds.height + 6.0,
                                1.5,
                            ),
                            indicator,
                        );
                    }
                }
            }
        }
    }
}
