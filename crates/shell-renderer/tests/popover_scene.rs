use shell_core::Popover;

use crate::{
    DipRect, PopoverContentState, PopoverLayoutStyle, PopoverRow, PopoverScene, PopoverSurfaceSize,
    layout_popover_scene, popover_surface_size,
};

fn system_panel_scene() -> PopoverScene {
    let rows = vec![
        PopoverRow::new("Settings", "Ctrl+,", true).with_icon_glyph("\u{E713}"),
        PopoverRow::new("Task Manager", "", true).with_icon_glyph("\u{E9D9}"),
        PopoverRow::new("Lock", "Win+L", true)
            .with_icon_glyph("\u{E72E}")
            .with_section_start(),
        PopoverRow::new("Sleep", "Requires confirmation", true).with_icon_glyph("\u{E708}"),
        PopoverRow::new("Sign out", "Requires confirmation", true).with_icon_glyph("\u{E8AC}"),
        PopoverRow::new("Restart", "Requires confirmation", true)
            .with_icon_glyph("\u{E777}")
            .with_section_start(),
        PopoverRow::new("Shut down", "Requires confirmation", true).with_icon_glyph("\u{E7E8}"),
    ];
    PopoverScene::new(
        Popover::SystemMenu,
        "Minha UI",
        PopoverContentState::Ready,
        rows,
        Some(0),
    )
    .with_layout_style(PopoverLayoutStyle::SystemPanel)
    .with_anchor_x(62.0)
}

fn rect_stays_inside(inner: DipRect, outer: DipRect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

#[test]
fn scrolled_popover_keeps_rows_inside_the_surface() {
    let rows = (0..24)
        .map(|index| PopoverRow::new(&format!("App {index:02}"), "", true))
        .collect();
    let scene = PopoverScene::new(
        Popover::BackgroundApps,
        "Aplicativos em segundo plano",
        PopoverContentState::Ready,
        rows,
        Some(18),
    )
    .with_scroll_offset(8);

    let layout = layout_popover_scene(&scene, DipRect::new(0.0, 0.0, 244.0, 420.0));

    assert_eq!(layout.rows().first().unwrap().index(), 8);
    let last = layout.rows().last().unwrap().bounds();
    assert!(last.y + last.height <= 415.0);
}

#[test]
fn background_row_carries_only_an_optional_icon_source() {
    let row = PopoverRow::new("Steam", "", true).with_icon_source(Some(r"C:\Steam.exe".to_owned()));

    assert_eq!(row.icon_source(), Some(r"C:\Steam.exe"));
}

#[test]
fn system_panel_has_title_spacing_group_dividers_and_288_dip_width() {
    let scene = system_panel_scene();
    let size = popover_surface_size(&scene);
    let layout = layout_popover_scene(&scene, DipRect::new(0.0, 0.0, size.width(), size.height()));

    assert_eq!(size, PopoverSurfaceSize::new(288.0, 372.0));
    assert_eq!(
        layout.title_bounds(),
        Some(DipRect::new(16.0, 20.0, 256.0, 28.0))
    );
    assert_eq!(layout.rows().len(), 7);
    assert!(layout.rows().iter().all(|row| row.bounds().height == 40.0));
    assert_eq!(layout.separators().len(), 2);
    assert_eq!(layout.rows()[0].bounds().y, 60.0);
    assert_eq!(layout.rows()[2].bounds().y, 152.0);
    assert_eq!(layout.rows()[5].bounds().y, 284.0);
    assert_eq!(
        layout.separators()[0].bounds(),
        DipRect::new(16.0, 145.5, 256.0, 1.0)
    );
    assert_eq!(
        layout.separators()[1].bounds(),
        DipRect::new(16.0, 277.5, 256.0, 1.0)
    );
    assert!(scene.rows().iter().all(|row| row.icon_glyph().is_some()));
}

#[test]
fn system_panel_notch_and_rows_stay_inside_the_surface() {
    let scene = system_panel_scene();
    let size = popover_surface_size(&scene);
    let surface = DipRect::new(0.0, 0.0, size.width(), size.height());

    let layout = layout_popover_scene(&scene, surface);
    let notch = layout.notch().expect("SystemPanel should expose its notch");

    assert_eq!(size, PopoverSurfaceSize::new(288.0, 372.0));
    assert_eq!(notch.tip_x(), 62.0);
    assert!(rect_stays_inside(notch.bounds(), surface));
    assert!(
        layout
            .rows()
            .iter()
            .all(|row| rect_stays_inside(row.bounds(), surface))
    );
}

#[test]
fn compact_popover_keeps_existing_244_dip_width_and_24_dip_rows() {
    let scene = PopoverScene::new(
        Popover::Network,
        "Network",
        PopoverContentState::Ready,
        vec![PopoverRow::new("Online", "", true)],
        Some(0),
    );
    let size = popover_surface_size(&scene);
    let layout = layout_popover_scene(&scene, DipRect::new(0.0, 0.0, size.width(), size.height()));

    assert_eq!(size.width(), 244.0);
    assert_eq!(layout.rows()[0].bounds().height, 24.0);
    assert_eq!(layout.title_bounds(), None);
    assert!(layout.separators().is_empty());
    assert_eq!(layout.notch(), None);
}

#[test]
fn popover_scene_defaults_and_sanitizes_presentation_metadata() {
    let row = PopoverRow::new("Online", "", true);
    assert_eq!(row.icon_glyph(), None);
    assert!(!row.section_start());

    let scene = PopoverScene::new(
        Popover::Network,
        "Network",
        PopoverContentState::Ready,
        vec![row],
        None,
    );
    assert_eq!(scene.layout_style(), PopoverLayoutStyle::Compact);
    assert_eq!(scene.anchor_x(), None);
    assert_eq!(scene.clone().with_anchor_x(62.0).anchor_x(), Some(62.0));
    assert_eq!(scene.with_anchor_x(f32::NAN).anchor_x(), None);
}
