use crate::{
    DipPoint, DipRect, TopbarDensity, TopbarModuleStatus, TopbarModuleVisual, TopbarScene,
    layout_topbar_scene,
};
use shell_core::{Popover, TopbarModuleKind};

#[test]
fn lays_out_visible_topbar_modules_in_order_with_overflow() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a compact topbar with more visible modules than a narrow surface can fit.
    let scene = TopbarScene::new(
        TopbarDensity::Compact,
        vec![
            TopbarModuleVisual::new(
                TopbarModuleKind::SystemMenu,
                "Menu",
                "Minha UI",
                TopbarModuleStatus::Neutral,
                Some(Popover::SystemMenu),
            ),
            TopbarModuleVisual::new(
                TopbarModuleKind::Network,
                "Net",
                "12 KB/s",
                TopbarModuleStatus::Good,
                Some(Popover::Network),
            ),
            TopbarModuleVisual::new(
                TopbarModuleKind::Notifications,
                "Alerts",
                "0",
                TopbarModuleStatus::Neutral,
                Some(Popover::Notifications),
            ),
        ],
    );

    // When: the native surface is too narrow for every module.
    let layout = layout_topbar_scene(&scene, DipRect::new(0.0, 0.0, 270.0, 32.0));

    // Then: order is stable, overflow is explicit, and hit testing returns typed intents.
    assert_eq!(
        layout.visible_items()[0].kind(),
        TopbarModuleKind::SystemMenu
    );
    let crate::TopbarOverflow::Collapsed {
        hidden_count,
        bounds,
        intent,
    } = layout.overflow()
    else {
        return Err("expected collapsed topbar overflow".into());
    };
    assert_eq!(hidden_count, 1);
    assert_eq!(intent, Popover::SystemMenu);
    assert_eq!(
        layout.hit_test(DipPoint::new(24.0, 16.0)),
        Some(Popover::SystemMenu)
    );
    assert_eq!(
        layout.hit_test(DipPoint::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0
        )),
        Some(Popover::SystemMenu)
    );
    Ok(())
}

#[test]
fn separates_system_menu_from_right_aligned_status_cluster() {
    // Given: a wide topbar with a system menu and glanceable status modules.
    let scene = TopbarScene::new(
        TopbarDensity::Compact,
        vec![
            TopbarModuleVisual::new(
                TopbarModuleKind::SystemMenu,
                "Menu",
                "Obsidian Glass",
                TopbarModuleStatus::Neutral,
                Some(Popover::SystemMenu),
            ),
            TopbarModuleVisual::new(
                TopbarModuleKind::Network,
                "Network",
                "12 KB/s",
                TopbarModuleStatus::Good,
                Some(Popover::Network),
            ),
            TopbarModuleVisual::new(
                TopbarModuleKind::Clock,
                "Clock",
                "17:33",
                TopbarModuleStatus::Neutral,
                Some(Popover::Calendar),
            ),
        ],
    );

    // When: the modules are laid out across the native surface.
    let layout = layout_topbar_scene(&scene, DipRect::new(0.0, 0.0, 1000.0, 32.0));

    // Then: the app menu stays left while status modules form a right cluster.
    assert_eq!(layout.visible_items()[0].bounds().x, 10.0);
    assert!(layout.visible_items()[1].bounds().x > 700.0);
    assert!(layout.visible_items()[2].bounds().x > layout.visible_items()[1].bounds().x);
    let clock = layout.visible_items()[2].bounds();
    assert_eq!(clock.x + clock.width, 980.0);
    assert_eq!(clock.y, 4.0);
    assert_eq!(clock.height, 24.0);
}

#[test]
fn high_text_scale_collapses_status_before_overlapping_system_menu() {
    let modules = vec![
        TopbarModuleVisual::new(
            TopbarModuleKind::SystemMenu,
            "Menu",
            "Minha UI",
            TopbarModuleStatus::Neutral,
            Some(Popover::SystemMenu),
        ),
        TopbarModuleVisual::new(
            TopbarModuleKind::Network,
            "Network",
            "12 KB/s",
            TopbarModuleStatus::Good,
            Some(Popover::Network),
        ),
        TopbarModuleVisual::new(
            TopbarModuleKind::Notifications,
            "Alerts",
            "Clear",
            TopbarModuleStatus::Neutral,
            Some(Popover::Notifications),
        ),
    ];
    let normal = layout_topbar_scene(
        &TopbarScene::new(TopbarDensity::Compact, modules.clone()),
        DipRect::new(0.0, 0.0, 400.0, 32.0),
    );
    let scaled = layout_topbar_scene(
        &TopbarScene::new(TopbarDensity::Compact, modules).with_text_scale(2.5),
        DipRect::new(0.0, 0.0, 400.0, 68.0),
    );
    assert_eq!(normal.overflow(), crate::TopbarOverflow::None);
    assert!(matches!(
        scaled.overflow(),
        crate::TopbarOverflow::Collapsed { .. }
    ));
    assert!(
        scaled
            .visible_items()
            .iter()
            .all(|item| item.bounds().height == 60.0)
    );
}
