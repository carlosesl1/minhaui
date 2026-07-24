use shell_config::QuickSettingsSettings;
use shell_core::QuickControlKind;
use shell_renderer::{
    DipPoint, DipRect, QuickSettingsAudioOutputId, QuickSettingsFocus, layout_quick_settings,
};

use crate::brightness_coordinator::BrightnessApplyResult;
use crate::{
    AudioOutputId, AudioOutputSnapshot, AudioPanelSnapshot, AudioSessionId, AudioSessionSnapshot,
    DoNotDisturbMode, QueuedQuickSettingsAction, QuickControlAvailability, QuickControlCapability,
    QuickSettingsCapabilities, QuickSettingsController, QuickSettingsIntent, QuickSettingsKey,
};

#[test]
fn sound_header_opens_a_native_audio_detail_page() {
    let snapshot = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Volume,
        QuickControlAvailability::Available { active: false },
        "Sound",
        "Speakers",
        Some(40),
    )]);
    let mut controller =
        QuickSettingsController::new(QuickSettingsSettings::default(), snapshot.clone());
    controller.replace_audio_panel(AudioPanelSnapshot::new(
        vec![AudioSessionSnapshot::new(
            AudioSessionId::new(11),
            "Vivaldi",
            "Playing audio",
            72,
            false,
        )],
        vec![AudioOutputSnapshot::new(
            AudioOutputId::new(17),
            "Speakers",
            "Realtek High Definition Audio",
            true,
        )],
        true,
    ));
    controller.open(snapshot);
    let surface = DipRect::new(0.0, 0.0, 368.0, 620.0);
    let details = layout_quick_settings(&controller.scene(), surface)
        .sound_details_bounds()
        .expect("sound details hit target");
    let point = DipPoint::new(details.x + 8.0, details.y + 8.0);

    controller.pointer_press(point, surface);
    let actions = controller.pointer_release(point, surface);

    let scene = controller.scene();
    let audio = scene.audio_panel().expect("audio detail page");
    assert_eq!(audio.sessions()[0].label(), "Vivaldi");
    assert_eq!(audio.outputs()[0].label(), "Speakers");
    assert_eq!(actions, vec![QueuedQuickSettingsAction::Reflow]);
}

#[test]
fn audio_topbar_module_opens_the_detailed_panel_as_a_root_surface() {
    let capabilities = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Volume,
        QuickControlAvailability::Available { active: false },
        "Sound",
        "Speakers",
        Some(56),
    )]);
    let audio = AudioPanelSnapshot::new(
        vec![AudioSessionSnapshot::new(
            AudioSessionId::new(21),
            "Vivaldi",
            "Playing audio",
            64,
            false,
        )],
        vec![AudioOutputSnapshot::new(
            AudioOutputId::new(22),
            "Speakers",
            "Current output",
            true,
        )],
        true,
    );
    let mut controller = QuickSettingsController::new(
        QuickSettingsSettings::default(),
        QuickSettingsCapabilities::default(),
    );

    controller.open_audio(capabilities, audio);

    let scene = controller.scene();
    assert!(scene.audio_panel().expect("audio panel").standalone());
    let layout = layout_quick_settings(&scene, DipRect::new(0.0, 0.0, 368.0, 720.0));
    assert_eq!(layout.back_bounds(), None);
    assert_eq!(
        scene.audio_panel().unwrap().sessions()[0].label(),
        "Vivaldi"
    );
}

