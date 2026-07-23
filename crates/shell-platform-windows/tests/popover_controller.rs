use crate::{
    BackgroundAppId, DefaultPopoverDataProvider, PopoverAction, PopoverController,
    PopoverDataError, PopoverDataProvider, PopoverItem, PopoverKey, PopoverLoadState,
    PopoverPayload, ProjectionMode, QueuedPopoverAction, SessionAction, TopbarSnapshot,
};

#[test]
fn background_apps_loading_ignores_stale_results_and_scrolls_focus_into_view() {
    let mut controller = PopoverController::new();
    let first = controller.begin_loading(Popover::BackgroundApps);
    let current = controller.begin_loading(Popover::BackgroundApps);

    assert!(!controller.complete_background_apps(first, Ok(background_app_items(20))));
    assert!(controller.complete_background_apps(current, Ok(background_app_items(20))));
    assert_eq!(
        controller.handle_scroll(1),
        vec![QueuedPopoverAction::Redraw]
    );
    for _ in 0..18 {
        controller.handle_key(PopoverKey::Next);
    }

    let scene = controller.scene().expect("background apps scene");
    assert!(scene.scroll_offset() > 0);
    assert_eq!(scene.focused(), Some(18));
}

#[test]
fn background_app_activation_emits_stable_id() {
    let mut controller = PopoverController::new();
    let generation = controller.begin_loading(Popover::BackgroundApps);
    assert!(controller.complete_background_apps(generation, Ok(background_app_items(1))));

    assert_eq!(
        controller.handle_key(PopoverKey::Activate),
        vec![QueuedPopoverAction::TypedIntent(
            PopoverAction::OpenBackgroundApp(BackgroundAppId::new(0))
        )]
    );
}

fn background_app_items(count: usize) -> Vec<PopoverItem> {
    (0..count)
        .map(|index| {
            PopoverItem::new(
                &format!("App {index:02}"),
                "",
                true,
                Some(PopoverAction::OpenBackgroundApp(BackgroundAppId::new(
                    index as u64,
                ))),
            )
            .with_icon_source(Some(format!(r"C:\Apps\app{index:02}.exe")))
        })
        .collect()
}
use shell_core::{CalendarDate, Popover};
use shell_renderer::{
    DipPoint, DipRect, Dpi, PhysicalRect, PopoverContentState, layout_popover_scene,
    popover_anchor_rect, popover_anchor_rect_with_height, popover_height_for_rows,
};

#[test]
fn popover_keyboard_navigation_and_confirmation_are_shared_across_modules()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: the power popover is opened through the default provider.
    let provider = DefaultPopoverDataProvider::offline();
    let mut controller = PopoverController::new();
    controller.open(Popover::Power, &provider)?;

    // When: keyboard navigation skips informational rows and reaches sign out.
    assert_eq!(
        controller.handle_key(PopoverKey::Next),
        vec![QueuedPopoverAction::Redraw]
    );
    assert_eq!(
        controller.handle_key(PopoverKey::Next),
        vec![QueuedPopoverAction::Redraw]
    );
    let first = controller.handle_key(PopoverKey::Activate);
    let second = controller.handle_key(PopoverKey::Activate);

    // Then: the first activation only asks for confirmation; the second emits intent.
    assert_eq!(
        first,
        vec![QueuedPopoverAction::RequestConfirmation(
            SessionAction::SignOut
        )]
    );
    assert_eq!(
        second,
        vec![QueuedPopoverAction::TypedIntent(
            crate::PopoverAction::ConfirmSession(SessionAction::SignOut)
        )]
    );
    Ok(())
}

#[test]
fn system_menu_settings_activation_emits_open_settings_intent()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: the system menu is opened through the default provider.
    let provider = DefaultPopoverDataProvider::offline();
    let mut controller = PopoverController::new();
    controller.open(Popover::SystemMenu, &provider)?;

    // When: the focused settings row is activated.
    let actions = controller.handle_key(PopoverKey::Activate);

    // Then: the native action layer can open the settings surface.
    assert_eq!(
        actions,
        vec![QueuedPopoverAction::TypedIntent(
            PopoverAction::OpenSettings
        )]
    );
    Ok(())
}

