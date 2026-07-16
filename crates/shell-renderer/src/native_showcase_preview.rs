use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};

use crate::native_showcase_primitives::{draw_text, draw_text_clipped, fill_round, rect};
use crate::native_showcase_resources::ShowcaseFormats;
use crate::{
    DipRect, PreviewUnavailableReason, Rgba8, WINDOW_PREVIEW_CARD_RADIUS,
    WINDOW_PREVIEW_THUMBNAIL_RADIUS, WindowPreviewCapture, WindowPreviewScene,
    layout_window_preview,
};

pub(crate) const PREVIEW_PANEL_FILL: Rgba8 = Rgba8::new(28, 28, 30, 188);
pub(crate) const PREVIEW_PANEL_SOLID_FILL: Rgba8 = Rgba8::new(28, 28, 30, 255);
pub(crate) const PREVIEW_CARD_FILL: Rgba8 = Rgba8::new(28, 28, 30, 150);
pub(crate) const PREVIEW_HOVER_RIM: Rgba8 = Rgba8::new(248, 248, 248, 72);
pub(crate) const PREVIEW_CLOSE_FILL: Rgba8 = Rgba8::new(18, 18, 20, 190);
pub(crate) const PREVIEW_CLOSE_RIM: Rgba8 = Rgba8::new(255, 255, 255, 54);
pub(crate) const PREVIEW_CLOSE_HOVER_FILL: Rgba8 = Rgba8::new(255, 69, 58, 232);
pub(crate) const PREVIEW_CLOSE_HOVER_RIM: Rgba8 = Rgba8::new(255, 154, 148, 128);
pub(crate) const PREVIEW_CLOSE_HOVER_GLYPH: Rgba8 = Rgba8::new(255, 238, 236, 255);

pub(crate) struct PreviewBrushes<'a> {
    pub card: &'a ID2D1SolidColorBrush,
    pub hover_rim: &'a ID2D1SolidColorBrush,
    pub close: &'a ID2D1SolidColorBrush,
    pub close_rim: &'a ID2D1SolidColorBrush,
    pub close_hover: &'a ID2D1SolidColorBrush,
    pub close_hover_rim: &'a ID2D1SolidColorBrush,
    pub close_hover_glyph: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub warning: &'a ID2D1SolidColorBrush,
}

pub(crate) fn draw_window_preview(
    context: &ID2D1DeviceContext,
    formats: ShowcaseFormats<'_>,
    surface: DipRect,
    scene: &WindowPreviewScene,
    brushes: PreviewBrushes<'_>,
) {
    let layout = layout_window_preview(scene, surface);
    for card in layout.cards() {
        let visual = scene
            .visible_cards()
            .iter()
            .find(|visual| visual.window() == card.window());
        let Some(visual) = visual else {
            continue;
        };
        let selected = scene.hovered_card() == Some(card.window())
            || scene.focused_card() == Some(card.window());
        let card_bounds = card.card();
        if selected {
            fill_round(
                context,
                rect(
                    card_bounds.x - 2.0,
                    card_bounds.y - 2.0,
                    card_bounds.x + card_bounds.width + 2.0,
                    card_bounds.y + card_bounds.height + 2.0,
                    11.0,
                ),
                brushes.hover_rim,
            );
        }
        fill_round(
            context,
            rect(
                card_bounds.x,
                card_bounds.y,
                card_bounds.x + card_bounds.width,
                card_bounds.y + card_bounds.height,
                WINDOW_PREVIEW_CARD_RADIUS,
            ),
            brushes.card,
        );
        let thumbnail = card.thumbnail();
        fill_round(
            context,
            rect(
                thumbnail.x,
                thumbnail.y,
                thumbnail.x + thumbnail.width,
                thumbnail.y + thumbnail.height,
                WINDOW_PREVIEW_THUMBNAIL_RADIUS,
            ),
            brushes.card,
        );
        if let WindowPreviewCapture::Restricted(reason) = visual.capture() {
            draw_unavailable(context, formats, thumbnail, reason, &brushes);
        }
        let title = card.title();
        draw_text_clipped(
            context,
            visual.title(),
            formats.text,
            text_rect(title),
            if visual.minimized() {
                brushes.secondary
            } else {
                brushes.primary
            },
        );
        let close = card.close();
        let close_hovered = scene.hovered_close() == Some(card.window());
        fill_round(
            context,
            rect(
                close.x - 0.5,
                close.y - 0.5,
                close.x + close.width + 0.5,
                close.y + close.height + 0.5,
                14.5,
            ),
            if close_hovered {
                brushes.close_hover_rim
            } else {
                brushes.close_rim
            },
        );
        fill_round(
            context,
            rect(
                close.x,
                close.y,
                close.x + close.width,
                close.y + close.height,
                14.0,
            ),
            if close_hovered {
                brushes.close_hover
            } else {
                brushes.close
            },
        );
        draw_text(
            context,
            "\u{E711}",
            formats.icon,
            text_rect(close),
            if close_hovered {
                brushes.close_hover_glyph
            } else {
                brushes.primary
            },
        );
    }
    if let Some(bounds) = layout.previous() {
        draw_text(
            context,
            "\u{E76B}",
            formats.icon,
            text_rect(bounds),
            brushes.secondary,
        );
    }
    if let Some(bounds) = layout.next() {
        draw_text(
            context,
            "\u{E76C}",
            formats.icon,
            text_rect(bounds),
            brushes.secondary,
        );
    }
    if let Some(bounds) = layout.page_indicator() {
        draw_text(
            context,
            &format!("{} / {}", scene.corrected_page() + 1, scene.page_count()),
            formats.text,
            text_rect(bounds),
            brushes.secondary,
        );
    }
}

