use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_ROUNDED_RECT, ID2D1DeviceContext, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

use crate::native_showcase_primitives::{draw_text, draw_text_clipped, fill_round, rect};
use crate::{
    DipRect, SettingsControl, SettingsControlKind, SettingsFocus, SettingsLayout, SettingsScene,
    layout_settings_scene,
};

const CONTROL_VALUE_WIDTH: f32 = 164.0;

pub(crate) struct SettingsBrushes<'a> {
    pub raised: &'a ID2D1SolidColorBrush,
    pub hover: &'a ID2D1SolidColorBrush,
    pub selected: &'a ID2D1SolidColorBrush,
    pub pressed: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub disabled: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
    pub focus: &'a ID2D1SolidColorBrush,
    pub divider: &'a ID2D1SolidColorBrush,
    pub on_accent: &'a ID2D1SolidColorBrush,
}

#[derive(Clone, Copy)]
pub(crate) struct SettingsFormats<'a> {
    pub title: &'a IDWriteTextFormat,
    pub label: &'a IDWriteTextFormat,
    pub detail: &'a IDWriteTextFormat,
    pub icon: &'a IDWriteTextFormat,
}

pub(crate) fn draw_functional_settings(
    context: &ID2D1DeviceContext,
    formats: SettingsFormats<'_>,
    surface: DipRect,
    scene: &SettingsScene,
    brushes: SettingsBrushes<'_>,
) {
    let layout = layout_settings_scene(scene, surface);
    draw_header(context, formats, scene, &layout, &brushes);
    draw_navigation(context, formats, scene, &layout, &brushes);
    draw_content(context, formats, scene, &layout, &brushes);
    draw_footer(context, formats, scene, &layout, &brushes);
}

fn draw_header(
    context: &ID2D1DeviceContext,
    formats: SettingsFormats<'_>,
    scene: &SettingsScene,
    layout: &SettingsLayout,
    brushes: &SettingsBrushes<'_>,
) {
    if let Some(bounds) = layout.back_bounds() {
        draw_focus_background(
            context,
            bounds,
            scene.focus() == Some(SettingsFocus::Back),
            brushes,
        );
        draw_text(
            context,
            "\u{E72B}",
            formats.icon,
            text_rect(bounds),
            brushes.primary,
        );
    }
    draw_text_clipped(
        context,
        scene.title(),
        formats.title,
        text_rect(layout.title_bounds()),
        brushes.primary,
    );
}

fn draw_navigation(
    context: &ID2D1DeviceContext,
    formats: SettingsFormats<'_>,
    scene: &SettingsScene,
    layout: &SettingsLayout,
    brushes: &SettingsBrushes<'_>,
) {
    let Some(bounds) = layout.navigation_bounds() else {
        return;
    };
    fill_round(context, rect_from_bounds(bounds, 12.0), brushes.raised);
    for item in layout.navigation() {
        let Some(model) = scene.navigation_item(item.section()) else {
            continue;
        };
        let bounds = item.bounds();
        if item.focused() {
            draw_two_part_focus(context, bounds, brushes);
        }
        if item.active() {
            fill_round(context, rect_from_bounds(bounds, 8.0), brushes.selected);
            fill_round(
                context,
                rect(
                    bounds.x + 3.0,
                    bounds.y + 9.0,
                    bounds.x + 6.0,
                    bounds.y + bounds.height - 9.0,
                    1.5,
                ),
                brushes.accent,
            );
        } else if item.focused() {
            fill_round(context, rect_from_bounds(bounds, 8.0), brushes.hover);
        }
        let text_brush = if item.enabled() {
            brushes.primary
        } else {
            brushes.disabled
        };
        let icon_left = bounds.x + 12.0;
        if let Some(glyph) = model.icon_glyph() {
            draw_text(
                context,
                glyph,
                formats.icon,
                D2D_RECT_F {
                    left: icon_left,
                    top: bounds.y,
                    right: icon_left + 24.0,
                    bottom: bounds.y + bounds.height,
                },
                text_brush,
            );
        }
        let label_left = if model.icon_glyph().is_some() {
            bounds.x + 44.0
        } else {
            bounds.x + 14.0
        };
        draw_text_clipped(
            context,
            model.label(),
            formats.label,
            D2D_RECT_F {
                left: label_left,
                top: bounds.y,
                right: bounds.x + bounds.width - 26.0,
                bottom: bounds.y + bounds.height,
            },
            text_brush,
        );
        if item.modified() {
            fill_round(
                context,
                rect(
                    bounds.x + bounds.width - 17.0,
                    bounds.y + bounds.height / 2.0 - 3.0,
                    bounds.x + bounds.width - 11.0,
                    bounds.y + bounds.height / 2.0 + 3.0,
                    3.0,
                ),
                brushes.accent,
            );
        }
    }
}

