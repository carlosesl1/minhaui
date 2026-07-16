#![deny(unsafe_code)]

use shell_renderer::{DipRect, PhysicalRect, WindowPreviewCapture, physical_from_dip};

use crate::PreviewCapture;
use crate::win32_preview::PREVIEW_THUMBNAIL_OPACITY;
use crate::win32_window::OwnedWindow;

pub(super) const fn renderer_capture(capture: PreviewCapture) -> WindowPreviewCapture {
    match capture {
        PreviewCapture::DwmThumbnail => WindowPreviewCapture::DwmThumbnail,
        PreviewCapture::Restricted(reason) => WindowPreviewCapture::Restricted(reason),
    }
}

pub(super) fn physical_anchor(window: &OwnedWindow, bounds: DipRect) -> PhysicalRect {
    PhysicalRect::new(
        window.rect.x + physical_from_dip(bounds.x, window.dpi()),
        window.rect.y + physical_from_dip(bounds.y, window.dpi()),
        physical_from_dip(bounds.width, window.dpi()),
        physical_from_dip(bounds.height, window.dpi()),
    )
}

pub(super) fn physical_card_rect(bounds: DipRect, dpi: shell_renderer::Dpi) -> PhysicalRect {
    PhysicalRect::new(
        physical_from_dip(bounds.x, dpi),
        physical_from_dip(bounds.y, dpi),
        physical_from_dip(bounds.width.max(1.0), dpi),
        physical_from_dip(bounds.height.max(1.0), dpi),
    )
}

pub(super) fn thumbnail_opacity(progress: f32) -> u8 {
    (progress.clamp(0.0, 1.0) * f32::from(PREVIEW_THUMBNAIL_OPACITY)).round() as u8
}

#[cfg(test)]
mod tests {
    use shell_renderer::{DipRect, Dpi, PhysicalRect};

    use super::physical_card_rect;

    #[test]
    fn native_thumbnail_host_keeps_the_full_layout_size() {
        let bounds = DipRect::new(12.0, 48.0, 248.0, 140.0);

        for (dpi, expected) in [
            (96, PhysicalRect::new(12, 48, 248, 140)),
            (120, PhysicalRect::new(15, 60, 310, 175)),
            (144, PhysicalRect::new(18, 72, 372, 210)),
            (192, PhysicalRect::new(24, 96, 496, 280)),
        ] {
            assert_eq!(physical_card_rect(bounds, Dpi::from_raw(dpi)), expected);
        }
    }
}