fn draw_unavailable(
    context: &ID2D1DeviceContext,
    formats: ShowcaseFormats<'_>,
    bounds: DipRect,
    reason: PreviewUnavailableReason,
    brushes: &PreviewBrushes<'_>,
) {
    let message = match reason {
        PreviewUnavailableReason::CaptureRestricted => "Conteúdo protegido",
        PreviewUnavailableReason::SourceUnavailable => "Preview indisponível",
    };
    draw_text(
        context,
        "\u{E7BA}",
        formats.icon,
        D2D_RECT_F {
            left: bounds.x,
            top: bounds.y + bounds.height / 2.0 - 30.0,
            right: bounds.x + bounds.width,
            bottom: bounds.y + bounds.height / 2.0,
        },
        brushes.warning,
    );
    draw_text(
        context,
        message,
        formats.text,
        D2D_RECT_F {
            left: bounds.x + 12.0,
            top: bounds.y + bounds.height / 2.0,
            right: bounds.x + bounds.width - 12.0,
            bottom: bounds.y + bounds.height / 2.0 + 28.0,
        },
        brushes.secondary,
    );
}

const fn text_rect(bounds: DipRect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: bounds.x,
        top: bounds.y,
        right: bounds.x + bounds.width,
        bottom: bounds.y + bounds.height,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PREVIEW_CARD_FILL, PREVIEW_CLOSE_FILL, PREVIEW_CLOSE_HOVER_FILL, PREVIEW_CLOSE_HOVER_GLYPH,
        PREVIEW_HOVER_RIM, PREVIEW_PANEL_FILL, PREVIEW_PANEL_SOLID_FILL,
    };

    fn assert_minimum_alpha(color: crate::Rgba8, minimum: u8) {
        assert!(color.a >= minimum);
    }

    fn assert_maximum_alpha(color: crate::Rgba8, maximum: u8) {
        assert!(color.a <= maximum);
    }

    fn assert_alpha(color: crate::Rgba8, expected: u8) {
        assert_eq!(color.a, expected);
    }

    fn assert_red_dominant(color: crate::Rgba8) {
        assert!(color.r > color.g.saturating_mul(2));
        assert!(color.r > color.b.saturating_mul(2));
    }

    fn assert_warm_white(color: crate::Rgba8) {
        assert!(color.r > color.g);
    }

    #[test]
    fn preview_hover_treatment_is_neutral_and_translucent() {
        for color in [
            PREVIEW_PANEL_FILL,
            PREVIEW_PANEL_SOLID_FILL,
            PREVIEW_CARD_FILL,
            PREVIEW_HOVER_RIM,
            PREVIEW_CLOSE_FILL,
        ] {
            let minimum = color.r.min(color.g).min(color.b);
            let maximum = color.r.max(color.g).max(color.b);
            assert!(maximum - minimum <= 8);
        }
        assert_maximum_alpha(PREVIEW_PANEL_FILL, 219);
        assert_alpha(PREVIEW_PANEL_SOLID_FILL, 255);
        assert_maximum_alpha(PREVIEW_CARD_FILL, 219);
        assert_maximum_alpha(PREVIEW_HOVER_RIM, 219);
        assert_maximum_alpha(PREVIEW_CLOSE_FILL, 219);
        assert_minimum_alpha(PREVIEW_HOVER_RIM, 48);
        assert_minimum_alpha(PREVIEW_CLOSE_FILL, 120);
    }

    #[test]
    fn close_hover_uses_a_destructive_red_accent() {
        assert_red_dominant(PREVIEW_CLOSE_HOVER_FILL);
        assert_warm_white(PREVIEW_CLOSE_HOVER_GLYPH);
    }
}