#[test]
fn audio_topbar_module_preserves_every_available_output() {
    let capabilities = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Volume,
        QuickControlAvailability::Available { active: false },
        "Sound",
        "Speakers",
        Some(56),
    )]);
    let outputs = (0..5)
        .map(|index| {
            AudioOutputSnapshot::new(
                AudioOutputId::new(index + 1),
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
    let mut controller = QuickSettingsController::new(
        QuickSettingsSettings::default(),
        QuickSettingsCapabilities::default(),
    );

    controller.open_audio(
        capabilities,
        AudioPanelSnapshot::new(Vec::new(), outputs, false),
    );

    let scene = controller.scene();
    let audio = scene.audio_panel().expect("audio panel");
    assert_eq!(audio.outputs().len(), 5);
    assert_eq!(audio.outputs()[4].label(), "Output 5");

    for _ in 0..5 {
        controller.handle_key(QuickSettingsKey::Next);
    }
    assert_eq!(
        controller.scene().focused(),
        Some(QuickSettingsFocus::AudioOutput(
            QuickSettingsAudioOutputId::new(5)
        ))
    );
}

#[test]
fn audio_session_preserves_its_executable_icon_source_for_rendering() {
    let capabilities = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Volume,
        QuickControlAvailability::Available { active: false },
        "Sound",
        "Speakers",
        Some(56),
    )]);
    let audio = AudioPanelSnapshot::new(
        vec![
            AudioSessionSnapshot::new(
                AudioSessionId::new(31),
                "Vivaldi",
                "Playing audio",
                64,
                false,
            )
            .with_icon_source(r"C:\Program Files\Vivaldi\Application\vivaldi.exe"),
        ],
        Vec::new(),
        false,
    );
    let mut controller = QuickSettingsController::new(
        QuickSettingsSettings::default(),
        QuickSettingsCapabilities::default(),
    );

    controller.open_audio(capabilities, audio);

    assert_eq!(
        controller.scene().audio_panel().unwrap().sessions()[0].icon_source(),
        Some(r"C:\Program Files\Vivaldi\Application\vivaldi.exe")
    );
}

fn capability(
    kind: QuickControlKind,
    availability: QuickControlAvailability,
) -> QuickControlCapability {
    QuickControlCapability::new(kind, availability, "Control", "Off", None)
}

#[test]
fn unsupported_controls_are_removed_and_off_hardware_remains() {
    let snapshot = QuickSettingsCapabilities::new(vec![
        capability(
            QuickControlKind::Bluetooth,
            QuickControlAvailability::Unsupported,
        ),
        capability(
            QuickControlKind::Wifi,
            QuickControlAvailability::Available { active: false },
        ),
        capability(
            QuickControlKind::Focus,
            QuickControlAvailability::RouteOnly { active: None },
        ),
    ]);
    let controller = QuickSettingsController::new(QuickSettingsSettings::default(), snapshot);
    let scene = controller.scene();

    assert_eq!(
        scene
            .tiles()
            .iter()
            .map(|tile| tile.kind())
            .collect::<Vec<_>>(),
        vec![QuickControlKind::Wifi, QuickControlKind::Focus]
    );
    assert!(!scene.tiles()[0].active());
}

#[test]
fn do_not_disturb_route_only_tile_shows_the_observed_active_state() {
    let controller = QuickSettingsController::new(
        QuickSettingsSettings::default(),
        QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
            QuickControlKind::Focus,
            QuickControlAvailability::RouteOnly { active: Some(true) },
            "Do not disturb",
            "On",
            None,
        )]),
    );

    let scene = controller.scene();
    let tile = scene
        .tiles()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::Focus)
        .expect("do not disturb tile");
    assert!(tile.active());
    assert_eq!(tile.label(), "Do not disturb");
    assert_eq!(tile.detail(), "On");
}

#[test]
fn empty_media_player_keeps_its_two_slot_anchor() {
    let controller = QuickSettingsController::new(
        QuickSettingsSettings::default(),
        QuickSettingsCapabilities::new(vec![capability(
            QuickControlKind::Focus,
            QuickControlAvailability::Available { active: false },
        )]),
    );

    let scene = controller.scene();
    let player = scene.media().expect("persistent media player");
    assert_eq!(player.title(), "Nothing playing");
    assert_eq!(player.artist(), "Start audio in any app");
    assert!(!player.can_previous());
    assert!(!player.can_toggle());
    assert!(!player.can_next());
}

