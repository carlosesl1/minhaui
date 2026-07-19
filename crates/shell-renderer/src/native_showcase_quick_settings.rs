use shell_core::QuickControlKind;
use windows::Win32::Graphics::Direct2D::{ID2D1DeviceContext, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectWrite::IDWriteTextFormat;

use crate::native_icons::NativeIconCache;
use crate::native_showcase_primitives::{D2DPoint2F, draw_text, fill_round, fill_triangle, rect};
use crate::{
    DipRect, QuickSettingsChoice, QuickSettingsFocus, QuickSettingsHit, QuickSettingsMediaAction,
    QuickSettingsMediaChoice, QuickSettingsPlaybackState, QuickSettingsScene, QuickSettingsTile,
    layout_quick_settings,
};

const TILE_INSET: f32 = 12.0;
const TILE_ICON_SIZE: f32 = 32.0;
const TILE_ICON_TEXT_GAP: f32 = 8.0;

pub(crate) struct QuickSettingsBrushes<'a> {
    pub section: &'a ID2D1SolidColorBrush,
    pub tile: &'a ID2D1SolidColorBrush,
    pub hover: &'a ID2D1SolidColorBrush,
    pub pressed: &'a ID2D1SolidColorBrush,
    pub selected: &'a ID2D1SolidColorBrush,
    pub primary: &'a ID2D1SolidColorBrush,
    pub secondary: &'a ID2D1SolidColorBrush,
    pub accent: &'a ID2D1SolidColorBrush,
    pub on_accent: &'a ID2D1SolidColorBrush,
    pub focus: &'a ID2D1SolidColorBrush,
    pub error: &'a ID2D1SolidColorBrush,
    pub base: &'a ID2D1SolidColorBrush,
    pub rim: &'a ID2D1SolidColorBrush,
}

pub(crate) struct QuickSettingsFormats<'a> {
    pub label: &'a IDWriteTextFormat,
    pub detail: &'a IDWriteTextFormat,
    pub footer: &'a IDWriteTextFormat,
    pub icon: &'a IDWriteTextFormat,
}