#[test]
fn system_menu_rows_activate_from_pointer_input() -> Result<(), Box<dyn std::error::Error>> {
    let provider = DefaultPopoverDataProvider::offline();
    let mut controller = PopoverController::new();
    controller.open(Popover::SystemMenu, &provider)?;
    let actions = controller.handle_pointer(
        DipPoint::new(24.0, 17.0),
        DipRect::new(0.0, 0.0, 244.0, 82.0),
    );
    assert_eq!(
        actions,
        vec![QueuedPopoverAction::TypedIntent(
            PopoverAction::OpenSettings
        )]
    );
    Ok(())
}

#[test]
fn popover_layout_anchors_inside_monitor_work_area_at_current_dpi()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a right-edge topbar anchor on a scaled secondary monitor.
    let work = PhysicalRect::new(1920, 0, 1280, 900);
    let anchor = PhysicalRect::new(3060, 12, 128, 40);

    // When: the popover anchor is calculated at 150 percent scale.
    let rect = popover_anchor_rect(anchor, work, Dpi::from_raw(144));

    // Then: it stays inside that monitor and uses the token-driven popover size.
    assert!(rect.x >= work.x);
    assert!(rect.x + rect.width <= work.x + work.width);
    assert_eq!(rect.width, 366);
    assert_eq!(rect.height, 630);
    Ok(())
}

#[test]
fn compact_popover_height_tracks_content_rows() {
    assert_eq!(popover_height_for_rows(3), 82.0);
    let work = PhysicalRect::new(0, 0, 1920, 1080);
    let anchor = PhysicalRect::new(1600, 0, 120, 32);
    let rect = popover_anchor_rect_with_height(
        anchor,
        work,
        Dpi::from_raw(96),
        popover_height_for_rows(3),
    );
    assert_eq!(rect.height, 82);
}

#[test]
fn menu_rows_match_the_compact_figma_rhythm() -> Result<(), Box<dyn std::error::Error>> {
    // Given: the complete functional system menu in the compact native surface.
    let mut controller = PopoverController::new();
    controller.open(Popover::SystemMenu, &DefaultPopoverDataProvider::offline())?;
    let scene = controller.scene().ok_or("missing system menu")?;

    // When: the menu rows are laid out at the Figma node's intrinsic width.
    let layout = layout_popover_scene(&scene, DipRect::new(0.0, 0.0, 244.0, 178.0));

    // Then: all actions keep the existing 12-DIP inset and 24-DIP row rhythm.
    assert_eq!(layout.rows().len(), 7);
    assert_eq!(
        layout.rows()[0].bounds(),
        DipRect::new(12.0, 5.0, 220.0, 24.0)
    );
    assert_eq!(
        layout.rows()[6].bounds(),
        DipRect::new(12.0, 149.0, 220.0, 24.0)
    );
    Ok(())
}

#[test]
fn changing_focus_cancels_a_pending_session_confirmation() -> Result<(), Box<dyn std::error::Error>>
{
    let provider = DefaultPopoverDataProvider::offline();
    let mut controller = PopoverController::new();
    controller.open(Popover::SystemMenu, &provider)?;

    controller.handle_key(PopoverKey::Next);
    controller.handle_key(PopoverKey::Next);
    assert_eq!(
        controller.handle_key(PopoverKey::Activate),
        vec![QueuedPopoverAction::RequestConfirmation(
            SessionAction::Lock
        )]
    );

    controller.handle_key(PopoverKey::Next);
    controller.handle_key(PopoverKey::Previous);

    assert_eq!(
        controller.handle_key(PopoverKey::Activate),
        vec![QueuedPopoverAction::RequestConfirmation(
            SessionAction::Lock
        )]
    );
    Ok(())
}