#[test]
fn do_not_disturb_and_night_light_use_distinct_system_glyphs() {
    let controller = QuickSettingsController::new(
        QuickSettingsSettings::default(),
        QuickSettingsCapabilities::new(vec![
            capability(
                QuickControlKind::Focus,
                QuickControlAvailability::Available { active: false },
            ),
            capability(
                QuickControlKind::NightLight,
                QuickControlAvailability::Available { active: false },
            ),
        ]),
    );
    let scene = controller.scene();
    let focus = scene
        .tiles()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::Focus)
        .expect("focus tile");
    let night_light = scene
        .display()
        .expect("display section")
        .actions()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::NightLight)
        .expect("night light tile");

    assert_ne!(focus.glyph(), night_light.glyph());
}

#[test]
fn hidden_absent_hardware_keeps_its_saved_preference() {
    let preferences =
        QuickSettingsSettings::default().with_visibility(QuickControlKind::Bluetooth, false);
    let mut controller =
        QuickSettingsController::new(preferences, QuickSettingsCapabilities::default());
    controller.replace_capabilities(QuickSettingsCapabilities::new(vec![capability(
        QuickControlKind::Bluetooth,
        QuickControlAvailability::Available { active: true },
    )]));

    assert!(
        !controller
            .scene()
            .tiles()
            .iter()
            .any(|tile| tile.kind() == QuickControlKind::Bluetooth)
    );
}

#[test]
fn battery_and_volume_become_sections_instead_of_grid_tiles() {
    let controller = QuickSettingsController::new(
        QuickSettingsSettings::default(),
        QuickSettingsCapabilities::new(vec![
            QuickControlCapability::new(
                QuickControlKind::Volume,
                QuickControlAvailability::Available { active: false },
                "Sound",
                "Speakers",
                Some(40),
            ),
            QuickControlCapability::new(
                QuickControlKind::Battery,
                QuickControlAvailability::Available { active: false },
                "Battery",
                "Discharging",
                Some(83),
            ),
        ]),
    );
    let scene = controller.scene();
    assert!(scene.tiles().is_empty());
    assert!(scene.sound().is_some());
    assert_eq!(scene.energy().unwrap().percent(), 83);
}

#[test]
fn volume_follows_the_pointer_while_dragging() {
    let snapshot = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Volume,
        QuickControlAvailability::Available { active: false },
        "Sound",
        "Speakers",
        Some(40),
    )]);
    let mut controller =
        QuickSettingsController::new(QuickSettingsSettings::default(), snapshot.clone());
    controller.open(snapshot);
    let surface = DipRect::new(0.0, 0.0, 368.0, 420.0);
    let slider = layout_quick_settings(&controller.scene(), surface).sliders()[0];
    let point_at = |ratio: f32| {
        DipPoint::new(
            slider.track().x + slider.track().width * ratio,
            slider.track().y + slider.track().height / 2.0,
        )
    };

    controller.pointer_press(point_at(0.25), surface);
    let actions = controller.pointer_move(point_at(0.75), surface);

    assert!(actions.contains(&QueuedQuickSettingsAction::Intent(
        QuickSettingsIntent::SetValue {
            kind: QuickControlKind::Volume,
            value: 75,
        }
    )));
    assert_eq!(controller.scene().sound().unwrap().volume().value(), 75);
}

