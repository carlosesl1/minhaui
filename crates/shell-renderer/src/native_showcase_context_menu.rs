use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

use crate::native_showcase_primitives::{draw_text, fill_round, rect};
use crate::{ContextMenuScene, DipRect, layout_context_menu_scene};

pub(crate) struct ContextMenuBrushes<'a> {
    pub shadows: &'a [ID2D1SolidColorBrush],
    pub rim: &'a ID2D1SolidColorBrush,
    pub body: &'a ID2D1SolidColorBrush,
    pub insets: &'a [ID2D1SolidColorBrush],
    pub hover: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub disabled: &'a ID2D1SolidColorBrush,
    pub separator: &'a ID2D1SolidColorBrush,
}

#[derive(Clone, Copy)]
struct ShadowPass {
    spread: f32,
}

// Drawn outside-in so the overlapping translucent layers approximate the
// long, soft 0 8px 48px falloff without depending on a GPU-only blur effect.
const SHADOW_PASSES: [ShadowPass; 11] = [
    ShadowPass { spread: 18.0 },
    ShadowPass { spread: 16.0 },
    ShadowPass { spread: 14.0 },
    ShadowPass { spread: 12.0 },
    ShadowPass { spread: 10.0 },
    ShadowPass { spread: 8.0 },
    ShadowPass { spread: 6.0 },
    ShadowPass { spread: 4.5 },
    ShadowPass { spread: 3.0 },
    ShadowPass { spread: 2.0 },
    ShadowPass { spread: 1.0 },
];

pub(crate) const SHADOW_ALPHA_PROFILE: [u8; 11] = [2, 3, 3, 4, 5, 6, 7, 9, 12, 16, 21];
pub(crate) const INSET_ALPHA_PROFILE: [u8; 5] = [17, 11, 7, 5, 3];
pub(crate) const MENU_BODY_ALPHA: u8 = 102;

pub(crate) fn draw_context_menu(
    context: &ID2D1DeviceContext,
    text_format: &IDWriteTextFormat,
    surface: DipRect,
    scene: &ContextMenuScene,
    brushes: ContextMenuBrushes<'_>,
) {
    let layout = layout_context_menu_scene(scene, surface);
    let card = layout.card_bounds();

    debug_assert_eq!(brushes.shadows.len(), SHADOW_PASSES.len());
    for (pass, brush) in SHADOW_PASSES.into_iter().zip(brushes.shadows) {
        fill_round(
            context,
            rect(
                card.x - pass.spread,
                card.y + 8.0 - pass.spread,
                card.x + card.width + pass.spread,
                card.y + card.height + 8.0 + pass.spread,
                12.0 + pass.spread,
            ),
            brush,
        );
    }
    fill_round(
        context,
        rect(
            card.x - 0.5,
            card.y - 0.5,
            card.x + card.width + 0.5,
            card.y + card.height + 0.5,
            12.5,
        ),
        brushes.rim,
    );
    fill_round(
        context,
        rect(
            card.x,
            card.y,
            card.x + card.width,
            card.y + card.height,
            12.0,
        ),
        brushes.body,
    );
    debug_assert_eq!(brushes.insets.len(), INSET_ALPHA_PROFILE.len());
    for (index, brush) in brushes.insets.iter().enumerate() {
        let depth = index as f32 * 0.8;
        let horizontal_inset = 2.0 + depth * 0.8;
        let top = card.y + 1.0 + depth;
        let bottom = card.y + card.height - 1.0 - depth;
        fill_round(
            context,
            rect(
                card.x + horizontal_inset,
                top,
                card.x + card.width - horizontal_inset,
                top + 0.7,
                0.35,
            ),
            brush,
        );
        fill_round(
            context,
            rect(
                card.x + horizontal_inset,
                bottom - 0.7,
                card.x + card.width - horizontal_inset,
                bottom,
                0.35,
            ),
            brush,
        );
    }

    for separator in layout.separators() {
        let bounds = separator.bounds();
        let y = bounds.y + bounds.height / 2.0;
        fill_round(
            context,
            rect(bounds.x, y, bounds.x + bounds.width, y + 0.75, 0.375),
            brushes.separator,
        );
    }

    for row in layout.rows() {
        let entry = &scene.entries()[row.entry_index()];
        let bounds = row.bounds();
        if row.focused() {
            fill_round(
                context,
                rect(
                    bounds.x - 6.0,
                    bounds.y + 1.0,
                    bounds.x + bounds.width + 6.0,
                    bounds.y + bounds.height - 1.0,
                    6.0,
                ),
                brushes.hover,
            );
        }
        if let Some(label) = entry.label() {
            draw_text(
                context,
                label,
                text_format,
                D2D_RECT_F {
                    left: bounds.x,
                    top: bounds.y,
                    right: bounds.x + bounds.width,
                    bottom: bounds.y + bounds.height,
                },
                if entry.enabled() {
                    brushes.primary
                } else {
                    brushes.disabled
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{INSET_ALPHA_PROFILE, MENU_BODY_ALPHA, SHADOW_ALPHA_PROFILE, SHADOW_PASSES};

    #[test]
    fn context_menu_shadow_uses_a_monotonic_soft_falloff() {
        assert!(SHADOW_PASSES.len() >= 10);
        assert!(
            SHADOW_PASSES
                .windows(2)
                .all(|pair| pair[0].spread > pair[1].spread)
        );
        assert!(
            SHADOW_ALPHA_PROFILE
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        );
        assert!(SHADOW_PASSES[0].spread >= 18.0);
        assert!(SHADOW_ALPHA_PROFILE[0] <= 3);
    }

    #[test]
    fn context_menu_inset_fades_instead_of_forming_a_solid_band() {
        assert!(INSET_ALPHA_PROFILE.len() >= 5);
        assert!(INSET_ALPHA_PROFILE.windows(2).all(|pair| pair[0] > pair[1]));
        assert!(INSET_ALPHA_PROFILE[0] <= 20);
        assert!(INSET_ALPHA_PROFILE[INSET_ALPHA_PROFILE.len() - 1] <= 4);
    }

    #[test]
    fn context_menu_neutral_coats_leave_background_chroma_visible() {
        let body = f32::from(MENU_BODY_ALPHA) / 255.0;
        let background_transmission = 1.0 - body;

        assert!((0.55..=0.65).contains(&background_transmission));
    }
}
