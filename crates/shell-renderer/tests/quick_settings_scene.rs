use shell_core::QuickControlKind;

use crate::{
    DipPoint, DipRect, QUICK_SETTINGS_WIDTH, QuickSettingsAudioOutput, QuickSettingsAudioOutputId,
    QuickSettingsAudioPanel, QuickSettingsAudioSession, QuickSettingsAudioSessionId,
    QuickSettingsChoice, QuickSettingsChoiceId, QuickSettingsDisplay, QuickSettingsHit,
    QuickSettingsMediaAction, QuickSettingsMediaChoice, QuickSettingsMediaPlayer,
    QuickSettingsMediaSessionId, QuickSettingsPlaybackState, QuickSettingsScene,
    QuickSettingsSlider, QuickSettingsSound, QuickSettingsSubmenu, QuickSettingsTile,
    layout_quick_settings, quick_settings_surface_size,
};

#[test]
fn detailed_audio_panel_keeps_output_informational_and_settings_actionable() {
    let panel = QuickSettingsAudioPanel::new(
        QuickSettingsSlider::new(QuickControlKind::Volume, "Volume", 42, true),
        vec![QuickSettingsAudioSession::new(
            QuickSettingsAudioSessionId::new(7),
            "Vivaldi",
            "Playing audio",
            68,
            false,
        )],
        vec![QuickSettingsAudioOutput::new(
            QuickSettingsAudioOutputId::new(9),
            "Speakers",
            "Realtek High Definition Audio",
            true,
        )],
        true,
    );
    let scene = QuickSettingsScene::new(Vec::new()).with_audio_panel(Some(panel));
    let (_, height) = quick_settings_surface_size(&scene, 620.0);
    let layout =
        layout_quick_settings(&scene, DipRect::new(0.0, 0.0, QUICK_SETTINGS_WIDTH, height));

    assert_eq!(layout.audio_sessions().len(), 1);
    assert!(layout.audio_outputs().is_empty());
    assert!(layout.audio_settings_bounds().is_some());
    assert!(layout.content_height() <= 620.0);

    let session = layout.audio_sessions()[0];
    assert!(matches!(
        layout.hit_test(DipPoint::new(
            session.track().x + session.track().width * 0.75,
            session.track().y + 2.0,
        )),
        Some(QuickSettingsHit::AudioSessionSlider { id, value: 75 })
            if id == QuickSettingsAudioSessionId::new(7)
    ));
}

#[test]
fn audio_session_identity_and_slider_share_a_balanced_card_geometry() {
    let panel = QuickSettingsAudioPanel::new(
        QuickSettingsSlider::new(QuickControlKind::Volume, "Volume", 42, true),
        vec![QuickSettingsAudioSession::new(
            QuickSettingsAudioSessionId::new(7),
            "Vivaldi",
            "Playing audio",
            68,
            false,
        )],
        Vec::new(),
        false,
    );
    let scene = QuickSettingsScene::new(Vec::new()).with_audio_panel(Some(panel));
    let (_, height) = quick_settings_surface_size(&scene, 620.0);
    let layout =
        layout_quick_settings(&scene, DipRect::new(0.0, 0.0, QUICK_SETTINGS_WIDTH, height));

    let session = layout.audio_sessions()[0];
    assert_eq!(session.bounds().height, 68.0);
    assert_eq!(session.track().x - session.bounds().x, 52.0);
    assert_eq!(session.track().y - session.bounds().y, 40.0);
    assert_eq!(
        session.bounds().x + session.bounds().width - (session.track().x + session.track().width),
        8.0
    );
}

#[test]
fn detailed_audio_panel_never_exposes_endpoints_as_in_app_selectors() {
    let outputs = (0..5)
        .map(|index| {
            QuickSettingsAudioOutput::new(
                QuickSettingsAudioOutputId::new(index + 1),
                &format!("Output {}", index + 1),
                if index == 0 {
                    "Current output"
                } else {
                    "Available output"
                },
                index == 0,
            )
        })
        .collect();
    let panel = QuickSettingsAudioPanel::new(
        QuickSettingsSlider::new(QuickControlKind::Volume, "Volume", 42, true),
        Vec::new(),
        outputs,
        false,
    )
    .with_standalone(true);
    let scene = QuickSettingsScene::new(Vec::new()).with_audio_panel(Some(panel));
    let (_, height) = quick_settings_surface_size(&scene, 900.0);
    let layout =
        layout_quick_settings(&scene, DipRect::new(0.0, 0.0, QUICK_SETTINGS_WIDTH, height));

    assert!(layout.audio_outputs().is_empty());
    let settings = layout.audio_settings_bounds().expect("sound settings");
    assert_eq!(
        layout.hit_test(DipPoint::new(settings.x + 8.0, settings.y + 8.0)),
        Some(QuickSettingsHit::AudioSettings)
    );
    assert!(settings.y + settings.height <= height);
}