#[test]
fn brightness_drag_stays_live_while_native_writes_are_coalesced() {
    let snapshot = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Brightness,
        QuickControlAvailability::Available { active: false },
        "Brightness",
        "Display brightness",
        Some(40),
    )]);
    let mut controller =
        QuickSettingsController::new(QuickSettingsSettings::default(), snapshot.clone());
    controller.open(snapshot);
    let surface = DipRect::new(0.0, 0.0, 368.0, 420.0);
    let slider = layout_quick_settings(&controller.scene(), surface)
        .brightness()
        .expect("brightness slider");
    let point_at = |ratio: f32| {
        DipPoint::new(
            slider.track().x + slider.track().width * ratio,
            slider.track().y + slider.track().height / 2.0,
        )
    };

    let first = controller.pointer_press(point_at(0.25), surface);
    let request = first
        .iter()
        .find_map(|action| match action {
            QueuedQuickSettingsAction::Brightness(request) => Some(*request),
            _ => None,
        })
        .expect("first brightness write");
    let during_drag = controller.pointer_move(point_at(0.75), surface);

    assert_eq!(request.value, 25);
    assert_eq!(
        controller
            .scene()
            .display()
            .unwrap()
            .brightness()
            .unwrap()
            .value(),
        75
    );
    assert!(
        !during_drag
            .iter()
            .any(|action| matches!(action, QueuedQuickSettingsAction::Brightness(_)))
    );

    let follow_up = controller.complete_brightness(
        request.id,
        Ok(BrightnessApplyResult {
            value: request.value,
        }),
    );
    assert!(follow_up.iter().any(|action| matches!(
        action,
        QueuedQuickSettingsAction::Brightness(next) if next.value == 75
    )));
}

#[test]
fn applying_one_native_result_preserves_the_live_interaction() {
    let snapshot = QuickSettingsCapabilities::new(vec![
        capability(
            QuickControlKind::DarkMode,
            QuickControlAvailability::Available { active: false },
        ),
        QuickControlCapability::new(
            QuickControlKind::Volume,
            QuickControlAvailability::Available { active: false },
            "Sound",
            "Speakers",
            Some(40),
        ),
    ]);
    let mut controller =
        QuickSettingsController::new(QuickSettingsSettings::default(), snapshot.clone());
    controller.open(snapshot);

    controller.apply_capability_update(QuickControlCapability::new(
        QuickControlKind::Volume,
        QuickControlAvailability::Available { active: false },
        "Sound",
        "Speakers",
        Some(72),
    ));

    assert_eq!(controller.scene().sound().unwrap().volume().value(), 72);
    assert_eq!(controller.capabilities().controls().len(), 2);
}

#[test]
fn projection_tile_opens_an_inline_mode_picker() {
    let snapshot = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Projection,
        QuickControlAvailability::Available { active: false },
        "Projection",
        "PC screen only",
        Some(0),
    )]);
    let mut controller =
        QuickSettingsController::new(QuickSettingsSettings::default(), snapshot.clone());
    controller.open(snapshot);
    let surface = DipRect::new(0.0, 0.0, 368.0, 420.0);
    let tile = layout_quick_settings(&controller.scene(), surface).tiles()[0].bounds();
    let point = DipPoint::new(tile.x + tile.width / 2.0, tile.y + tile.height / 2.0);

    controller.pointer_press(point, surface);
    let actions = controller.pointer_release(point, surface);

    let scene = controller.scene();
    let submenu = scene.submenu().expect("projection submenu");
    assert_eq!(submenu.choices().len(), 4);
    assert_eq!(submenu.choices()[0].label(), "PC screen only");
    assert_eq!(actions, vec![QueuedQuickSettingsAction::Reflow]);
}

#[test]
fn projection_picker_emits_the_selected_native_mode() {
    let snapshot = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Projection,
        QuickControlAvailability::Available { active: false },
        "Projection",
        "PC screen only",
        Some(0),
    )]);
    let mut controller =
        QuickSettingsController::new(QuickSettingsSettings::default(), snapshot.clone());
    controller.open(snapshot);
    let surface = DipRect::new(0.0, 0.0, 368.0, 420.0);
    let tile = layout_quick_settings(&controller.scene(), surface).tiles()[0].bounds();
    let tile_point = DipPoint::new(tile.x + 8.0, tile.y + 8.0);
    controller.pointer_press(tile_point, surface);
    controller.pointer_release(tile_point, surface);

    let layout = layout_quick_settings(&controller.scene(), surface);
    let extend = layout.choices()[2].bounds();
    let point = DipPoint::new(extend.x + 8.0, extend.y + 8.0);
    controller.pointer_press(point, surface);
    let actions = controller.pointer_release(point, surface);

    assert!(actions.contains(&QueuedQuickSettingsAction::Intent(
        QuickSettingsIntent::SetProjectionMode(crate::ProjectionMode::Extend)
    )));
}