fn draw_content(
    context: &ID2D1DeviceContext,
    formats: SettingsFormats<'_>,
    scene: &SettingsScene,
    layout: &SettingsLayout,
    brushes: &SettingsBrushes<'_>,
) {
    let Some(content) = layout.content_bounds() else {
        return;
    };
    fill_round(context, rect_from_bounds(content, 12.0), brushes.raised);
    let Some(active) = scene.active_navigation_item() else {
        draw_text(
            context,
            "Choose a category to personalize Minha UI.",
            formats.label,
            inset_text_rect(content, 24.0),
            brushes.secondary,
        );
        return;
    };
    if let Some(heading) = layout.content_heading_bounds() {
        draw_text_clipped(
            context,
            active.label(),
            formats.title,
            D2D_RECT_F {
                left: heading.x + 20.0,
                top: heading.y + 4.0,
                right: heading.x + heading.width - 20.0,
                bottom: heading.y + 30.0,
            },
            brushes.primary,
        );
        draw_text_clipped(
            context,
            active.detail(),
            formats.detail,
            D2D_RECT_F {
                left: heading.x + 20.0,
                top: heading.y + 28.0,
                right: heading.x + heading.width - 20.0,
                bottom: heading.y + heading.height,
            },
            brushes.secondary,
        );
        fill_round(
            context,
            rect(
                heading.x + 16.0,
                heading.y + heading.height - 1.0,
                heading.x + heading.width - 16.0,
                heading.y + heading.height,
                0.5,
            ),
            brushes.divider,
        );
    }
    if layout.controls().is_empty() {
        draw_text(
            context,
            "No settings are available in this category yet.",
            formats.label,
            D2D_RECT_F {
                left: content.x + 20.0,
                top: content.y + 72.0,
                right: content.x + content.width - 20.0,
                bottom: content.y + content.height - 20.0,
            },
            brushes.secondary,
        );
        return;
    }
    for laid_out in layout.controls() {
        let Some(control) = scene.control(laid_out.id()) else {
            continue;
        };
        draw_control(context, formats, control, *laid_out, brushes);
    }
}

fn draw_control(
    context: &ID2D1DeviceContext,
    formats: SettingsFormats<'_>,
    control: &SettingsControl,
    laid_out: crate::SettingsLaidOutControl,
    brushes: &SettingsBrushes<'_>,
) {
    let bounds = laid_out.bounds();
    let row = DipRect::new(
        bounds.x + 12.0,
        bounds.y + 4.0,
        (bounds.width - 24.0).max(0.0),
        (bounds.height - 8.0).max(0.0),
    );
    if laid_out.focused() {
        draw_two_part_focus(context, row, brushes);
        fill_round(context, rect_from_bounds(row, 9.0), brushes.hover);
    }
    let text_brush = if laid_out.enabled() {
        brushes.primary
    } else {
        brushes.disabled
    };
    let value_width = CONTROL_VALUE_WIDTH.min((row.width * 0.42).max(112.0));
    let value_left = row.x + row.width - value_width;
    draw_text_clipped(
        context,
        control.label(),
        formats.label,
        D2D_RECT_F {
            left: row.x + 12.0,
            top: row.y + 5.0,
            right: value_left - 12.0,
            bottom: row.y + 28.0,
        },
        text_brush,
    );
    draw_text_clipped(
        context,
        control.detail(),
        formats.detail,
        D2D_RECT_F {
            left: row.x + 12.0,
            top: row.y + 27.0,
            right: value_left - 12.0,
            bottom: row.y + row.height - 5.0,
        },
        if laid_out.enabled() {
            brushes.secondary
        } else {
            brushes.disabled
        },
    );
    draw_control_value(
        context,
        formats,
        control,
        DipRect::new(value_left, row.y, value_width, row.height),
        brushes,
    );
    if laid_out.modified() {
        fill_round(
            context,
            rect(
                row.x + 2.0,
                row.y + row.height / 2.0 - 3.0,
                row.x + 8.0,
                row.y + row.height / 2.0 + 3.0,
                3.0,
            ),
            brushes.accent,
        );
    }
}