fn tile(kind: QuickControlKind) -> QuickSettingsTile {
    QuickSettingsTile::new(kind, "Control", "Off", "x", false, true)
}

fn media() -> QuickSettingsMediaPlayer {
    QuickSettingsMediaPlayer::new(
        QuickSettingsMediaSessionId::new(7),
        "Spotify",
        "A long track title",
        "Artist",
        None,
        QuickSettingsPlaybackState::Paused,
        true,
        true,
        true,
        Some(QuickSettingsMediaAction::TogglePlayback),
    )
}

#[test]
fn media_scene_preserves_player_and_manual_choices() {
    let choice = QuickSettingsMediaChoice::new(
        QuickSettingsMediaSessionId::new(7),
        "Spotify",
        "Track",
        "Artist",
        None,
        QuickSettingsPlaybackState::Playing,
        true,
    );
    let scene = QuickSettingsScene::new(Vec::new())
        .with_media(Some(media()))
        .with_media_choices(Some(vec![choice]));

    let player = scene.media().expect("media player");
    assert_eq!(player.title(), "A long track title");
    assert_eq!(player.artist(), "Artist");
    assert_eq!(
        player.pending(),
        Some(QuickSettingsMediaAction::TogglePlayback)
    );
    assert_eq!(
        scene.media_choices().expect("choices")[0].source(),
        "Spotify"
    );
}

#[test]
fn media_occupies_two_left_slots_beside_focus_and_projection() {
    let scene = QuickSettingsScene::new(vec![
        tile(QuickControlKind::Focus),
        tile(QuickControlKind::Projection),
    ])
    .with_media(Some(media()));
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, QUICK_SETTINGS_WIDTH, 480.0));
    let media = layout.media().expect("media layout");
    let focus = layout
        .tiles()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::Focus)
        .expect("focus")
        .bounds();
    let projection = layout
        .tiles()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::Projection)
        .expect("projection")
        .bounds();

    assert_eq!(media.bounds().height, 120.0);
    assert!(media.bounds().x < focus.x);
    assert_eq!(focus.height, 56.0);
    assert_eq!(projection.y - focus.y, 64.0);
    assert_eq!(
        layout.hit_test(DipPoint::new(
            media.artwork().x + 4.0,
            media.artwork().y + 4.0
        )),
        Some(QuickSettingsHit::MediaArtwork)
    );
    assert_eq!(
        layout.hit_test(DipPoint::new(
            media.toggle().x + 4.0,
            media.toggle().y + 4.0
        )),
        Some(QuickSettingsHit::MediaAction(
            QuickSettingsMediaAction::TogglePlayback
        ))
    );
}

#[test]
fn odd_final_tile_spans_the_two_column_grid() {
    let scene = QuickSettingsScene::new(vec![
        tile(QuickControlKind::Wifi),
        tile(QuickControlKind::Focus),
        tile(QuickControlKind::Projection),
    ]);
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, 320.0, 560.0));

    assert_eq!(layout.tiles().len(), 3);
    assert_eq!(
        layout.tiles()[0].bounds().width,
        layout.tiles()[1].bounds().width
    );
    assert_eq!(
        layout.tiles()[2].bounds().width,
        layout.content_bounds().width
    );
}

#[test]
fn hit_test_distinguishes_tiles_sliders_and_edit_footer() {
    let scene = QuickSettingsScene::new(vec![tile(QuickControlKind::Wifi)]).with_display(Some(
        QuickSettingsDisplay::new(
            Some(QuickSettingsSlider::new(
                QuickControlKind::Brightness,
                "Brightness",
                45,
                true,
            )),
            Vec::new(),
        ),
    ));
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, 320.0, 620.0));
    let tile_bounds = layout.tiles()[0].bounds();
    let slider = layout.brightness().expect("brightness slider");
    let edit = layout.edit_bounds().expect("edit footer");

    assert_eq!(
        layout.hit_test(DipPoint::new(tile_bounds.x + 4.0, tile_bounds.y + 4.0)),
        Some(QuickSettingsHit::Tile(QuickControlKind::Wifi))
    );
    assert!(matches!(
        layout.hit_test(DipPoint::new(
            slider.track().x + slider.track().width / 2.0,
            slider.track().y + 2.0
        )),
        Some(QuickSettingsHit::Slider {
            kind: QuickControlKind::Brightness,
            value: 50
        })
    ));
    assert_eq!(
        layout.hit_test(DipPoint::new(edit.x + 4.0, edit.y + 4.0)),
        Some(QuickSettingsHit::EditControls)
    );
}