pub(crate) fn draw_quick_settings(
    context: &ID2D1DeviceContext,
    icons: &mut NativeIconCache,
    formats: QuickSettingsFormats<'_>,
    surface: DipRect,
    scene: &QuickSettingsScene,
    brushes: QuickSettingsBrushes<'_>,
) {
    if let Some(anchor_x) = scene.anchor_x() {
        let tip = anchor_x.clamp(12.0, surface.width - 12.0);
        let _ = fill_triangle(
            context,
            [
                D2DPoint2F { x: tip, y: 0.0 },
                D2DPoint2F {
                    x: tip + 10.0,
                    y: 9.0,
                },
                D2DPoint2F {
                    x: tip - 10.0,
                    y: 9.0,
                },
            ],
            brushes.rim,
        );
        let _ = fill_triangle(
            context,
            [
                D2DPoint2F { x: tip, y: 1.5 },
                D2DPoint2F {
                    x: tip + 8.0,
                    y: 9.0,
                },
                D2DPoint2F {
                    x: tip - 8.0,
                    y: 9.0,
                },
            ],
            brushes.base,
        );
    }
    let layout = layout_quick_settings(scene, surface);
    if scene.media_choices().is_some() {
        draw_media_choices(context, icons, &formats, scene, &layout, &brushes);
        retain_media_artwork(icons, scene);
        return;
    }
    if let Some(submenu) = scene.submenu() {
        draw_submenu(context, &formats, scene, submenu, &layout, &brushes);
        return;
    }
    if let (Some(player), Some(media)) = (scene.media(), layout.media()) {
        draw_media_player(context, icons, &formats, scene, player, media, &brushes);
    }
    for (kind, bounds) in layout.sections() {
        fill_round(
            context,
            rect(
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
                12.0,
            ),
            brushes.section,
        );
        draw_text(
            context,
            section_title(*kind),
            formats.label,
            text_rect(
                bounds.x + 12.0,
                bounds.y + 8.0,
                bounds.x + bounds.width - 12.0,
                bounds.y + 36.0,
            ),
            brushes.primary,
        );
    }

    for laid_out in layout.tiles() {
        if let Some(tile) = find_tile(scene, laid_out.kind()) {
            draw_tile(
                context,
                formats.label,
                formats.detail,
                formats.icon,
                tile,
                laid_out.bounds(),
                scene,
                &brushes,
            );
        }
    }

    for slider in layout.sliders() {
        let Some(value) = slider_value(scene, slider.kind()) else {
            continue;
        };
        let bounds = slider.bounds();
        let track = slider.track();
        if scene.focused() == Some(QuickSettingsFocus::Control(slider.kind())) {
            fill_round(
                context,
                rect(
                    bounds.x - 2.0,
                    bounds.y - 1.0,
                    bounds.x + bounds.width + 2.0,
                    bounds.y + bounds.height + 1.0,
                    9.0,
                ),
                brushes.focus,
            );
        }
        draw_text(
            context,
            slider_label(slider.kind()),
            formats.icon,
            text_rect(
                bounds.x,
                bounds.y,
                bounds.x + 28.0,
                bounds.y + bounds.height,
            ),
            brushes.primary,
        );
        fill_round(
            context,
            rect(
                track.x,
                track.y,
                track.x + track.width,
                track.y + track.height,
                4.0,
            ),
            brushes.pressed,
        );
        let filled = track.width * f32::from(value) / 100.0;
        fill_round(
            context,
            rect(
                track.x,
                track.y,
                track.x + filled.max(4.0),
                track.y + track.height,
                4.0,
            ),
            brushes.accent,
        );
        fill_round(
            context,
            rect(
                track.x + filled - 6.0,
                track.y - 4.0,
                track.x + filled + 6.0,
                track.y + track.height + 4.0,
                6.0,
            ),
            brushes.primary,
        );
    }

    if let Some(sound) = scene.sound() {
        if let Some((_, section)) = layout
            .sections()
            .iter()
            .find(|(kind, _)| *kind == QuickControlKind::Volume)
        {
            draw_text(
                context,
                sound.output(),
                formats.detail,
                text_rect(
                    section.x + 12.0,
                    section.y + section.height - 32.0,
                    section.x + section.width - 12.0,
                    section.y + section.height - 12.0,
                ),
                brushes.secondary,
            );
        }
    }
    if let Some(energy) = scene.energy() {
        if let Some((_, section)) = layout
            .sections()
            .iter()
            .find(|(kind, _)| *kind == QuickControlKind::Battery)
        {
            draw_text(
                context,
                &format!("{}%", energy.percent()),
                formats.label,
                text_rect(
                    section.x + 12.0,
                    section.y + 44.0,
                    section.x + 74.0,
                    section.y + 84.0,
                ),
                brushes.primary,
            );
            draw_text(
                context,
                if energy.charging() {
                    "Charging"
                } else {
                    "Battery"
                },
                formats.detail,
                text_rect(
                    section.x + 76.0,
                    section.y + 44.0,
                    section.x + section.width - 12.0,
                    section.y + 84.0,
                ),
                brushes.secondary,
            );
        }
    }

    let Some(edit) = layout.edit_bounds() else {
        return;
    };
    let edit_brush = if scene.hovered() == Some(QuickSettingsHit::EditControls) {
        brushes.hover
    } else {
        brushes.section
    };
    if scene.focused() == Some(QuickSettingsFocus::EditControls) {
        fill_round(
            context,
            rect(
                edit.x - 1.5,
                edit.y - 1.5,
                edit.x + edit.width + 1.5,
                edit.y + edit.height + 1.5,
                12.0,
            ),
            brushes.focus,
        );
    }
    fill_round(
        context,
        rect(
            edit.x,
            edit.y,
            edit.x + edit.width,
            edit.y + edit.height,
            10.0,
        ),
        edit_brush,
    );
    draw_text(
        context,
        "Edit controls...",
        formats.footer,
        text_rect(
            edit.x + 12.0,
            edit.y,
            edit.x + edit.width - 12.0,
            edit.y + edit.height,
        ),
        brushes.secondary,
    );
    if !scene.status_text().is_empty() {
        draw_text(
            context,
            scene.status_text(),
            formats.detail,
            text_rect(
                16.0,
                surface.height - 30.0,
                surface.width - 16.0,
                surface.height - 8.0,
            ),
            brushes.error,
        );
    }
    retain_media_artwork(icons, scene);
}

