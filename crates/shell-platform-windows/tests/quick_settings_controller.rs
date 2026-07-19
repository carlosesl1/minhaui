use shell_config::QuickSettingsSettings;
use shell_core::QuickControlKind;
use shell_renderer::{DipPoint, DipRect, layout_quick_settings};

use crate::{
    QueuedQuickSettingsAction, QuickControlAvailability, QuickControlCapability,
    QuickSettingsCapabilities, QuickSettingsController, QuickSettingsIntent,
};

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
        capability(QuickControlKind::Focus, QuickControlAvailability::RouteOnly),
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
