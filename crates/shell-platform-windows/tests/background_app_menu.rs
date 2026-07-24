use crate::{
    BackgroundAppId, BackgroundAppMenuCommand, BackgroundAppMenuController, PopoverKey,
    QueuedBackgroundAppMenuAction, background_app_menu_rect,
};
use shell_renderer::{
    DipPoint, DipRect, Dpi, PhysicalRect, context_menu_height_for_entries,
    context_menu_surface_width,
};

fn menu_surface(controller: &BackgroundAppMenuController) -> DipRect {
    let scene = controller.scene().expect("app menu should be open");
    DipRect::new(
        0.0,
        0.0,
        context_menu_surface_width(),
        context_menu_height_for_entries(scene.entries()),
    )
}

#[test]
fn fallback_menu_is_truthfully_shell_owned_and_contains_only_safe_commands() {
    let app = BackgroundAppId::new(41);
    let mut controller = BackgroundAppMenuController::new();

    controller.open(app, false);

    let scene = controller.scene().expect("app menu should be open");
    assert_eq!(controller.target_app(), Some(app));
    let labels = scene
        .entries()
        .iter()
        .filter_map(shell_renderer::ContextMenuEntry::label)
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            "Minha UI menu (shell-owned)",
            "Open or focus",
            "Open file location",
        ]
    );
    assert!(!scene.entries()[0].enabled());
    assert!(
        labels.iter().all(|label| {
            let label = label.to_ascii_lowercase();
            !label.contains("quit") && !label.contains("terminate") && !label.contains("end task")
        }),
        "the shell-owned fallback must not expose destructive process actions"
    );

    assert_eq!(
        controller.handle_key(PopoverKey::Next),
        vec![QueuedBackgroundAppMenuAction::Redraw]
    );
    assert_eq!(
        controller.handle_key(PopoverKey::Activate),
        vec![QueuedBackgroundAppMenuAction::Execute(
            BackgroundAppMenuCommand::OpenOrFocus(app)
        )]
    );
}

#[test]
fn alternate_activation_is_opt_in_and_pointer_release_returns_the_typed_command() {
    let app = BackgroundAppId::new(42);
    let mut controller = BackgroundAppMenuController::new();

    controller.open(app, false);
    assert!(
        controller
            .scene()
            .expect("menu")
            .entries()
            .iter()
            .all(|entry| entry.label() != Some("Try alternate activation"))
    );

    controller.open(app, true);
    let surface = menu_surface(&controller);
    assert_eq!(
        controller.handle_pointer_move(DipPoint::new(40.0, 130.0), surface),
        vec![QueuedBackgroundAppMenuAction::Redraw]
    );
    assert_eq!(
        controller.handle_pointer_release(DipPoint::new(40.0, 130.0), surface),
        vec![QueuedBackgroundAppMenuAction::Execute(
            BackgroundAppMenuCommand::TryAlternateActivation(app)
        )]
    );
    assert!(!controller.is_active());
}

#[test]
fn escape_and_non_action_release_dismiss_the_menu() {
    let mut controller = BackgroundAppMenuController::new();
    controller.open(BackgroundAppId::new(43), false);

    assert_eq!(
        controller.handle_key(PopoverKey::Escape),
        vec![QueuedBackgroundAppMenuAction::Dismiss]
    );
    assert!(!controller.is_active());

    controller.open(BackgroundAppId::new(43), false);
    let surface = menu_surface(&controller);
    assert_eq!(
        controller.handle_pointer_release(DipPoint::new(40.0, 30.0), surface),
        vec![QueuedBackgroundAppMenuAction::Dismiss]
    );
    assert!(!controller.is_active());
}

#[test]
fn app_menu_placement_prefers_right_then_left_and_preserves_negative_work_areas() {
    let dpi = Dpi::from_raw(96);
    let height = 157.0;

    assert_eq!(
        background_app_menu_rect(
            PhysicalRect::new(200, 100, 320, 40),
            PhysicalRect::new(0, 0, 1_200, 800),
            dpi,
            height,
        ),
        PhysicalRect::new(520, 100, 300, 157)
    );
    assert_eq!(
        background_app_menu_rect(
            PhysicalRect::new(850, 700, 100, 40),
            PhysicalRect::new(0, 0, 1_000, 800),
            dpi,
            height,
        ),
        PhysicalRect::new(550, 643, 300, 157)
    );
    assert_eq!(
        background_app_menu_rect(
            PhysicalRect::new(-500, -1_100, 100, 40),
            PhysicalRect::new(-1_920, -1_080, 1_920, 1_040),
            dpi,
            height,
        ),
        PhysicalRect::new(-400, -1_080, 300, 157)
    );
}