fn draw_media_player(
    context: &ID2D1DeviceContext,
    icons: &mut NativeIconCache,
    formats: &QuickSettingsFormats<'_>,
    scene: &QuickSettingsScene,
    player: &crate::QuickSettingsMediaPlayer,
    layout: crate::QuickSettingsLaidOutMedia,
    brushes: &QuickSettingsBrushes<'_>,
) {
    let bounds = layout.bounds();
    fill_round(
        context,
        rect(
            bounds.x,
            bounds.y,
            bounds.x + bounds.width,
            bounds.y + bounds.height,
            12.0,
        ),
        brushes.tile,
    );
    let artwork = layout.artwork();
    fill_round(
        context,
        rect(
            artwork.x,
            artwork.y,
            artwork.x + artwork.width,
            artwork.y + artwork.height,
            10.0,
        ),
        if scene.hovered() == Some(QuickSettingsHit::MediaArtwork) {
            brushes.hover
        } else {
            brushes.pressed
        },
    );
    let drew_artwork = player.artwork().is_some_and(|value| {
        icons.draw_encoded(
            value.generation(),
            value.encoded(),
            text_rect(
                artwork.x,
                artwork.y,
                artwork.x + artwork.width,
                artwork.y + artwork.height,
            ),
        )
    });
    if !drew_artwork {
        draw_text(
            context,
            "\u{E8D6}",
            formats.icon,
            text_rect(
                artwork.x,
                artwork.y,
                artwork.x + artwork.width,
                artwork.y + artwork.height,
            ),
            brushes.secondary,
        );
    }
    let text_x = artwork.x + artwork.width + 8.0;
    draw_text(
        context,
        player.title(),
        formats.label,
        text_rect(
            text_x,
            bounds.y + 14.0,
            bounds.x + bounds.width - 10.0,
            bounds.y + 35.0,
        ),
        brushes.primary,
    );
    draw_text(
        context,
        if player.artist().is_empty() {
            player.source()
        } else {
            player.artist()
        },
        formats.detail,
        text_rect(
            text_x,
            bounds.y + 35.0,
            bounds.x + bounds.width - 10.0,
            bounds.y + 56.0,
        ),
        brushes.secondary,
    );
    for (action, action_bounds, enabled) in [
        (
            QuickSettingsMediaAction::Previous,
            layout.previous(),
            player.can_previous(),
        ),
        (
            QuickSettingsMediaAction::TogglePlayback,
            layout.toggle(),
            player.can_toggle(),
        ),
        (
            QuickSettingsMediaAction::Next,
            layout.next(),
            player.can_next(),
        ),
    ] {
        let hit = QuickSettingsHit::MediaAction(action);
        if scene.pressed() == Some(hit) || scene.hovered() == Some(hit) {
            fill_round(
                context,
                rect(
                    action_bounds.x,
                    action_bounds.y,
                    action_bounds.x + action_bounds.width,
                    action_bounds.y + action_bounds.height,
                    15.0,
                ),
                if scene.pressed() == Some(hit) {
                    brushes.pressed
                } else {
                    brushes.hover
                },
            );
        }
        draw_text(
            context,
            media_glyph(action, player.playback()),
            formats.icon,
            text_rect(
                action_bounds.x,
                action_bounds.y,
                action_bounds.x + action_bounds.width,
                action_bounds.y + action_bounds.height,
            ),
            if enabled && player.pending() != Some(action) {
                brushes.primary
            } else {
                brushes.secondary
            },
        );
    }
}

fn draw_media_choices(
    context: &ID2D1DeviceContext,
    icons: &mut NativeIconCache,
    formats: &QuickSettingsFormats<'_>,
    scene: &QuickSettingsScene,
    layout: &crate::QuickSettingsLayout,
    brushes: &QuickSettingsBrushes<'_>,
) {
    let Some(back) = layout.back_bounds() else {
        return;
    };
    fill_round(
        context,
        rect(
            back.x,
            back.y,
            back.x + back.width,
            back.y + back.height,
            18.0,
        ),
        if scene.hovered() == Some(QuickSettingsHit::Back) {
            brushes.hover
        } else {
            brushes.tile
        },
    );
    draw_text(
        context,
        "\u{E72B}",
        formats.icon,
        text_rect(back.x, back.y, back.x + back.width, back.y + back.height),
        brushes.primary,
    );
    draw_text(
        context,
        "Media",
        formats.label,
        text_rect(
            back.x + 48.0,
            back.y - 1.0,
            layout.content_bounds().x + layout.content_bounds().width,
            back.y + 20.0,
        ),
        brushes.primary,
    );
    draw_text(
        context,
        "Choose an app",
        formats.detail,
        text_rect(
            back.x + 48.0,
            back.y + 18.0,
            layout.content_bounds().x + layout.content_bounds().width,
            back.y + 39.0,
        ),
        brushes.secondary,
    );
    let Some(choices) = scene.media_choices() else {
        return;
    };
    for laid_out in layout.media_choices() {
        let Some(choice) = choices
            .iter()
            .find(|choice| choice.session_id() == laid_out.id())
        else {
            continue;
        };
        draw_media_choice(
            context,
            icons,
            formats,
            scene,
            choice,
            laid_out.bounds(),
            brushes,
        );
    }
}