fn draw_control_value(
    context: &ID2D1DeviceContext,
    formats: SettingsFormats<'_>,
    control: &SettingsControl,
    bounds: DipRect,
    brushes: &SettingsBrushes<'_>,
) {
    let enabled = control.enabled();
    match control.kind() {
        SettingsControlKind::Toggle { checked } => {
            let switch = DipRect::new(
                bounds.x + bounds.width - 52.0,
                bounds.y + (bounds.height - 26.0) / 2.0,
                44.0,
                26.0,
            );
            fill_round(
                context,
                rect_from_bounds(switch, 13.0),
                if checked && enabled {
                    brushes.accent
                } else {
                    brushes.pressed
                },
            );
            let thumb_x = if checked {
                switch.x + 22.0
            } else {
                switch.x + 2.0
            };
            fill_round(
                context,
                rect(
                    thumb_x,
                    switch.y + 2.0,
                    thumb_x + 22.0,
                    switch.y + 24.0,
                    11.0,
                ),
                if enabled {
                    brushes.on_accent
                } else {
                    brushes.disabled
                },
            );
        }
        SettingsControlKind::Slider { position } => {
            let track = DipRect::new(
                bounds.x + 8.0,
                bounds.y + bounds.height / 2.0 + 6.0,
                (bounds.width - 16.0).max(1.0),
                6.0,
            );
            fill_round(context, rect_from_bounds(track, 3.0), brushes.pressed);
            let filled = track.width * f32::from(position) / 100.0;
            if enabled && filled > 0.0 {
                fill_round(
                    context,
                    rect(
                        track.x,
                        track.y,
                        track.x + filled.max(track.height),
                        track.y + track.height,
                        3.0,
                    ),
                    brushes.accent,
                );
            }
            draw_text_clipped(
                context,
                control.value(),
                formats.detail,
                D2D_RECT_F {
                    left: bounds.x + 8.0,
                    top: bounds.y,
                    right: bounds.x + bounds.width - 8.0,
                    bottom: bounds.y + bounds.height / 2.0 + 4.0,
                },
                if enabled {
                    brushes.primary
                } else {
                    brushes.disabled
                },
            );
        }
        SettingsControlKind::Choice
        | SettingsControlKind::Text
        | SettingsControlKind::Path
        | SettingsControlKind::Shortcut
        | SettingsControlKind::Action
        | SettingsControlKind::ReadOnly
        | SettingsControlKind::Reorder => {
            let glyph = match control.kind() {
                SettingsControlKind::Choice | SettingsControlKind::Reorder => "\u{E76C}",
                SettingsControlKind::Path => "\u{E838}",
                SettingsControlKind::Action => "\u{E72A}",
                SettingsControlKind::ReadOnly => "",
                SettingsControlKind::Text | SettingsControlKind::Shortcut => "\u{E70F}",
                SettingsControlKind::Toggle { .. } | SettingsControlKind::Slider { .. } => "",
            };
            draw_text_clipped(
                context,
                control.value(),
                formats.detail,
                D2D_RECT_F {
                    left: bounds.x + 8.0,
                    top: bounds.y,
                    right: bounds.x + bounds.width - 30.0,
                    bottom: bounds.y + bounds.height,
                },
                if enabled {
                    brushes.primary
                } else {
                    brushes.disabled
                },
            );
            if !glyph.is_empty() {
                draw_text(
                    context,
                    glyph,
                    formats.icon,
                    D2D_RECT_F {
                        left: bounds.x + bounds.width - 30.0,
                        top: bounds.y,
                        right: bounds.x + bounds.width - 4.0,
                        bottom: bounds.y + bounds.height,
                    },
                    if enabled {
                        brushes.secondary
                    } else {
                        brushes.disabled
                    },
                );
            }
        }
    }
}

