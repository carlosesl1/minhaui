use shell_core::QuickControlKind;

use crate::{
    QuickSettingsAudioOutput, QuickSettingsAudioOutputId, QuickSettingsAudioPanel,
    QuickSettingsAudioSession, QuickSettingsAudioSessionId, QuickSettingsScene,
    QuickSettingsSlider, QuickSettingsSound, QuickSettingsTile,
};

#[test]
fn current_panels_model_adaptive_controls_and_standalone_audio() {
    let controls = QuickSettingsScene::new(vec![QuickSettingsTile::new(
        QuickControlKind::Wifi,
        "Wi-Fi",
        "Connected",
        "wifi",
        true,
        true,
    )])
    .with_sound(Some(QuickSettingsSound::new(
        QuickSettingsSlider::new(QuickControlKind::Volume, "Volume", 72, true),
        "Speakers",
        false,
    )));

    let audio = QuickSettingsScene::new(Vec::new()).with_audio_panel(Some(
        QuickSettingsAudioPanel::new(
            QuickSettingsSlider::new(QuickControlKind::Volume, "Master volume", 64, true),
            vec![QuickSettingsAudioSession::new(
                QuickSettingsAudioSessionId::new(7),
                "Music",
                "Playing",
                58,
                false,
            )],
            vec![QuickSettingsAudioOutput::new(
                QuickSettingsAudioOutputId::new(11),
                "Speakers",
                "Default output",
                true,
            )],
            true,
        )
        .with_standalone(true),
    ));

    assert_eq!(controls.tiles()[0].kind(), QuickControlKind::Wifi);
    assert_eq!(
        controls.sound().expect("sound section").volume().value(),
        72
    );

    let audio = audio.audio_panel().expect("standalone audio panel");
    assert!(audio.standalone());
    assert_eq!(audio.master().value(), 64);
    assert_eq!(audio.sessions()[0].label(), "Music");
    assert!(audio.outputs()[0].selected());
}
