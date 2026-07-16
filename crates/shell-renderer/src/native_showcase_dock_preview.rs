use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;

use crate::native_showcase_primitives::{fill_round, rect};
use crate::native_showcase_resources::DockBrushes;
use crate::{
    DipRect, DockScene, WindowPreviewRenderKind, layout_dock_scene, layout_window_previews,
};

pub(crate) fn draw_unavailable_previews(
    context: &ID2D1DeviceContext,
    surface: DipRect,
    scene: &DockScene,
    brushes: &DockBrushes<'_>,
) {
    let unavailable = layout_window_previews(scene, surface)
        .iter()
        .any(|preview| matches!(preview.kind(), WindowPreviewRenderKind::Unavailable(_)));
    if !unavailable {
        return;
    }
    let Some(hovered) = layout_dock_scene(scene, surface)
        .items()
        .iter()
        .find(|item| item.hovered())
        .map(|item| item.bounds())
    else {
        return;
    };
    fill_round(
        context,
        rect(
            hovered.x + hovered.width - 9.0,
            hovered.y + 3.0,
            hovered.x + hovered.width - 3.0,
            hovered.y + 9.0,
            3.0,
        ),
        brushes.warning,
    );
}