fn draw_footer(
    context: &ID2D1DeviceContext,
    formats: SettingsFormats<'_>,
    scene: &SettingsScene,
    layout: &SettingsLayout,
    brushes: &SettingsBrushes<'_>,
) {
    if let Some(status) = layout.status_bounds() {
        let status_text = if !scene.status_text().is_empty() {
            scene.status_text()
        } else if scene.dirty() {
            "Changes not applied"
        } else {
            ""
        };
        draw_text_clipped(
            context,
            status_text,
            formats.detail,
            text_rect(status),
            brushes.secondary,
        );
    }
    draw_action(
        context,
        formats,
        "Reset",
        layout.reset_bounds(),
        true,
        scene.focus() == Some(SettingsFocus::Reset),
        false,
        brushes,
    );
    draw_action(
        context,
        formats,
        "Cancel",
        layout.cancel_bounds(),
        scene.dirty(),
        scene.focus() == Some(SettingsFocus::Cancel),
        false,
        brushes,
    );
    draw_action(
        context,
        formats,
        "Apply",
        layout.apply_bounds(),
        scene.dirty(),
        scene.focus() == Some(SettingsFocus::Apply),
        true,
        brushes,
    );
}

#[allow(
    clippy::too_many_arguments,
    reason = "the native action helper keeps state and Direct2D resources explicit"
)]
fn draw_action(
    context: &ID2D1DeviceContext,
    formats: SettingsFormats<'_>,
    label: &str,
    bounds: Option<DipRect>,
    enabled: bool,
    focused: bool,
    primary: bool,
    brushes: &SettingsBrushes<'_>,
) {
    let Some(bounds) = bounds else {
        return;
    };
    if focused {
        draw_two_part_focus(context, bounds, brushes);
    }
    fill_round(
        context,
        rect_from_bounds(bounds, 7.0),
        if primary && enabled {
            brushes.accent
        } else if focused {
            brushes.hover
        } else {
            brushes.raised
        },
    );
    draw_text(
        context,
        label,
        formats.label,
        text_rect(bounds),
        if enabled {
            if primary {
                brushes.on_accent
            } else {
                brushes.primary
            }
        } else {
            brushes.disabled
        },
    );
}

fn draw_focus_background(
    context: &ID2D1DeviceContext,
    bounds: DipRect,
    focused: bool,
    brushes: &SettingsBrushes<'_>,
) {
    if focused {
        draw_two_part_focus(context, bounds, brushes);
        fill_round(context, rect_from_bounds(bounds, 8.0), brushes.hover);
    }
}

fn draw_two_part_focus(
    context: &ID2D1DeviceContext,
    bounds: DipRect,
    brushes: &SettingsBrushes<'_>,
) {
    fill_round(
        context,
        rect(
            bounds.x - 2.0,
            bounds.y - 2.0,
            bounds.x + bounds.width + 2.0,
            bounds.y + bounds.height + 2.0,
            10.0,
        ),
        brushes.focus,
    );
    fill_round(
        context,
        rect(
            bounds.x - 1.0,
            bounds.y - 1.0,
            bounds.x + bounds.width + 1.0,
            bounds.y + bounds.height + 1.0,
            9.0,
        ),
        brushes.divider,
    );
}

fn rect_from_bounds(bounds: DipRect, radius: f32) -> D2D1_ROUNDED_RECT {
    rect(
        bounds.x,
        bounds.y,
        bounds.x + bounds.width,
        bounds.y + bounds.height,
        radius,
    )
}

const fn text_rect(bounds: DipRect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: bounds.x,
        top: bounds.y,
        right: bounds.x + bounds.width,
        bottom: bounds.y + bounds.height,
    }
}

const fn inset_text_rect(bounds: DipRect, inset: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: bounds.x + inset,
        top: bounds.y + inset,
        right: bounds.x + bounds.width - inset,
        bottom: bounds.y + bounds.height - inset,
    }
}

#[cfg(test)]
mod tests {
    use super::CONTROL_VALUE_WIDTH;

    #[test]
    fn value_column_remains_bounded_for_readable_labels() {
        assert!((120.0..=180.0).contains(&CONTROL_VALUE_WIDTH));
    }
}
