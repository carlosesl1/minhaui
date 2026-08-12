use crate::{
    DipPoint, DipRect, SETTINGS_CONTENT_MAX_WIDTH, SettingsControl, SettingsControlId,
    SettingsControlKind, SettingsFocus, SettingsHit, SettingsLayoutMode, SettingsNavigationItem,
    SettingsScene, SettingsSectionId, layout_settings_scene,
};

const DOCK: SettingsSectionId = SettingsSectionId::new(1);
const TOPBAR: SettingsSectionId = SettingsSectionId::new(2);
const SIZE: SettingsControlId = SettingsControlId::new(101);
const MAGNIFICATION: SettingsControlId = SettingsControlId::new(102);

fn navigation() -> Vec<SettingsNavigationItem> {
    vec![
        SettingsNavigationItem::new(DOCK, "Dock", "Size and behavior", true).with_icon_glyph("D"),
        SettingsNavigationItem::new(TOPBAR, "Top bar", "Modules and density", true)
            .with_modified(true),
    ]
}

fn controls() -> Vec<SettingsControl> {
    vec![
        SettingsControl::new(
            SIZE,
            "Icon size",
            "Changes the resting size of Dock icons.",
            "48 DIP",
            SettingsControlKind::Slider { position: 50 },
            true,
            true,
        ),
        SettingsControl::new(
            MAGNIFICATION,
            "Magnification",
            "Enlarge nearby icons while pointing at the Dock.",
            "On",
            SettingsControlKind::Toggle { checked: true },
            true,
            false,
        ),
    ]
}

fn active_scene(dirty: bool) -> SettingsScene {
    SettingsScene::from_navigation("Settings", navigation())
        .with_active_section(Some(DOCK))
        .with_controls(controls())
        .with_focus(Some(SettingsFocus::Control(SIZE)))
        .with_dirty(dirty)
}

fn center(bounds: DipRect) -> DipPoint {
    DipPoint::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    )
}

#[test]
fn standard_width_keeps_navigation_and_content_visible() {
    let scene = active_scene(true);
    let layout = layout_settings_scene(&scene, DipRect::new(0.0, 0.0, 992.0, 620.0));

    assert_eq!(layout.mode(), SettingsLayoutMode::Split);
    assert!(layout.navigation_bounds().is_some());
    assert!(layout.content_bounds().is_some());
    assert!(layout.back_bounds().is_none());
    assert_eq!(layout.navigation().len(), 2);
    assert_eq!(layout.controls().len(), 2);
    assert_eq!(
        layout.hit_test(center(layout.navigation()[1].bounds())),
        Some(SettingsHit::Navigation(TOPBAR))
    );
    assert_eq!(
        layout.hit_test(center(layout.controls()[0].bounds())),
        Some(SettingsHit::Control(SIZE))
    );
    assert_eq!(
        layout.hit_test(center(layout.apply_bounds().expect("apply bounds"))),
        Some(SettingsHit::Apply)
    );
}

#[test]
fn compact_scene_without_selection_is_navigation_only() {
    let scene = SettingsScene::from_navigation("Settings", navigation());
    let layout = layout_settings_scene(&scene, DipRect::new(0.0, 0.0, 680.0, 560.0));

    assert_eq!(layout.mode(), SettingsLayoutMode::NavigationOnly);
    assert!(layout.navigation_bounds().is_some());
    assert!(layout.content_bounds().is_none());
    assert!(layout.back_bounds().is_none());
    assert_eq!(layout.navigation().len(), 2);
    assert!(layout.controls().is_empty());
}

#[test]
fn compact_selected_scene_becomes_content_page_with_back_target() {
    let scene = active_scene(true);
    let layout = layout_settings_scene(&scene, DipRect::new(0.0, 0.0, 680.0, 560.0));

    assert_eq!(layout.mode(), SettingsLayoutMode::ContentOnly);
    assert!(layout.navigation_bounds().is_none());
    assert!(layout.navigation().is_empty());
    assert!(layout.content_bounds().is_some());
    assert_eq!(layout.controls().len(), 2);
    assert_eq!(
        layout.hit_test(center(layout.back_bounds().expect("back bounds"))),
        Some(SettingsHit::Back)
    );
}

#[test]
fn disabled_controls_and_clean_transaction_actions_do_not_hit() {
    let disabled = SettingsControl::new(
        SIZE,
        "Icon size",
        "Unavailable while Windows high contrast is active.",
        "Managed by Windows",
        SettingsControlKind::ReadOnly,
        false,
        false,
    );
    let scene = SettingsScene::from_navigation("Settings", navigation())
        .with_active_section(Some(DOCK))
        .with_controls(vec![disabled]);
    let layout = layout_settings_scene(&scene, DipRect::new(0.0, 0.0, 992.0, 620.0));

    assert_eq!(layout.hit_test(center(layout.controls()[0].bounds())), None);
    assert_eq!(
        layout.hit_test(center(layout.apply_bounds().expect("apply bounds"))),
        None
    );
    assert_eq!(
        layout.hit_test(center(layout.cancel_bounds().expect("cancel bounds"))),
        None
    );
    assert_eq!(
        layout.hit_test(center(layout.reset_bounds().expect("reset bounds"))),
        Some(SettingsHit::Reset)
    );
}

#[test]
fn content_width_is_bounded_on_wide_settings_windows() {
    let layout =
        layout_settings_scene(&active_scene(false), DipRect::new(0.0, 0.0, 1_600.0, 900.0));
    let content = layout.content_bounds().expect("content bounds");

    assert_eq!(content.width, SETTINGS_CONTENT_MAX_WIDTH);
}

#[test]
fn scroll_offsets_preserve_stable_hit_identities() {
    let scene = active_scene(true).with_scroll_offsets(1, 1);
    let layout = layout_settings_scene(&scene, DipRect::new(0.0, 0.0, 992.0, 620.0));

    assert_eq!(layout.navigation()[0].section(), TOPBAR);
    assert_eq!(layout.controls()[0].id(), MAGNIFICATION);
    assert_eq!(
        layout.hit_test(center(layout.controls()[0].bounds())),
        Some(SettingsHit::Control(MAGNIFICATION))
    );
}