#[test]
fn do_not_disturb_tile_opens_a_three_mode_picker() {
    let snapshot = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Focus,
        QuickControlAvailability::Available { active: true },
        "Do not disturb",
        "Priority only",
        Some(1),
    )]);
    let mut controller =
        QuickSettingsController::new(QuickSettingsSettings::default(), snapshot.clone());
    controller.open(snapshot);
    let surface = DipRect::new(0.0, 0.0, 368.0, 420.0);
    let tile = layout_quick_settings(&controller.scene(), surface).tiles()[0].bounds();
    let point = DipPoint::new(tile.x + 8.0, tile.y + 8.0);

    controller.pointer_press(point, surface);
    let actions = controller.pointer_release(point, surface);

    let scene = controller.scene();
    let submenu = scene.submenu().expect("do not disturb submenu");
    assert_eq!(submenu.title(), "Do not disturb");
    assert_eq!(
        submenu
            .choices()
            .iter()
            .map(|choice| choice.label())
            .collect::<Vec<_>>(),
        vec!["Off", "Priority only", "Alarms only"]
    );
    assert!(submenu.choices()[1].selected());
    assert_eq!(actions, vec![QueuedQuickSettingsAction::Reflow]);
}

#[test]
fn do_not_disturb_picker_emits_the_selected_native_mode() {
    let snapshot = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::Focus,
        QuickControlAvailability::Available { active: false },
        "Do not disturb",
        "Off",
        Some(0),
    )]);
    let mut controller =
        QuickSettingsController::new(QuickSettingsSettings::default(), snapshot.clone());
    controller.open(snapshot);
    let surface = DipRect::new(0.0, 0.0, 368.0, 420.0);
    let tile = layout_quick_settings(&controller.scene(), surface).tiles()[0].bounds();
    let tile_point = DipPoint::new(tile.x + 8.0, tile.y + 8.0);
    controller.pointer_press(tile_point, surface);
    controller.pointer_release(tile_point, surface);

    let layout = layout_quick_settings(&controller.scene(), surface);
    let alarms = layout.choices()[2].bounds();
    let point = DipPoint::new(alarms.x + 8.0, alarms.y + 8.0);
    controller.pointer_press(point, surface);
    let actions = controller.pointer_release(point, surface);

    assert!(actions.contains(&QueuedQuickSettingsAction::Intent(
        QuickSettingsIntent::SetDoNotDisturbMode(DoNotDisturbMode::AlarmsOnly)
    )));
}

#[test]
fn night_light_clicks_are_serialized_and_show_latest_desired_direction() {
    let snapshot = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
        QuickControlKind::NightLight,
        QuickControlAvailability::Available { active: false },
        "Night light",
        "Off",
        None,
    )]);
    let mut controller =
        QuickSettingsController::new(QuickSettingsSettings::default(), snapshot.clone());
    controller.open(snapshot);
    let surface = DipRect::new(0.0, 0.0, 368.0, 420.0);
    let tile = layout_quick_settings(&controller.scene(), surface)
        .tiles()
        .iter()
        .find(|tile| tile.kind() == QuickControlKind::NightLight)
        .expect("night light tile")
        .bounds();
    let point = DipPoint::new(tile.x + 8.0, tile.y + 8.0);

    controller.pointer_press(point, surface);
    let first = controller.pointer_release(point, surface);
    assert!(matches!(
        first.as_slice(),
        [
            QueuedQuickSettingsAction::Redraw,
            QueuedQuickSettingsAction::NightLight(_)
        ]
    ));
    assert_eq!(
        controller.scene().display().unwrap().actions()[0].detail(),
        "Turning on..."
    );

    controller.pointer_press(point, surface);
    let second = controller.pointer_release(point, surface);
    assert_eq!(second, vec![QueuedQuickSettingsAction::Redraw]);
    assert_eq!(
        controller.scene().display().unwrap().actions()[0].detail(),
        "Turning off..."
    );
}