#[test]
fn slider_values_are_bounded() {
    assert_eq!(
        QuickSettingsSlider::new(QuickControlKind::Volume, "Volume", 240, true).value(),
        100
    );
}

#[test]
fn brightness_uses_a_full_width_inset_pill_above_display_actions() {
    let scene = QuickSettingsScene::new(Vec::new()).with_display(Some(QuickSettingsDisplay::new(
        Some(QuickSettingsSlider::new(
            QuickControlKind::Brightness,
            "Brightness",
            55,
            true,
        )),
        vec![
            tile(QuickControlKind::DarkMode),
            tile(QuickControlKind::NightLight),
        ],
    )));
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, 368.0, 420.0));
    let brightness = layout.brightness().expect("brightness slider");
    let first_action = layout
        .tiles()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::DarkMode)
        .expect("display action")
        .bounds();

    assert_eq!(brightness.track().x, brightness.bounds().x);
    assert_eq!(brightness.track().width, brightness.bounds().width);
    assert_eq!(brightness.track().height, 28.0);
    assert!(first_action.y >= brightness.bounds().y + brightness.bounds().height + 8.0);
}

#[test]
fn brightness_and_volume_share_the_same_pill_geometry() {
    let scene = QuickSettingsScene::new(Vec::new())
        .with_display(Some(QuickSettingsDisplay::new(
            Some(QuickSettingsSlider::new(
                QuickControlKind::Brightness,
                "Brightness",
                55,
                true,
            )),
            Vec::new(),
        )))
        .with_sound(Some(QuickSettingsSound::new(
            QuickSettingsSlider::new(QuickControlKind::Volume, "Volume", 70, true),
            "Default audio output",
            false,
        )));
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, 368.0, 420.0));
    let brightness = layout.brightness().expect("brightness slider");
    let volume = layout
        .sliders()
        .iter()
        .find(|slider| slider.kind() == QuickControlKind::Volume)
        .expect("volume slider");

    assert_eq!(brightness.track().width, volume.track().width);
    assert_eq!(brightness.track().height, volume.track().height);
    assert_eq!(brightness.track().x, volume.track().x);
}

#[test]
fn slider_feedback_uses_the_live_drag_value_and_disappears_after_release() {
    let dragging = QuickSettingsScene::new(Vec::new())
        .with_pressed(Some(QuickSettingsHit::Slider {
            kind: QuickControlKind::Volume,
            value: 40,
        }))
        .with_hover(Some(QuickSettingsHit::Slider {
            kind: QuickControlKind::Volume,
            value: 72,
        }));
    let released = QuickSettingsScene::new(Vec::new()).with_hover(Some(QuickSettingsHit::Slider {
        kind: QuickControlKind::Volume,
        value: 72,
    }));

    assert_eq!(
        dragging.slider_feedback_value(QuickControlKind::Volume),
        Some(72)
    );
    assert_eq!(
        released.slider_feedback_value(QuickControlKind::Volume),
        None
    );
}

#[test]
fn section_controls_begin_below_their_headers() {
    let scene = QuickSettingsScene::new(Vec::new())
        .with_display(Some(QuickSettingsDisplay::new(
            None,
            vec![tile(QuickControlKind::DarkMode)],
        )))
        .with_sound(Some(QuickSettingsSound::new(
            QuickSettingsSlider::new(QuickControlKind::Volume, "Volume", 70, true),
            "Default audio output",
            false,
        )));
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, 320.0, 560.0));

    let display_section = layout
        .sections()
        .iter()
        .find(|(kind, _)| *kind == QuickControlKind::Brightness)
        .expect("display section")
        .1;
    let display_action = layout
        .tiles()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::DarkMode)
        .expect("display action")
        .bounds();
    let sound_section = layout
        .sections()
        .iter()
        .find(|(kind, _)| *kind == QuickControlKind::Volume)
        .expect("sound section")
        .1;
    let volume = layout
        .sliders()
        .iter()
        .find(|slider| slider.kind() == QuickControlKind::Volume)
        .expect("volume slider")
        .bounds();

    let header_content_offset = 38.0;
    assert!(display_action.y >= display_section.y + header_content_offset);
    assert!(volume.y >= sound_section.y + header_content_offset);
}

