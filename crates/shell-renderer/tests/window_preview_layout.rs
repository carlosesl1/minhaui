use crate::{
    DipRect, DockIcon, Dpi, PhysicalRect, PreviewCardVisual, PreviewPlacementInput,
    PreviewSourceSize, WindowPreviewCapture, WindowPreviewScene, layout_window_preview,
    place_window_preview, preview_panel_size, preview_panel_size_for_scene,
};
use shell_core::{DockItemId, WindowId};

fn scene(count: u64) -> WindowPreviewScene {
    WindowPreviewScene::new(
        DockItemId::new(3),
        "Explorer",
        (0..count)
            .map(|index| {
                PreviewCardVisual::new(
                    WindowId::new(index + 1),
                    "Explorer window",
                    WindowPreviewCapture::DwmThumbnail,
                )
            })
            .collect(),
    )
    .with_icon(DockIcon::SystemFallback)
}

#[test]
fn four_window_layout_is_a_contained_two_by_two_grid() {
    // Given: four visible windows and a surface sized for their panel.
    let scene = scene(4);
    let size = preview_panel_size(4, false);

    // When: renderer geometry is calculated.
    let layout = layout_window_preview(&scene, DipRect::new(0.0, 0.0, size.width, size.height));

    // Then: four non-overlapping cards remain inside the panel.
    assert_eq!(layout.cards().len(), 4);
    for card in layout.cards() {
        assert!(card.card().x >= layout.panel().x);
        assert!(card.card().y >= layout.panel().y);
        assert!(card.card().x + card.card().width <= layout.panel().x + layout.panel().width);
        assert!(card.card().y + card.card().height <= layout.panel().y + layout.panel().height);
    }
    assert!(layout.cards()[0].card().x < layout.cards()[1].card().x);
    assert!(layout.cards()[0].card().y < layout.cards()[2].card().y);
}

#[test]
fn placement_clamps_to_negative_coordinate_work_area() {
    // Given: an invoking icon near the right edge of a negative-coordinate monitor.
    let work = PhysicalRect::new(-1920, 0, 1920, 1080);
    let dock = PhysicalRect::new(-420, 1010, 380, 55);
    let anchor = PhysicalRect::new(-85, 1018, 36, 36);
    let size = preview_panel_size(4, false);

    // When: the panel is placed at 150 percent DPI.
    let placed = place_window_preview(
        PreviewPlacementInput {
            anchor,
            dock,
            work,
            dpi: Dpi::from_raw(144),
        },
        size,
    );

    // Then: the complete panel respects the 16-DIP work-area margin.
    let margin = 24;
    assert!(placed.x >= work.x + margin);
    assert!(placed.x + placed.width <= work.x + work.width - margin);
    assert!(placed.y >= work.y + margin);
    assert!(placed.y + placed.height <= work.y + work.height - margin);
}

#[test]
fn header_keeps_the_close_button_above_the_native_thumbnail() {
    let scene = scene(1);
    let size = preview_panel_size(1, false);
    let layout = layout_window_preview(&scene, DipRect::new(0.0, 0.0, size.width, size.height));
    let card = layout.cards()[0];

    assert_eq!(card.close().x, card.card().x + card.card().width - 32.0);
    assert_eq!(card.close().y, card.card().y + 6.0);
    assert_eq!(card.close().width, 24.0);
    assert_eq!(card.close().height, 24.0);
    assert!(card.close().y + card.close().height <= card.thumbnail().y);
    assert!(card.title().x + card.title().width <= card.close().x - 8.0);
}

#[test]
fn portrait_window_keeps_standard_height_and_shrinks_the_card_to_its_aspect_ratio() {
    let portrait = WindowPreviewScene::new(
        DockItemId::new(3),
        "Calculator",
        vec![
            PreviewCardVisual::new(
                WindowId::new(1),
                "Calculator",
                WindowPreviewCapture::DwmThumbnail,
            )
            .with_source_size(PreviewSourceSize::new(320, 531)),
        ],
    );
    let size = preview_panel_size_for_scene(&portrait);
    let layout = layout_window_preview(&portrait, DipRect::new(0.0, 0.0, size.width, size.height));
    let thumbnail = layout.cards()[0].thumbnail();

    assert!((thumbnail.width - 84.37).abs() < 0.01);
    assert_eq!(thumbnail.height, 140.0);
    assert_eq!(layout.cards()[0].card().width, 120.0);
    assert_eq!(size.width, 144.0);
    assert_eq!(size.height, preview_panel_size(1, false).height);
    assert!(thumbnail.x > layout.cards()[0].card().x);
}
