use shell_renderer::{
    DipPoint, DipRect, DockAlignment, DockItemVisual, DockItemVisualKind, DockLayoutConfig,
    DockScene, PreviewUnavailableReason, RunningIndicator, WindowPreviewRenderKind,
    WindowPreviewVisual, layout_dock_scene, layout_window_previews,
};

#[test]
fn lays_out_apps_separators_and_running_indicators_when_center_aligned() {
    // Given: a center-aligned dock scene with apps and a separator.
    let scene = DockScene::new(
        DockLayoutConfig::new(DockAlignment::Center)
            .with_item_size(48.0)
            .with_spacing(8.0)
            .with_padding(12.0)
            .with_magnified_item_size(72.0),
        vec![
            DockItemVisual::app(1, "Files", RunningIndicator::Running),
            DockItemVisual::separator(2),
            DockItemVisual::app(3, "Notes", RunningIndicator::Focused),
        ],
    );

    // When: the scene is laid out in the native dock surface.
    let layout = layout_dock_scene(&scene, DipRect::new(0.0, 0.0, 260.0, 96.0));

    // Then: separator geometry is narrower and the focused app exposes an indicator.
    assert_eq!(layout.items().len(), 3);
    assert_eq!(layout.items()[1].kind(), DockItemVisualKind::Separator);
    assert!(layout.items()[1].bounds().width < layout.items()[0].bounds().width);
    assert_eq!(layout.items()[2].indicator(), RunningIndicator::Focused);
    assert_eq!(layout.hit_test(DipPoint::new(118.0, 48.0)), Some(2));
}

#[test]
fn magnifies_only_the_hover_neighborhood_without_changing_input_order() {
    // Given: three dock apps with the pointer over the middle app.
    let scene = DockScene::new(
        DockLayoutConfig::new(DockAlignment::Center)
            .with_item_size(50.0)
            .with_spacing(6.0)
            .with_padding(10.0)
            .with_magnified_item_size(76.0),
        vec![
            DockItemVisual::app(10, "One", RunningIndicator::Stopped),
            DockItemVisual::app(11, "Two", RunningIndicator::Running),
            DockItemVisual::app(12, "Three", RunningIndicator::Stopped),
        ],
    )
    .with_hovered_item(Some(11));

    // When: the scene is laid out.
    let layout = layout_dock_scene(&scene, DipRect::new(0.0, 0.0, 260.0, 96.0));

    // Then: the hovered item is visually larger but hit order stays stable.
    assert_eq!(layout.items()[0].id(), 10);
    assert_eq!(layout.items()[1].id(), 11);
    assert_eq!(layout.items()[2].id(), 12);
    assert!(layout.items()[1].bounds().height > layout.items()[0].bounds().height);
}

#[test]
fn alignment_changes_the_visual_origin_inside_the_same_surface() {
    // Given: the same scene rendered with different alignments.
    let left = DockScene::new(
        DockLayoutConfig::new(DockAlignment::Left)
            .with_item_size(40.0)
            .with_spacing(6.0)
            .with_padding(10.0)
            .with_magnified_item_size(56.0),
        vec![DockItemVisual::app(1, "A", RunningIndicator::Stopped)],
    );
    let right = DockScene::new(
        DockLayoutConfig::new(DockAlignment::Right)
            .with_item_size(40.0)
            .with_spacing(6.0)
            .with_padding(10.0)
            .with_magnified_item_size(56.0),
        vec![DockItemVisual::app(1, "A", RunningIndicator::Stopped)],
    );

    // When: both scenes are laid out in the same bounds.
    let left_layout = layout_dock_scene(&left, DipRect::new(0.0, 0.0, 220.0, 80.0));
    let right_layout = layout_dock_scene(&right, DipRect::new(0.0, 0.0, 220.0, 80.0));

    // Then: alignment moves the item origin while preserving item size.
    assert!(left_layout.items()[0].bounds().x < right_layout.items()[0].bounds().x);
    assert_eq!(
        left_layout.items()[0].bounds().width,
        right_layout.items()[0].bounds().width
    );
}

#[test]
fn restricted_preview_has_visible_fallback_layout() {
    // Given: a scene with a capture-restricted window preview.
    let scene = DockScene::new(
        DockLayoutConfig::new(DockAlignment::Center),
        vec![DockItemVisual::app(1, "Notes", RunningIndicator::Running)],
    )
    .with_window_previews(vec![WindowPreviewVisual::restricted(
        shell_core::WindowId::new(44),
        shell_core::DockItemId::new(1),
        PreviewUnavailableReason::CaptureRestricted,
    )]);

    // When: native preview render layout is derived for the dock surface.
    let previews = layout_window_previews(&scene, DipRect::new(0.0, 0.0, 320.0, 180.0));

    // Then: the restricted capture is a visible unavailable fallback, not omitted.
    assert_eq!(previews.len(), 1);
    assert_eq!(
        previews[0].kind(),
        WindowPreviewRenderKind::Unavailable(PreviewUnavailableReason::CaptureRestricted)
    );
    assert!(previews[0].bounds().width > 0.0);
    assert!(previews[0].bounds().height > 0.0);
}
