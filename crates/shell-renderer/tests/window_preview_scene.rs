use crate::{
    DockIcon, PreviewCardVisual, PreviewUnavailableReason, WindowPreviewCapture, WindowPreviewScene,
};
use shell_core::{DockItemId, WindowId};

fn cards(count: u64) -> Vec<PreviewCardVisual> {
    (0..count)
        .map(|index| {
            PreviewCardVisual::new(
                WindowId::new(index + 1),
                &format!("Window {}", index + 1),
                WindowPreviewCapture::DwmThumbnail,
            )
            .with_focused(index == 0)
        })
        .collect()
}

#[test]
fn scene_pages_every_four_windows() {
    // Given: a preview scene with five application windows.
    let scene = WindowPreviewScene::new(DockItemId::new(7), "Notepad", cards(5))
        .with_icon(DockIcon::SystemFallback)
        .with_page(1);

    // When: the visible page is requested.
    let visible = scene.visible_cards();

    // Then: only the fifth window appears on the second page.
    assert_eq!(scene.page_count(), 2);
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].window(), WindowId::new(5));
}

#[test]
fn scene_clamps_an_invalid_page_after_windows_close() {
    // Given: a one-window scene still carries a stale later page.
    let scene = WindowPreviewScene::new(DockItemId::new(7), "Notepad", cards(1))
        .with_icon(DockIcon::SystemFallback)
        .with_page(3);

    // When: its corrected page is calculated.
    let page = scene.corrected_page();

    // Then: paging returns to the nearest valid page.
    assert_eq!(page, 0);
    assert_eq!(scene.visible_cards().len(), 1);
}

#[test]
fn scene_tracks_close_hover_separately_from_card_hover() {
    // Given: a preview card whose destructive control is under the pointer.
    let scene = WindowPreviewScene::new(DockItemId::new(7), "Notepad", cards(1))
        .with_hovered_card(Some(WindowId::new(1)))
        .with_hovered_close(Some(WindowId::new(1)));

    // Then: renderers can accent only the close button without losing card hover.
    assert_eq!(scene.hovered_card(), Some(WindowId::new(1)));
    assert_eq!(scene.hovered_close(), Some(WindowId::new(1)));
}

#[test]
fn failed_native_thumbnail_changes_only_its_card_to_unavailable() {
    let scene = WindowPreviewScene::new(DockItemId::new(7), "Notepad", cards(2)).with_card_capture(
        WindowId::new(1),
        WindowPreviewCapture::Restricted(PreviewUnavailableReason::SourceUnavailable),
    );

    assert_eq!(
        scene.cards()[0].capture(),
        WindowPreviewCapture::Restricted(PreviewUnavailableReason::SourceUnavailable)
    );
    assert_eq!(
        scene.cards()[1].capture(),
        WindowPreviewCapture::DwmThumbnail
    );
}
