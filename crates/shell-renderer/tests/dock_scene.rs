use crate::{
    DipPoint, DipRect, DockAlignment, DockIcon, DockItemVisual, DockItemVisualKind,
    DockLayoutConfig, DockScene, PreviewUnavailableReason, RunningIndicator,
    WindowPreviewRenderKind, WindowPreviewVisual, dock_material_bounds, dock_scene_max_width,
    dock_scene_width, layout_dock_scene, layout_window_previews,
};

#[test]
fn preferred_dock_width_tracks_the_number_of_items() {
    let short = DockScene::new(
        DockLayoutConfig::default(),
        vec![DockItemVisual::app(1, "One", RunningIndicator::Stopped)],
    );
    let long = DockScene::new(
        DockLayoutConfig::default(),
        (1..=6)
            .map(|id| DockItemVisual::app(id, "App", RunningIndicator::Stopped))
            .collect(),
    );

    assert_eq!(dock_scene_width(&short), 52.0);
    assert_eq!(dock_scene_width(&long), 277.0);
}

#[test]
fn default_layout_matches_the_responsive_figma_geometry() {
    // Given: the default native dock configuration from the approved design.
    let scene = DockScene::new(
        DockLayoutConfig::default(),
        vec![
            DockItemVisual::app(1, "One", RunningIndicator::Stopped),
            DockItemVisual::app(2, "Two", RunningIndicator::Running),
        ],
    );

    // When: the dock is laid out in its 55-DIP resting surface.
    let layout = layout_dock_scene(&scene, DipRect::new(0.0, 0.0, 97.0, 55.0));

    // Then: icon slots, padding and gaps match node 682:3932.
    assert_eq!(dock_scene_width(&scene), 97.0);
    assert_eq!(
        layout.items()[0].bounds(),
        DipRect::new(8.0, 8.0, 36.0, 36.0)
    );
    assert_eq!(
        layout.items()[1].bounds(),
        DipRect::new(53.0, 8.0, 36.0, 36.0)
    );
}

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
    assert_eq!(layout.items()[1].bounds().height, 48.0);
    assert_eq!(layout.items()[2].indicator(), RunningIndicator::Focused);
    assert_eq!(layout.hit_test(DipPoint::new(130.0, 48.0)), Some(2));
}

#[test]
fn dragged_item_tracks_pointer_with_lift_without_leaving_dock_padding() {
    let scene = DockScene::new(
        DockLayoutConfig::default(),
        vec![
            DockItemVisual::app(1, "One", RunningIndicator::Stopped),
            DockItemVisual::app(2, "Two", RunningIndicator::Stopped),
            DockItemVisual::app(3, "Three", RunningIndicator::Stopped),
        ],
    )
    .with_dragged_item(Some(2), 250.0);
    let surface = DipRect::new(0.0, 0.0, 300.0, 64.0);

    let layout = layout_dock_scene(&scene, surface);
    let dragged = layout
        .items()
        .iter()
        .find(|item| item.id() == 2)
        .expect("dragged item should be laid out");

    assert!(dragged.dragged());
    assert!((dragged.bounds().x + dragged.bounds().width / 2.0 - 250.0).abs() < 0.01);
    assert!(dragged.bounds().y < surface.y + scene.config().padding());
    assert!(
        dragged.bounds().x + dragged.bounds().width <= surface.width - scene.config().padding()
    );
}

#[test]
fn magnifies_only_the_hover_neighborhood_without_changing_input_order() {
    // Given: five dock apps with the pointer over the middle app.
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
            DockItemVisual::app(13, "Four", RunningIndicator::Stopped),
            DockItemVisual::app(14, "Five", RunningIndicator::Stopped),
        ],
    )
    .with_hovered_item(Some(12));

    // When: the scene is laid out.
    let layout = layout_dock_scene(&scene, DipRect::new(0.0, 0.0, 260.0, 96.0));

    // Then: the hovered item is visually larger but hit order stays stable.
    assert_eq!(layout.items()[0].id(), 10);
    assert_eq!(layout.items()[1].id(), 11);
    assert_eq!(layout.items()[4].id(), 14);
    assert_eq!(layout.items()[2].bounds().height, 61.0);
    assert_eq!(layout.items()[1].bounds().height, 55.5);
    assert_eq!(layout.items()[3].bounds().height, 55.5);
    assert_eq!(layout.items()[0].bounds().height, 50.0);
    assert_eq!(layout.items()[4].bounds().height, 50.0);
    assert_eq!(layout.items()[2].bounds().x, 99.5);
}

