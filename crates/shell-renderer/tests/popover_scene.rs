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

fn balanced_apps_scene() -> PopoverScene {
    balanced_apps_scene_with_focus(Some(0))
}

fn balanced_apps_scene_with_focus(focused: Option<usize>) -> PopoverScene {
    let rows = (0..12)
        .map(|index| {
            PopoverRow::new(
                if index == 0 {
                    "A very long background application label that should trim"
                } else {
                    "Background app"
                },
                "",
                true,
            )
            .with_icon_source(Some(format!("shell:appsfolder\\App{index:02}")))
        })
        .collect();
    PopoverScene::new(
        Popover::BackgroundApps,
        "Background apps",
        PopoverContentState::Ready,
        rows,
        focused,
    )
    .with_layout_style(PopoverLayoutStyle::BalancedApps)
    .with_header_detail("12")
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
fn balanced_apps_uses_fixed_header_and_visible_row_budget() {
    let scene = balanced_apps_scene();
    let size = popover_surface_size(&scene);
    let layout = layout_popover_scene(&scene, DipRect::new(0.0, 0.0, size.width(), size.height()));

    assert_eq!(size, PopoverSurfaceSize::new(288.0, 388.0));
    assert_eq!(scene.header_detail(), Some("12"));
    assert_eq!(layout.rows().len(), 8);
    assert!(layout.rows().iter().all(|row| row.bounds().height == 40.0));
    assert_eq!(
        layout.rows()[0].bounds(),
        DipRect::new(16.0, 60.0, 256.0, 40.0)
    );
    assert_eq!(
        layout.icon_bounds(0),
        Some(DipRect::new(18.0, 66.0, 28.0, 28.0))
    );
}

#[test]
fn balanced_apps_long_label_uses_all_remaining_row_width() {
    let scene = balanced_apps_scene();
    let size = popover_surface_size(&scene);
    let layout = layout_popover_scene(&scene, DipRect::new(0.0, 0.0, size.width(), size.height()));

    assert_eq!(
        layout.label_bounds(0),
        Some(DipRect::new(56.0, 60.0, 216.0, 40.0))
    );
}

#[test]
fn external_active_apps_row_reuses_focus_band_without_geometry_changes() {
    let focused = balanced_apps_scene();
    let externally_active =
        balanced_apps_scene_with_focus(Some(3)).with_external_active_row(Some(0));
    let focused_size = popover_surface_size(&focused);
    let external_size = popover_surface_size(&externally_active);
    let focused_layout = layout_popover_scene(
        &focused,
        DipRect::new(0.0, 0.0, focused_size.width(), focused_size.height()),
    );
    let external_layout = layout_popover_scene(
        &externally_active,
        DipRect::new(0.0, 0.0, external_size.width(), external_size.height()),
    );

    assert_eq!(focused_size, external_size);
    assert_eq!(focused_layout.rows().len(), external_layout.rows().len());
    for (focused_row, external_row) in focused_layout.rows().iter().zip(external_layout.rows()) {
        assert_eq!(focused_row.index(), external_row.index());
        assert_eq!(focused_row.bounds(), external_row.bounds());
    }
    assert!(focused_layout.rows()[0].focused());
    assert!(!focused_layout.rows()[0].externally_active());
    assert!(!external_layout.rows()[0].focused());
    assert!(external_layout.rows()[0].externally_active());
    assert!(external_layout.rows()[3].focused());
}

#[test]
fn external_active_metadata_does_not_change_compact_or_system_geometry() {
    let compact = PopoverScene::new(
        Popover::Network,
        "Network",
        PopoverContentState::Ready,
        vec![PopoverRow::new("Online", "", true)],
        Some(0),
    )
    .with_external_active_row(Some(0));
    let system = system_panel_scene().with_external_active_row(Some(2));

    let compact_size = popover_surface_size(&compact);
    let compact_layout = layout_popover_scene(
        &compact,
        DipRect::new(0.0, 0.0, compact_size.width(), compact_size.height()),
    );
    assert_eq!(compact_size.width(), 244.0);
    assert!(!compact_layout.rows()[0].externally_active());

    let system_size = popover_surface_size(&system);
    let system_layout = layout_popover_scene(
        &system,
        DipRect::new(0.0, 0.0, system_size.width(), system_size.height()),
    );
    assert_eq!(system_size, PopoverSurfaceSize::new(288.0, 372.0));
    assert!(
        system_layout
            .rows()
            .iter()
            .all(|row| !row.externally_active())
    );
}

#[test]
fn balanced_apps_rowless_states_reserve_a_visible_status_row() {
    for state in [
        PopoverContentState::Loading,
        PopoverContentState::Empty,
        PopoverContentState::Error,
        PopoverContentState::Offline,
    ] {
        let scene = PopoverScene::new(
            Popover::BackgroundApps,
            "Background apps",
            state,
            Vec::new(),
            None,
        )
        .with_layout_style(PopoverLayoutStyle::BalancedApps);
        let size = popover_surface_size(&scene);
        let layout =
            layout_popover_scene(&scene, DipRect::new(0.0, 0.0, size.width(), size.height()));

        assert_eq!(size, PopoverSurfaceSize::new(288.0, 108.0));
        assert!(layout.rows().is_empty());
        assert_eq!(
            layout.status_bounds(),
            Some(DipRect::new(16.0, 60.0, 256.0, 40.0))
        );
    }
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
