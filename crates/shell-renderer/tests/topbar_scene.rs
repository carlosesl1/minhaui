use shell_core::{Popover, TopbarModuleKind};
use shell_renderer::{
    DipPoint, DipRect, TopbarDensity, TopbarModuleStatus, TopbarModuleVisual, TopbarScene,
    layout_topbar_scene,
};

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
    let shell_renderer::TopbarOverflow::Collapsed {
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