fn draw_media_choice(
    context: &ID2D1DeviceContext,
    icons: &mut NativeIconCache,
    formats: &QuickSettingsFormats<'_>,
    scene: &QuickSettingsScene,
    choice: &QuickSettingsMediaChoice,
    bounds: DipRect,
    brushes: &QuickSettingsBrushes<'_>,
) {
    let hit = QuickSettingsHit::MediaSession(choice.session_id());
    fill_round(
        context,
        rect(
            bounds.x,
            bounds.y,
            bounds.x + bounds.width,
            bounds.y + bounds.height,
            12.0,
        ),
        if scene.pressed() == Some(hit) {
            brushes.pressed
        } else if scene.hovered() == Some(hit) {
            brushes.hover
        } else if choice.selected() {
            brushes.selected
        } else {
            brushes.tile
        },
    );
    let artwork = DipRect::new(bounds.x + 11.0, bounds.y + 9.0, 40.0, 40.0);
    let drew = choice.artwork().is_some_and(|value| {
        icons.draw_encoded(
            value.generation(),
            value.encoded(),
            text_rect(
                artwork.x,
                artwork.y,
                artwork.x + artwork.width,
                artwork.y + artwork.height,
            ),
        )
    });
    if !drew {
        fill_round(
            context,
            rect(
                artwork.x,
                artwork.y,
                artwork.x + artwork.width,
                artwork.y + artwork.height,
                9.0,
            ),
            brushes.pressed,
        );
        draw_text(
            context,
            "\u{E8D6}",
            formats.icon,
            text_rect(
                artwork.x,
                artwork.y,
                artwork.x + artwork.width,
                artwork.y + artwork.height,
            ),
            brushes.secondary,
        );
    }
    let text_x = artwork.x + artwork.width + 10.0;
    let text_right = bounds.x + bounds.width - 42.0;
    draw_text(
        context,
        choice.title(),
        formats.label,
        text_rect(text_x, bounds.y + 8.0, text_right, bounds.y + 29.0),
        brushes.primary,
    );
    draw_text(
        context,
        if choice.artist().is_empty() {
            choice.source()
        } else {
            choice.artist()
        },
        formats.detail,
        text_rect(text_x, bounds.y + 29.0, text_right, bounds.y + 50.0),
        brushes.secondary,
    );
    if choice.selected() {
        draw_text(
            context,
            "\u{E73E}",
            formats.icon,
            text_rect(
                bounds.x + bounds.width - 38.0,
                bounds.y + 11.0,
                bounds.x + bounds.width - 8.0,
                bounds.y + 47.0,
            ),
            brushes.accent,
        );
    }
}

fn retain_media_artwork(icons: &mut NativeIconCache, scene: &QuickSettingsScene) {
    let mut generations = Vec::new();
    if let Some(generation) = scene
        .media()
        .and_then(|player| player.artwork())
        .map(|artwork| artwork.generation())
    {
        generations.push(generation);
    }
    if let Some(choices) = scene.media_choices() {
        generations.extend(
            choices
                .iter()
                .filter_map(|choice| choice.artwork().map(|artwork| artwork.generation())),
        );
    }
    icons.retain_encoded(&generations);
}

const fn media_glyph(
    action: QuickSettingsMediaAction,
    playback: QuickSettingsPlaybackState,
) -> &'static str {
    match action {
        QuickSettingsMediaAction::Previous => "\u{E892}",
        QuickSettingsMediaAction::TogglePlayback
            if matches!(playback, QuickSettingsPlaybackState::Playing) =>
        {
            "\u{E769}"
        }
        QuickSettingsMediaAction::TogglePlayback => "\u{E768}",
        QuickSettingsMediaAction::Next => "\u{E893}",
    }
}

