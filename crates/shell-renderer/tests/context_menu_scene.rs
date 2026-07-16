use crate::{
    ContextMenuEntry, ContextMenuScene, DipPoint, DipRect, Dpi, PhysicalRect,
    context_menu_anchor_rect, context_menu_height_for_entries, context_menu_surface_width,
    layout_context_menu_scene,
};

#[test]
fn context_menu_keeps_a_260_dip_card_and_reserves_shadow_gutters() {
    let entries = vec![
        ContextMenuEntry::action("Open", true),
        ContextMenuEntry::separator(),
        ContextMenuEntry::action("Keep in Dock", true),
        ContextMenuEntry::action("Close Window", false),
    ];
    let scene = ContextMenuScene::new(entries.clone(), Some(0));
    let height = context_menu_height_for_entries(&entries);
    let layout = layout_context_menu_scene(
        &scene,
        DipRect::new(0.0, 0.0, context_menu_surface_width(), height),
    );

    assert_eq!(context_menu_surface_width(), 300.0);
    assert_eq!(layout.card_bounds().width, 260.0);
    assert_eq!(layout.rows().len(), 3);
    assert_eq!(
        layout.rows()[0].bounds(),
        DipRect::new(32.0, 17.0, 236.0, 30.0)
    );
    assert_eq!(
        layout.rows()[1].bounds(),
        DipRect::new(32.0, 56.0, 236.0, 30.0)
    );
    assert_eq!(
        layout.rows()[2].bounds(),
        DipRect::new(32.0, 86.0, 236.0, 30.0)
    );
}

#[test]
fn context_menu_hit_testing_skips_separators_and_uses_semi_open_edges() {
    let entries = vec![
        ContextMenuEntry::action("Open", true),
        ContextMenuEntry::separator(),
        ContextMenuEntry::action("Quit", true),
    ];
    let scene = ContextMenuScene::new(entries.clone(), None);
    let layout = layout_context_menu_scene(
        &scene,
        DipRect::new(
            0.0,
            0.0,
            context_menu_surface_width(),
            context_menu_height_for_entries(&entries),
        ),
    );

    assert_eq!(layout.hit_test(DipPoint::new(40.0, 46.9)), Some(0));
    assert_eq!(layout.hit_test(DipPoint::new(40.0, 47.0)), None);
    assert_eq!(layout.hit_test(DipPoint::new(40.0, 52.0)), None);
    assert_eq!(layout.hit_test(DipPoint::new(40.0, 56.0)), Some(2));
}

#[test]
fn context_menu_anchor_stays_inside_scaled_and_negative_monitor_work_areas() {
    let entries = vec![
        ContextMenuEntry::action("Open", true),
        ContextMenuEntry::separator(),
        ContextMenuEntry::action("Quit", true),
    ];
    let height = context_menu_height_for_entries(&entries);

    for (work, dpi, anchor) in [
        (
            PhysicalRect::new(0, 0, 1920, 1080),
            Dpi::from_raw(96),
            PhysicalRect::new(950, 1010, 1, 1),
        ),
        (
            PhysicalRect::new(-2560, 0, 2560, 1440),
            Dpi::from_raw(144),
            PhysicalRect::new(-1400, 1370, 1, 1),
        ),
        (
            PhysicalRect::new(1920, -200, 3840, 2160),
            Dpi::from_raw(192),
            PhysicalRect::new(4000, 1850, 1, 1),
        ),
    ] {
        let rect = context_menu_anchor_rect(anchor, work, dpi, height);
        assert!(rect.x >= work.x);
        assert!(rect.y >= work.y);
        assert!(rect.x + rect.width <= work.x + work.width);
        assert!(rect.y + rect.height <= work.y + work.height);
        assert!(rect.y + rect.height <= anchor.y + 1);
    }
}