#[test]
fn popovers_use_the_latest_snapshot_and_calendar_navigation_reloads_the_month()
-> Result<(), Box<dyn std::error::Error>> {
    let provider = DefaultPopoverDataProvider::offline();
    let snapshot = TopbarSnapshot::new(
        "10:30".to_owned(),
        crate::NetworkSnapshot::new("Online", 321, 45),
        73,
        crate::PowerSnapshot::new(Some(64), false),
        0,
    )
    .with_local_date(CalendarDate::new(2026, 7, 16).ok_or("invalid date")?);
    let mut controller = PopoverController::new();

    controller.open_with_snapshot(Popover::Network, &provider, &snapshot)?;
    let network = controller.scene().ok_or("missing network scene")?;
    assert_eq!(network.rows()[1].detail(), "321 KiB/s");
    assert_eq!(network.rows()[2].detail(), "45 KiB/s");

    controller.open_with_snapshot(Popover::Calendar, &provider, &snapshot)?;
    assert_eq!(
        controller.scene().ok_or("missing calendar scene")?.rows()[1].label(),
        "July"
    );
    controller.handle_key(PopoverKey::Next);
    controller.handle_key(PopoverKey::Next);
    assert_eq!(
        controller.handle_key(PopoverKey::Activate),
        vec![
            QueuedPopoverAction::TypedIntent(PopoverAction::CalendarNext),
            QueuedPopoverAction::Reload,
        ]
    );
    controller.reload(&provider, &snapshot)?;
    assert_eq!(
        controller
            .scene()
            .ok_or("missing reloaded calendar scene")?
            .rows()[1]
            .label(),
        "August"
    );
    Ok(())
}

#[test]
fn control_center_exposes_all_windows_projection_modes() -> Result<(), Box<dyn std::error::Error>> {
    let payload = DefaultPopoverDataProvider::offline().load(
        Popover::Notifications,
        &TopbarSnapshot::default(),
        0,
    )?;
    let PopoverLoadState::Ready(items) = payload.state() else {
        return Err("control center must be ready".into());
    };
    let projection = items
        .iter()
        .filter_map(|item| {
            let Some(PopoverAction::SetProjectionMode(mode)) = item.action() else {
                return None;
            };
            Some((item.label(), *mode))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        projection,
        vec![
            ("Somente tela do PC", ProjectionMode::Internal),
            ("Duplicar", ProjectionMode::Duplicate),
            ("Estender", ProjectionMode::Extend),
            ("Somente segunda tela", ProjectionMode::External),
        ]
    );
    Ok(())
}

#[test]
fn dismiss_clears_active_popover_and_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = PopoverController::new();
    controller.open(Popover::Calendar, &DefaultPopoverDataProvider::offline())?;
    assert_eq!(controller.active_kind(), Some(Popover::Calendar));
    assert_eq!(controller.dismiss(), vec![QueuedPopoverAction::Dismiss]);
    assert_eq!(controller.active_kind(), None);
    assert!(controller.dismiss().is_empty());
    Ok(())
}

#[test]
fn offline_weather_and_adapter_failures_render_without_crashing_host()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: the default provider has no network opt-in and another provider fails.
    let mut controller = PopoverController::new();
    controller.open(Popover::Calendar, &DefaultPopoverDataProvider::offline())?;
    let offline = controller.scene().ok_or("missing offline scene")?;

    // When: a provider returns an adapter failure.
    controller.open(Popover::Network, &FailingProvider)?;
    let failed = controller.scene().ok_or("missing failure scene")?;

    // Then: both states are represented in renderer scenes and can be laid out.
    assert_eq!(offline.state(), PopoverContentState::Offline);
    assert_eq!(failed.state(), PopoverContentState::Error);
    assert!(
        !layout_popover_scene(&offline, DipRect::new(0.0, 0.0, 360.0, 420.0))
            .rows()
            .is_empty()
    );
    assert!(
        layout_popover_scene(&failed, DipRect::new(0.0, 0.0, 360.0, 420.0))
            .rows()
            .is_empty()
    );
    Ok(())
}

struct FailingProvider;

impl PopoverDataProvider for FailingProvider {
    fn load(
        &self,
        kind: Popover,
        _snapshot: &crate::TopbarSnapshot,
        _calendar_offset: i16,
    ) -> Result<PopoverPayload, PopoverDataError> {
        let _ = kind;
        Err(PopoverDataError::Adapter("simulated failure"))
    }
}