fn draw_submenu(
    context: &ID2D1DeviceContext,
    formats: &QuickSettingsFormats<'_>,
    scene: &QuickSettingsScene,
    submenu: &crate::QuickSettingsSubmenu,
    layout: &crate::QuickSettingsLayout,
    brushes: &QuickSettingsBrushes<'_>,
) {
    let Some(back) = layout.back_bounds() else {
        return;
    };
    let back_fill = if scene.pressed() == Some(QuickSettingsHit::Back) {
        brushes.pressed
    } else if scene.hovered() == Some(QuickSettingsHit::Back) {
        brushes.hover
    } else {
        brushes.tile
    };
    if scene.focused() == Some(QuickSettingsFocus::Back) {
        fill_round(
            context,
            rect(
                back.x - 1.5,
                back.y - 1.5,
                back.x + back.width + 1.5,
                back.y + back.height + 1.5,
                19.0,
            ),
            brushes.focus,
        );
    }
    fill_round(
        context,
        rect(
            back.x,
            back.y,
            back.x + back.width,
            back.y + back.height,
            18.0,
        ),
        back_fill,
    );
    draw_text(
        context,
        "\u{E72B}",
        formats.icon,
        text_rect(back.x, back.y, back.x + back.width, back.y + back.height),
        brushes.primary,
    );
    draw_text(
        context,
        submenu.title(),
        formats.label,
        text_rect(
            back.x + 48.0,
            back.y - 1.0,
            layout.content_bounds().x + layout.content_bounds().width,
            back.y + 20.0,
        ),
        brushes.primary,
    );
    draw_text(
        context,
        submenu.detail(),
        formats.detail,
        text_rect(
            back.x + 48.0,
            back.y + 18.0,
            layout.content_bounds().x + layout.content_bounds().width,
            back.y + 39.0,
        ),
        brushes.secondary,
    );

    for laid_out in layout.choices() {
        let Some(choice) = submenu
            .choices()
            .iter()
            .find(|choice| choice.id() == laid_out.id())
        else {
            continue;
        };
        draw_choice(context, formats, scene, choice, laid_out.bounds(), brushes);
    }
}

fn draw_choice(
    context: &ID2D1DeviceContext,
    formats: &QuickSettingsFormats<'_>,
    scene: &QuickSettingsScene,
    choice: &QuickSettingsChoice,
    bounds: DipRect,
    brushes: &QuickSettingsBrushes<'_>,
) {
    let hit = QuickSettingsHit::Choice(choice.id());
    let fill = if scene.pressed() == Some(hit) {
        brushes.pressed
    } else if scene.hovered() == Some(hit) {
        brushes.hover
    } else if choice.selected() {
        brushes.selected
    } else {
        brushes.tile
    };
    if scene.focused() == Some(QuickSettingsFocus::Choice(choice.id())) {
        fill_round(
            context,
            rect(
                bounds.x - 1.5,
                bounds.y - 1.5,
                bounds.x + bounds.width + 1.5,
                bounds.y + bounds.height + 1.5,
                13.0,
            ),
            brushes.focus,
        );
    }
    fill_round(
        context,
        rect(
            bounds.x,
            bounds.y,
            bounds.x + bounds.width,
            bounds.y + bounds.height,
            12.0,
        ),
        fill,
    );
    let icon = DipRect::new(bounds.x + 12.0, bounds.y + 11.0, 36.0, 36.0);
    fill_round(
        context,
        rect(
            icon.x,
            icon.y,
            icon.x + icon.width,
            icon.y + icon.height,
            18.0,
        ),
        if choice.selected() {
            brushes.accent
        } else {
            brushes.pressed
        },
    );
    draw_text(
        context,
        choice.glyph(),
        formats.icon,
        text_rect(icon.x, icon.y, icon.x + icon.width, icon.y + icon.height),
        if choice.selected() {
            brushes.on_accent
        } else {
            brushes.primary
        },
    );
    let text_x = icon.x + icon.width + 10.0;
    let text_right = bounds.x + bounds.width - 46.0;
    draw_text(
        context,
        choice.label(),
        formats.label,
        text_rect(text_x, bounds.y + 9.0, text_right, bounds.y + 30.0),
        if choice.enabled() {
            brushes.primary
        } else {
            brushes.secondary
        },
    );
    draw_text(
        context,
        choice.detail(),
        formats.detail,
        text_rect(text_x, bounds.y + 29.0, text_right, bounds.y + 50.0),
        brushes.secondary,
    );
    if choice.selected() {
        draw_text(
            context,
            "\u{E73E}",
            formats.icon,
            text_rect(
                bounds.x + bounds.width - 40.0,
                bounds.y + 11.0,
                bounds.x + bounds.width - 8.0,
                bounds.y + 47.0,
            ),
            brushes.accent,
        );
    }
}

