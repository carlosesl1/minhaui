use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;
use windows::core::Result;

use crate::native_icons::NativeIconCache;
use crate::native_showcase_primitives::{D2DPoint2F, draw_text, fill_round, fill_triangle, rect};
use crate::{
    DipRect, PopoverContentState, PopoverLayout, PopoverLayoutStyle, PopoverScene,
    layout_popover_scene,
};

pub(crate) struct PopoverBrushes<'a> {
    pub hover: &'a ID2D1SolidColorBrush,
    pub raised: &'a ID2D1SolidColorBrush,
    pub divider: &'a ID2D1SolidColorBrush,
    pub focus: &'a ID2D1SolidColorBrush,
    pub base: &'a ID2D1SolidColorBrush,
    pub rim: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
    pub error: &'a ID2D1SolidColorBrush,
}

pub(crate) struct PopoverFormats<'a> {
    pub label: &'a IDWriteTextFormat,
    pub balanced_label: &'a IDWriteTextFormat,
    pub detail: &'a IDWriteTextFormat,
    pub icon: &'a IDWriteTextFormat,
    pub title: &'a IDWriteTextFormat,
}

pub(crate) fn draw_functional_popover(
    context: &ID2D1DeviceContext,
    icons: &mut NativeIconCache,
    formats: PopoverFormats<'_>,
    surface: DipRect,
    scene: &PopoverScene,
    brushes: PopoverBrushes<'_>,
) -> Result<()> {
    let width = surface.width;
    let height = surface.height;
    let layout = layout_popover_scene(scene, DipRect::new(0.0, 0.0, width, height));
    match scene.layout_style() {
        PopoverLayoutStyle::Compact => {
            draw_compact_popover(
                context, icons, formats, width, height, scene, &layout, &brushes,
            );
            Ok(())
        }
        PopoverLayoutStyle::SystemPanel => draw_system_panel(
            context, icons, formats, width, height, scene, &layout, &brushes,
        ),
        PopoverLayoutStyle::BalancedApps => draw_balanced_apps(
            context, icons, formats, width, height, scene, &layout, &brushes,
        ),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the native draw helper keeps scene, geometry, and Direct2D resources explicit"
)]
fn draw_compact_popover(
    context: &ID2D1DeviceContext,
    icons: &mut NativeIconCache,
    formats: PopoverFormats<'_>,
    width: f32,
    height: f32,
    scene: &PopoverScene,
    layout: &PopoverLayout,
    brushes: &PopoverBrushes<'_>,
) {
    if layout.rows().is_empty() {
        draw_text(
            context,
            scene.status_text(),
            formats.label,
            D2D_RECT_F {
                left: 16.0,
                top: 5.0,
                right: width - 16.0,
                bottom: height - 5.0,
            },
            status_brush(scene.state(), brushes),
        );
    }
    for item in layout.rows() {
        let row = &scene.rows()[item.index()];
        let bounds = item.bounds();
        if item.focused() {
            fill_round(
                context,
                rect(
                    bounds.x - 7.0,
                    bounds.y,
                    bounds.x + bounds.width + 7.0,
                    bounds.y + bounds.height,
                    6.0,
                ),
                brushes.hover,
            );
        }
        let label_left = if let Some(source) = row.icon_source() {
            let icon_bounds = D2D_RECT_F {
                left: bounds.x,
                top: bounds.y + 3.0,
                right: bounds.x + 18.0,
                bottom: bounds.y + 21.0,
            };
            if !icons.draw(source, icon_bounds) {
                draw_text(
                    context,
                    "\u{ECAA}",
                    formats.icon,
                    icon_bounds,
                    brushes.primary,
                );
            }
            bounds.x + 26.0
        } else {
            bounds.x
        };
        draw_text(
            context,
            row.label(),
            formats.label,
            D2D_RECT_F {
                left: label_left,
                top: bounds.y,
                right: bounds.x + bounds.width * 0.55,
                bottom: bounds.y + bounds.height,
            },
            if row.enabled() {
                brushes.primary
            } else {
                brushes.secondary
            },
        );
        draw_text(
            context,
            row.detail(),
            formats.detail,
            D2D_RECT_F {
                left: bounds.x + bounds.width * 0.55,
                top: bounds.y,
                right: bounds.x + bounds.width,
                bottom: bounds.y + bounds.height,
            },
            brushes.secondary,
        );
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the native draw helper keeps scene, geometry, and Direct2D resources explicit"
)]
fn draw_system_panel(
    context: &ID2D1DeviceContext,
    icons: &mut NativeIconCache,
    formats: PopoverFormats<'_>,
    width: f32,
    height: f32,
    scene: &PopoverScene,
    layout: &PopoverLayout,
    brushes: &PopoverBrushes<'_>,
) -> Result<()> {
    if let Some(notch) = layout.notch() {
        let bounds = notch.bounds();
        let left = bounds.x.max(0.0);
        let right = (bounds.x + bounds.width).min(width);
        let top = bounds.y.max(0.0);
        let bottom = (bounds.y + bounds.height).min(height);
        let inner_bottom = (bottom + 1.5).min(height);
        let tip_x = notch.tip_x().clamp(left, right);
        fill_triangle(
            context,
            [
                D2DPoint2F { x: tip_x, y: top },
                D2DPoint2F {
                    x: right,
                    y: bottom,
                },
                D2DPoint2F { x: left, y: bottom },
            ],
            brushes.rim,
        )?;
        fill_triangle(
            context,
            [
                D2DPoint2F {
                    x: tip_x,
                    y: (top + 1.0).min(bottom),
                },
                D2DPoint2F {
                    x: (right - 1.5).max(tip_x),
                    y: inner_bottom,
                },
                D2DPoint2F {
                    x: (left + 1.5).min(tip_x),
                    y: inner_bottom,
                },
            ],
            brushes.base,
        )?;
    }

    if let Some(bounds) = layout.title_bounds() {
        draw_text(
            context,
            scene.title(),
            formats.title,
            text_bounds(bounds, 0.0, 0.0),
            brushes.primary,
        );
    }

    for separator in layout.separators() {
        let bounds = separator.bounds();
        fill_round(
            context,
            rect(
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
                0.0,
            ),
            brushes.divider,
        );
    }

    if layout.rows().is_empty() {
        draw_text(
            context,
            scene.status_text(),
            formats.label,
            D2D_RECT_F {
                left: 16.0,
                top: 60.0,
                right: width - 16.0,
                bottom: height - 8.0,
            },
            status_brush(scene.state(), brushes),
        );
        return Ok(());
    }

    for item in layout.rows() {
        let row = &scene.rows()[item.index()];
        let bounds = item.bounds();
        if item.focused() {
            fill_round(
                context,
                rect(
                    bounds.x - 4.0,
                    bounds.y + 2.0,
                    bounds.x + bounds.width + 4.0,
                    bounds.y + bounds.height - 2.0,
                    8.0,
                ),
                brushes.focus,
            );
        }

        let icon_bounds = DipRect::new(bounds.x + 2.0, bounds.y + 6.0, 28.0, 28.0);
        let label_left = if let Some(glyph) = row.icon_glyph() {
            fill_round(
                context,
                rect(
                    icon_bounds.x,
                    icon_bounds.y,
                    icon_bounds.x + icon_bounds.width,
                    icon_bounds.y + icon_bounds.height,
                    8.0,
                ),
                brushes.raised,
            );
            draw_text(
                context,
                glyph,
                formats.icon,
                text_bounds(icon_bounds, 0.0, 0.0),
                if row.enabled() {
                    brushes.primary
                } else {
                    brushes.secondary
                },
            );
            icon_bounds.x + icon_bounds.width + 10.0
        } else if let Some(source) = row.icon_source() {
            let image_bounds = text_bounds(icon_bounds, 0.0, 0.0);
            if !icons.draw(source, image_bounds) {
                draw_text(
                    context,
                    "\u{ECAA}",
                    formats.icon,
                    image_bounds,
                    brushes.secondary,
                );
            }
            icon_bounds.x + icon_bounds.width + 10.0
        } else {
            bounds.x + 4.0
        };
        let detail_left = bounds.x + bounds.width * 0.62;
        let row_brush = if row.enabled() {
            brushes.primary
        } else {
            brushes.secondary
        };
        draw_text(
            context,
            row.label(),
            formats.label,
            D2D_RECT_F {
                left: label_left,
                top: bounds.y,
                right: detail_left - 8.0,
                bottom: bounds.y + bounds.height,
            },
            row_brush,
        );
        draw_text(
            context,
            row.detail(),
            formats.detail,
            D2D_RECT_F {
                left: detail_left,
                top: bounds.y,
                right: bounds.x + bounds.width - 4.0,
                bottom: bounds.y + bounds.height,
            },
            if row.enabled() {
                brushes.secondary
            } else {
                row_brush
            },
        );
    }
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "the native draw helper keeps scene, geometry, and Direct2D resources explicit"
)]
fn draw_balanced_apps(
    context: &ID2D1DeviceContext,
    icons: &mut NativeIconCache,
    formats: PopoverFormats<'_>,
    width: f32,
    height: f32,
    scene: &PopoverScene,
    layout: &PopoverLayout,
    brushes: &PopoverBrushes<'_>,
) -> Result<()> {
    if let Some(notch) = layout.notch() {
        let bounds = notch.bounds();
        let left = bounds.x.max(0.0);
        let right = (bounds.x + bounds.width).min(width);
        let top = bounds.y.max(0.0);
        let bottom = (bounds.y + bounds.height).min(height);
        let inner_bottom = (bottom + 1.5).min(height);
        let tip_x = notch.tip_x().clamp(left, right);
        fill_triangle(
            context,
            [
                D2DPoint2F { x: tip_x, y: top },
                D2DPoint2F {
                    x: right,
                    y: bottom,
                },
                D2DPoint2F { x: left, y: bottom },
            ],
            brushes.rim,
        )?;
        fill_triangle(
            context,
            [
                D2DPoint2F {
                    x: tip_x,
                    y: (top + 1.0).min(bottom),
                },
                D2DPoint2F {
                    x: (right - 1.5).max(tip_x),
                    y: inner_bottom,
                },
                D2DPoint2F {
                    x: (left + 1.5).min(tip_x),
                    y: inner_bottom,
                },
            ],
            brushes.base,
        )?;
    }

    if let Some(bounds) = layout.title_bounds() {
        let title_right = if scene.header_detail().is_some() {
            (bounds.x + bounds.width - 48.0).max(bounds.x)
        } else {
            bounds.x + bounds.width
        };
        draw_text(
            context,
            scene.title(),
            formats.title,
            D2D_RECT_F {
                left: bounds.x,
                top: bounds.y,
                right: title_right,
                bottom: bounds.y + bounds.height,
            },
            brushes.primary,
        );
        if let Some(detail) = scene.header_detail() {
            draw_text(
                context,
                detail,
                formats.detail,
                D2D_RECT_F {
                    left: title_right + 8.0,
                    top: bounds.y,
                    right: bounds.x + bounds.width,
                    bottom: bounds.y + bounds.height,
                },
                brushes.secondary,
            );
        }
    }

    if layout.rows().is_empty() {
        let status_bounds = layout.status_bounds().unwrap_or(DipRect::new(
            16.0,
            60.0,
            (width - 32.0).max(0.0),
            (height - 68.0).max(0.0),
        ));
        draw_text(
            context,
            scene.status_text(),
            formats.label,
            text_bounds(status_bounds, 0.0, 0.0),
            status_brush(scene.state(), brushes),
        );
        return Ok(());
    }

    for item in layout.rows() {
        let row = &scene.rows()[item.index()];
        let bounds = item.bounds();
        let row_brush = if row.enabled() {
            brushes.primary
        } else {
            brushes.secondary
        };
        if item.focused() || item.externally_active() {
            fill_round(
                context,
                rect(
                    bounds.x - 4.0,
                    bounds.y + 2.0,
                    bounds.x + bounds.width + 4.0,
                    bounds.y + bounds.height - 2.0,
                    9.0,
                ),
                brushes.focus,
            );
        }

        if let Some(icon_bounds) = layout.icon_bounds(item.index()) {
            if let Some(source) = row.icon_source() {
                let image_bounds = text_bounds(icon_bounds, 0.0, 0.0);
                if !icons.draw_contained(source, image_bounds) {
                    fill_round(
                        context,
                        rect(
                            icon_bounds.x,
                            icon_bounds.y,
                            icon_bounds.x + icon_bounds.width,
                            icon_bounds.y + icon_bounds.height,
                            8.0,
                        ),
                        brushes.raised,
                    );
                    draw_text(
                        context,
                        "\u{ECAA}",
                        formats.icon,
                        image_bounds,
                        brushes.secondary,
                    );
                }
            } else if let Some(glyph) = row.icon_glyph() {
                draw_text(
                    context,
                    glyph,
                    formats.icon,
                    text_bounds(icon_bounds, 0.0, 0.0),
                    row_brush,
                );
            }
        }

        let label_bounds = layout.label_bounds(item.index()).unwrap_or(DipRect::new(
            bounds.x + 40.0,
            bounds.y,
            (bounds.width - 40.0).max(0.0),
            bounds.height,
        ));
        draw_text(
            context,
            row.label(),
            formats.balanced_label,
            text_bounds(label_bounds, 0.0, 0.0),
            row_brush,
        );
    }
    Ok(())
}

fn text_bounds(bounds: DipRect, inset_x: f32, inset_y: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: bounds.x + inset_x,
        top: bounds.y + inset_y,
        right: bounds.x + bounds.width - inset_x,
        bottom: bounds.y + bounds.height - inset_y,
    }
}

fn status_brush<'a>(
    state: PopoverContentState,
    brushes: &'a PopoverBrushes<'a>,
) -> &'a ID2D1SolidColorBrush {
    match state {
        PopoverContentState::Loading | PopoverContentState::Ready => brushes.accent,
        PopoverContentState::Empty | PopoverContentState::Offline => brushes.secondary,
        PopoverContentState::Error => brushes.error,
    }
}