#[test]
fn hover_strength_eases_between_resting_and_full_magnification() {
    let config = DockLayoutConfig::default();
    let items = vec![DockItemVisual::app(1, "One", RunningIndicator::Stopped)];
    let resting = DockScene::new(config, items.clone())
        .with_hovered_item(Some(1))
        .with_hover_strength(0.0);
    let partial = DockScene::new(config, items.clone())
        .with_hovered_item(Some(1))
        .with_hover_strength(0.5);
    let full = DockScene::new(config, items).with_hovered_item(Some(1));
    let surface = DipRect::new(0.0, 0.0, 80.0, 55.0);

    let resting_size = layout_dock_scene(&resting, surface).items()[0]
        .bounds()
        .height;
    let partial_size = layout_dock_scene(&partial, surface).items()[0]
        .bounds()
        .height;
    let full_size = layout_dock_scene(&full, surface).items()[0].bounds().height;

    assert_eq!(resting_size, config.item_size());
    assert!(partial_size > resting_size);
    assert!(partial_size < full_size);
}

#[test]
fn continuous_magnification_is_symmetric_between_icon_centers() {
    let config = DockLayoutConfig::default();
    let scene = DockScene::new(
        config,
        vec![
            DockItemVisual::app(1, "One", RunningIndicator::Stopped),
            DockItemVisual::app(2, "Two", RunningIndicator::Stopped),
            DockItemVisual::app(3, "Three", RunningIndicator::Stopped),
        ],
    )
    .with_hovered_item(Some(1))
    .with_hover_position_x(48.5);

    let layout = layout_dock_scene(&scene, DipRect::new(0.0, 0.0, 142.0, 55.0));
    let first = layout.items()[0].bounds();
    let second = layout.items()[1].bounds();
    let third = layout.items()[2].bounds();

    assert!((first.width - second.width).abs() < 0.01);
    assert!(first.width > config.item_size());
    assert!(first.width < config.magnified_item_size());
    assert!(third.width < second.width);
}

#[test]
fn continuous_magnification_pushes_neighbors_without_changing_hit_slots() {
    let config = DockLayoutConfig::default();
    let items = vec![
        DockItemVisual::app(1, "One", RunningIndicator::Stopped),
        DockItemVisual::app(2, "Two", RunningIndicator::Stopped),
        DockItemVisual::app(3, "Three", RunningIndicator::Stopped),
    ];
    let resting = DockScene::new(config, items.clone());
    let magnified = DockScene::new(config, items)
        .with_hovered_item(Some(2))
        .with_hover_position_x(71.0);
    let surface = DipRect::new(0.0, 0.0, 142.0, 55.0);

    let resting_layout = layout_dock_scene(&resting, surface);
    let magnified_layout = layout_dock_scene(&magnified, surface);
    let left = magnified_layout.items()[0].bounds();
    let center = magnified_layout.items()[1].bounds();
    let right = magnified_layout.items()[2].bounds();

    assert!(left.x < resting_layout.items()[0].bounds().x);
    assert!(right.x > resting_layout.items()[2].bounds().x);
    assert!(left.x + left.width < center.x);
    assert!(center.x + center.width < right.x);
    assert_eq!(
        magnified_layout.hit_test(DipPoint::new(26.0, 26.0)),
        Some(1)
    );
    assert_eq!(
        magnified_layout.hit_test(DipPoint::new(71.0, 26.0)),
        Some(2)
    );
    assert_eq!(
        magnified_layout.hit_test(DipPoint::new(116.0, 26.0)),
        Some(3)
    );
}