#[test]
fn two_column_controls_keep_a_readable_text_column() {
    let scene = QuickSettingsScene::new(vec![
        tile(QuickControlKind::NearbySharing),
        tile(QuickControlKind::Focus),
        tile(QuickControlKind::Multitasking),
        tile(QuickControlKind::Projection),
    ])
    .with_display(Some(QuickSettingsDisplay::new(
        None,
        vec![
            tile(QuickControlKind::DarkMode),
            tile(QuickControlKind::NightLight),
        ],
    )));
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, QUICK_SETTINGS_WIDTH, 620.0));

    let top_level = layout.tiles()[0].bounds();
    let display_action = layout
        .tiles()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::DarkMode)
        .expect("display action")
        .bounds();

    assert!(top_level.width >= 156.0);
    assert!(display_action.width >= 156.0);
}

#[test]
fn controls_follow_the_panel_vertical_rhythm() {
    let scene =
        QuickSettingsScene::new(vec![tile(QuickControlKind::NearbySharing)]).with_display(Some(
            QuickSettingsDisplay::new(None, vec![tile(QuickControlKind::DarkMode)]),
        ));
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, QUICK_SETTINGS_WIDTH, 620.0));

    let top_level = layout.tiles()[0].bounds();
    let display_action = layout
        .tiles()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::DarkMode)
        .expect("display action")
        .bounds();
    let display_section = layout
        .sections()
        .iter()
        .find(|(kind, _)| *kind == QuickControlKind::Brightness)
        .expect("display section")
        .1;

    assert_eq!(top_level.height, 56.0);
    assert_eq!(display_action.height, 48.0);
    assert_eq!(display_action.y - display_section.y, 38.0);
}

#[test]
fn desktop_controls_fit_without_scrolling_or_clipping() {
    let scene = QuickSettingsScene::new(vec![
        tile(QuickControlKind::NearbySharing),
        tile(QuickControlKind::Focus),
        tile(QuickControlKind::Multitasking),
        tile(QuickControlKind::Projection),
    ])
    .with_display(Some(QuickSettingsDisplay::new(
        None,
        vec![
            tile(QuickControlKind::DarkMode),
            tile(QuickControlKind::NightLight),
        ],
    )))
    .with_sound(Some(QuickSettingsSound::new(
        QuickSettingsSlider::new(QuickControlKind::Volume, "Volume", 70, true),
        "Default audio output",
        false,
    )));

    let (_, height) = quick_settings_surface_size(&scene, 420.0);
    let layout =
        layout_quick_settings(&scene, DipRect::new(0.0, 0.0, QUICK_SETTINGS_WIDTH, height));

    assert!(layout.content_height() <= 420.0);
    assert!(layout.tiles()[0].bounds().y >= layout.content_bounds().y);
    let edit = layout.edit_bounds().expect("edit footer");
    assert!(edit.y + edit.height <= height);
}

#[test]
fn connector_has_its_own_space_above_the_panel_padding() {
    let scene = QuickSettingsScene::new(vec![tile(QuickControlKind::NearbySharing)]);
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, QUICK_SETTINGS_WIDTH, 180.0));

    let connector_height = 8.0;
    assert_eq!(layout.content_bounds().y, connector_height + 12.0);
    assert_eq!(layout.tiles()[0].bounds().y - connector_height, 12.0);
}

#[test]
fn contextual_projection_picker_has_large_rows_and_no_edit_footer() {
    let scene = QuickSettingsScene::new(Vec::new()).with_submenu(Some(QuickSettingsSubmenu::new(
        "Projection",
        "Choose how to use your displays",
        vec![QuickSettingsChoice::new(
            QuickSettingsChoiceId::ProjectionExtend,
            "Extend",
            "Use both as one workspace",
            "x",
            true,
            true,
        )],
    )));
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, 368.0, 360.0));
    let choice = layout.choices()[0];

    assert!(choice.bounds().height >= 56.0);
    assert_eq!(layout.edit_bounds(), None);
    assert_eq!(
        layout.hit_test(DipPoint::new(
            choice.bounds().x + 8.0,
            choice.bounds().y + 8.0
        )),
        Some(QuickSettingsHit::Choice(
            QuickSettingsChoiceId::ProjectionExtend
        ))
    );
}
