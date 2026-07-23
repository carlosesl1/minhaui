use shell_core::Popover;

use crate::{DipRect, PopoverContentState, PopoverRow, PopoverScene, layout_popover_scene};

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