#[test]
fn separator_keeps_a_thin_visual_but_has_a_forgiving_drag_target() {
    let scene = DockScene::new(
        DockLayoutConfig::default(),
        vec![
            DockItemVisual::app(1, "One", RunningIndicator::Stopped),
            DockItemVisual::separator(90),
            DockItemVisual::app(2, "Two", RunningIndicator::Stopped),
        ],
    );
    let layout = layout_dock_scene(&scene, DipRect::new(0.0, 0.0, 142.0, 55.0));
    let separator = layout.items()[1].bounds();

    assert!(separator.width < 12.0);
    assert_eq!(
        layout.hit_test(DipPoint::new(separator.x - 4.0, separator.y + 18.0)),
        Some(90)
    );
    assert_eq!(
        layout.hit_test(DipPoint::new(
            separator.x + separator.width + 4.0,
            separator.y + 18.0,
        )),
        Some(90)
    );
}

#[test]
fn layout_applies_transient_slot_offsets_without_moving_the_dragged_item() {
    let scene = DockScene::new(
        DockLayoutConfig::default(),
        vec![
            DockItemVisual::app(2, "Two", RunningIndicator::Stopped),
            DockItemVisual::app(1, "One", RunningIndicator::Stopped),
        ],
    )
    .with_item_offsets(vec![(1, -45.0), (2, 45.0)])
    .with_dragged_item(Some(2), 42.0);
    let layout = layout_dock_scene(&scene, DipRect::new(0.0, 0.0, 320.0, 55.0));
    let dragged = layout.items().iter().find(|item| item.id() == 2).unwrap();
    let neighbor = layout.items().iter().find(|item| item.id() == 1).unwrap();

    assert!((dragged.bounds().x + dragged.bounds().width / 2.0 - 42.0).abs() < 0.01);
    assert!(neighbor.bounds().x < 150.0);
}

#[test]
fn animated_dock_material_keeps_eight_dip_around_moving_icons() {
    let config = DockLayoutConfig::default();
    let items = vec![
        DockItemVisual::app(1, "One", RunningIndicator::Stopped),
        DockItemVisual::app(2, "Two", RunningIndicator::Stopped),
        DockItemVisual::app(3, "Three", RunningIndicator::Stopped),
    ];
    let resting_scene = DockScene::new(config, items.clone());
    let surface_width = dock_scene_max_width(&resting_scene);
    let surface = DipRect::new(0.0, 0.0, surface_width, 55.0);
    let resting_material = dock_material_bounds(&resting_scene, surface);
    let active_scene = DockScene::new(config, items)
        .with_hovered_item(Some(2))
        .with_hover_position_x(surface_width / 2.0);
    let active_layout = layout_dock_scene(&active_scene, surface);
    let active_material = dock_material_bounds(&active_scene, surface);
    let first = active_layout.items().first().expect("first icon").bounds();
    let last = active_layout.items().last().expect("last icon").bounds();

    assert_eq!(resting_material.width, dock_scene_width(&resting_scene));
    assert!(active_material.width > resting_material.width);
    assert!(active_material.width <= surface.width);
    assert!((first.x - active_material.x - 8.0).abs() < 0.01);
    assert!((active_material.x + active_material.width - (last.x + last.width) - 8.0).abs() < 0.01);
}

#[test]
fn preserves_a_typed_platform_icon_for_native_rendering() {
    // Given: a dock item associated with a Windows file application.
    let visual = DockItemVisual::app_with_icon(
        7,
        "Files",
        RunningIndicator::Running,
        DockIcon::windows_executable("C:\\Windows\\explorer.exe"),
    );

    // When: the renderer reads the visual item.
    let icon = visual.icon();

    // Then: the icon remains typed instead of becoming display text or an asset path.
    assert_eq!(
        icon,
        &DockIcon::windows_executable("C:\\Windows\\explorer.exe")
    );
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