fn draw_tile(
    context: &ID2D1DeviceContext,
    label: &IDWriteTextFormat,
    detail: &IDWriteTextFormat,
    icon: &IDWriteTextFormat,
    tile: &QuickSettingsTile,
    bounds: DipRect,
    scene: &QuickSettingsScene,
    brushes: &QuickSettingsBrushes<'_>,
) {
    let hit = QuickSettingsHit::Tile(tile.kind());
    let fill = if scene.pressed() == Some(hit) {
        brushes.pressed
    } else if scene.hovered() == Some(hit) {
        brushes.hover
    } else if tile.active() {
        brushes.selected
    } else {
        brushes.tile
    };
    if scene.focused() == Some(QuickSettingsFocus::Control(tile.kind())) {
        fill_round(
            context,
            rect(
                bounds.x - 1.5,
                bounds.y - 1.5,
                bounds.x + bounds.width + 1.5,
                bounds.y + bounds.height + 1.5,
                12.0,
            ),
            brushes.focus,
        );
    }
    fill_round(
        context,
        rect(
            bounds.x,
            bounds.y,
            bounds.x + bounds.width,
            bounds.y + bounds.height,
            11.0,
        ),
        fill,
    );
    let icon_x = bounds.x + TILE_INSET;
    let icon_y = bounds.y + (bounds.height - TILE_ICON_SIZE) / 2.0;
    let text_x = icon_x + TILE_ICON_SIZE + TILE_ICON_TEXT_GAP;
    let text_right = bounds.x + bounds.width - TILE_INSET;
    let vertical_inset = if bounds.height >= 64.0 { 8.0 } else { 6.0 };
    let text_midpoint = bounds.y + bounds.height / 2.0;
    fill_round(
        context,
        rect(
            icon_x,
            icon_y,
            icon_x + TILE_ICON_SIZE,
            icon_y + TILE_ICON_SIZE,
            16.0,
        ),
        if tile.active() {
            brushes.accent
        } else {
            brushes.pressed
        },
    );
    draw_text(
        context,
        tile.glyph(),
        icon,
        text_rect(
            icon_x,
            icon_y,
            icon_x + TILE_ICON_SIZE,
            icon_y + TILE_ICON_SIZE,
        ),
        if tile.active() {
            brushes.on_accent
        } else {
            brushes.primary
        },
    );
    draw_text(
        context,
        tile.label(),
        label,
        text_rect(text_x, bounds.y + vertical_inset, text_right, text_midpoint),
        brushes.primary,
    );
    draw_text(
        context,
        tile.detail(),
        detail,
        text_rect(
            text_x,
            text_midpoint - 1.0,
            text_right,
            bounds.y + bounds.height - vertical_inset,
        ),
        brushes.secondary,
    );
}

fn find_tile(scene: &QuickSettingsScene, kind: QuickControlKind) -> Option<&QuickSettingsTile> {
    scene
        .tiles()
        .iter()
        .find(|tile| tile.kind() == kind)
        .or_else(|| {
            scene
                .display()
                .and_then(|display| display.actions().iter().find(|tile| tile.kind() == kind))
        })
        .or_else(|| {
            scene
                .energy()
                .and_then(|energy| energy.saver())
                .filter(|tile| tile.kind() == kind)
        })
}

fn slider_value(scene: &QuickSettingsScene, kind: QuickControlKind) -> Option<u8> {
    match kind {
        QuickControlKind::Brightness => scene.display()?.brightness().map(|slider| slider.value()),
        QuickControlKind::Volume => Some(scene.sound()?.volume().value()),
        _ => None,
    }
}

const fn section_title(kind: QuickControlKind) -> &'static str {
    match kind {
        QuickControlKind::Brightness => "Display",
        QuickControlKind::Volume => "Sound",
        QuickControlKind::Battery => "Energy",
        _ => "",
    }
}

const fn slider_label(kind: QuickControlKind) -> &'static str {
    match kind {
        QuickControlKind::Brightness => "\u{E706}",
        QuickControlKind::Volume => "\u{E767}",
        _ => "",
    }
}

fn text_rect(
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) -> windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
    windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
        left,
        top,
        right,
        bottom,
    }
}
