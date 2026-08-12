use crate::{
    QueuedSettingsAction, SettingsAutomationAction, SettingsController, SettingsEdit,
    SettingsError, SettingsKey, SettingsSection,
};
use shell_config::{
    AppearanceSettings, ConfigStore, ShellConfigV1, ThemeError, ThemePayload, export_theme,
};
use shell_core::{AppId, DockItem, DockItemId, DockLayoutEntry, TopbarModuleKind};
use shell_renderer::{DipPoint, DipRect, SettingsFocus, SettingsHit, layout_settings_scene};

fn qa_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join(format!("shell-settings-{name}-{}", std::process::id()))
        .join("settings.json")
}

#[test]
fn transactional_apply_cancel_and_reset_keep_committed_state_until_apply()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: settings opened from the current persisted configuration.
    let original = ShellConfigV1::default();
    let mut controller = SettingsController::new(original.clone());

    // When: a reversible draft change is previewed and then cancelled.
    controller.edit(SettingsEdit::DockItemSize(60))?;
    assert_eq!(controller.preview().dock().item_size(), 60);
    controller.cancel();

    // Then: runtime returns to the committed configuration.
    assert_eq!(controller.committed(), &original);
    assert_eq!(controller.preview(), &original);

    // When: another draft is applied, then reset.
    controller.edit(SettingsEdit::DockSpacing(12))?;
    let applied = controller.apply()?;
    controller.reset();

    // Then: apply advances the committed state and reset restores defaults as draft preview.
    assert_eq!(applied.dock().spacing(), 12);
    assert_eq!(controller.committed().dock().spacing(), 12);
    assert_eq!(controller.preview(), &ShellConfigV1::default());
    Ok(())
}

#[test]
fn reset_preserves_current_dock_items_and_layout() -> Result<(), Box<dyn std::error::Error>> {
    let id = DockItemId::new(41);
    let original = ShellConfigV1::default()
        .with_dock_items(vec![DockItem::pinned(id, AppId::parse("safe.exe")?)])
        .with_dock_layout(vec![DockLayoutEntry::App(id)]);
    let mut controller = SettingsController::new(original.clone());
    controller.edit(SettingsEdit::DockItemSize(60))?;

    controller.reset();

    assert_eq!(controller.draft().dock_items(), original.dock_items());
    assert_eq!(controller.draft().dock_layout(), original.dock_layout());
    assert_eq!(
        controller.draft().dock().item_size(),
        ShellConfigV1::default().dock().item_size()
    );
    Ok(())
}

#[test]
fn commit_failure_keeps_the_draft_and_surfaces_retry_status()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.edit(SettingsEdit::DockSpacing(12))?;
    let draft = controller.draft().clone();

    controller.commit_failed();

    assert_eq!(controller.draft(), &draft);
    assert!(controller.is_dirty());
    assert_eq!(
        controller.scene().status_text(),
        "Couldn't save changes. Your draft is still available."
    );
    Ok(())
}

#[test]
fn remote_commit_preserves_dirty_draft_and_updates_cancel_baseline()
-> Result<(), Box<dyn std::error::Error>> {
    let original = ShellConfigV1::default();
    let mut controller = SettingsController::new(original.clone());
    controller.edit(SettingsEdit::DockSpacing(12))?;
    let local_draft = controller.draft().clone();
    let remote = original.with_appearance(AppearanceSettings::default().with_blur(18));

    let preserved = controller.reconcile_committed(remote.clone());

    assert!(preserved);
    assert_eq!(controller.committed(), &remote);
    assert_eq!(controller.draft(), &local_draft);
    assert_eq!(controller.preview(), &local_draft);
    assert_eq!(
        controller.scene().status_text(),
        "Settings changed elsewhere. Review your draft before applying."
    );

    controller.cancel();
    assert_eq!(controller.draft(), &remote);
    assert_eq!(controller.preview(), &remote);
    assert!(!controller.is_dirty());
    Ok(())
}

#[test]
fn matching_remote_commit_resolves_the_local_draft_without_conflict()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.edit(SettingsEdit::DockSpacing(12))?;
    let remote = controller.draft().clone();

    let preserved = controller.reconcile_committed(remote.clone());

    assert!(!preserved);
    assert_eq!(controller.committed(), &remote);
    assert_eq!(controller.draft(), &remote);
    assert_eq!(controller.preview(), &remote);
    assert!(!controller.is_dirty());
    assert_eq!(controller.scene().status_text(), "Up to date");
    Ok(())
}

#[test]
fn invalid_draft_values_never_reach_live_runtime_preview() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a valid preview is already live.
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.edit(SettingsEdit::DockItemSize(60))?;
    let valid_preview = controller.preview().clone();

    // When: an out-of-range draft value is entered.
    let result = controller.edit(SettingsEdit::DockItemSize(250));

    // Then: the error is retained in draft state, but runtime preview is unchanged.
    assert!(result.is_err());
    assert_eq!(controller.draft().dock().item_size(), 250);
    assert_eq!(controller.preview(), &valid_preview);
    Ok(())
}

#[test]
fn docktheme_import_export_reuses_checksum_and_rejects_bad_files()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: a theme exported through the checksum-protected shell-config boundary.
    let mut controller = SettingsController::new(ShellConfigV1::default());
    let theme = ThemePayload::default().with_dock_radius(20);
    let bytes = export_theme(&theme)?;

    // When: the file is imported and exported again.
    controller.import_theme(&bytes)?;
    let shared = controller.export_theme()?;

    // Then: the typed theme round-trips through settings.
    assert_eq!(shared, export_theme(&theme)?);

    // When/Then: corrupt, traversal, and oversized themes are rejected without mutation.
    let corrupt = b"{\"schema_version\":1,\"payload\":{},\"checksum_sha256\":\"bad\"}";
    assert!(controller.import_theme(corrupt).is_err());
    let traversal = br##"{
        "schema_version": 1,
        "payload": {
            "name": "Original",
            "tokens": {
                "surface_base": "#11151BEF",
                "surface_raised": "#181D24F2",
                "text_primary": "#F5F7FA",
                "accent_default": "#4C9AFF",
                "dock_radius": 18
            },
            "asset_references": ["../bad.png"]
        },
        "checksum_sha256": "bad"
    }"##;
    assert!(matches!(
        controller.import_theme(traversal),
        Err(SettingsError::Theme(ThemeError::ForbiddenAssetReference))
    ));
    assert!(
        controller
            .import_theme(&vec![b' '; shell_config::MAX_THEME_BYTES + 1])
            .is_err()
    );
    assert_eq!(controller.preview().appearance().theme(), &theme);
    Ok(())
}

#[test]
fn applied_settings_survive_restart_through_config_store() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a settings transaction applied to the persistent store.
    let path = qa_path("restart");
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir_all(parent);
        std::fs::create_dir_all(parent)?;
    }
    let store = ConfigStore::new(path.clone());
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.edit(SettingsEdit::Appearance(
        AppearanceSettings::default().with_blur(18),
    ))?;
    store.persist(&controller.apply()?)?;

    // When: the process starts again and loads from disk.
    let loaded = store.load();
    let restarted = SettingsController::new(loaded.config().clone());

    // Then: the applied setting is the new committed runtime state.
    assert_eq!(restarted.committed().appearance().blur(), 18);
    assert_eq!(restarted.preview().appearance().blur(), 18);
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
    Ok(())
}

#[test]
fn settings_keyboard_navigation_activates_sections_and_escapes()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: settings starts focused on the first section.
    let mut controller = SettingsController::new(ShellConfigV1::default());

    // When: keyboard navigation moves to Appearance and activates it.
    for _ in 0..3 {
        assert_eq!(
            controller.handle_key(SettingsKey::Next)?,
            vec![QueuedSettingsAction::Redraw]
        );
    }
    let actions = controller.handle_key(SettingsKey::Activate)?;

    // Then: activation opens a real content page instead of emitting an ignored action.
    assert_eq!(actions, vec![QueuedSettingsAction::Redraw]);
    assert_eq!(
        controller
            .scene()
            .active_navigation_item()
            .map(shell_renderer::SettingsNavigationItem::label),
        Some("Appearance")
    );

    // When: Escape is pressed after a draft edit.
    controller.edit(SettingsEdit::DockSpacing(12))?;
    let back = controller.handle_key(SettingsKey::Escape)?;
    let escape = controller.handle_key(SettingsKey::Escape)?;

    // Then: Escape first navigates back, then reverts and dismisses the window.
    assert_eq!(back, vec![QueuedSettingsAction::Redraw]);
    assert_eq!(
        escape,
        vec![
            QueuedSettingsAction::RevertConfig,
            QueuedSettingsAction::Dismiss,
        ]
    );
    assert_eq!(controller.preview(), controller.committed());
    Ok(())
}

#[test]
fn quick_controls_editor_lists_only_supported_controls() {
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.set_supported_quick_controls([
        shell_core::QuickControlKind::Wifi,
        shell_core::QuickControlKind::Focus,
    ]);
    controller.open_section(SettingsSection::QuickControls);
    let scene = controller.scene();

    assert_eq!(
        scene
            .quick_controls()
            .unwrap()
            .iter()
            .map(shell_renderer::QuickControlSettingsRow::kind)
            .collect::<Vec<_>>(),
        vec![
            shell_core::QuickControlKind::Wifi,
            shell_core::QuickControlKind::Focus,
        ]
    );
}

#[test]
fn capability_refresh_does_not_move_focus_outside_quick_controls()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = SettingsController::new(ShellConfigV1::default());
    for _ in 0..3 {
        controller.handle_key(SettingsKey::Next)?;
    }

    controller.set_supported_quick_controls([shell_core::QuickControlKind::Wifi]);

    let scene = controller.scene();
    let focused_label = match scene.focus() {
        Some(SettingsFocus::Navigation(section)) => {
            scene.navigation_item(section).map(|item| item.label())
        }
        _ => None,
    };
    assert_eq!(focused_label, Some("Appearance"));
    Ok(())
}

#[test]
fn quick_control_adapter_preserves_typed_modified_state() -> Result<(), Box<dyn std::error::Error>>
{
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.set_supported_quick_controls([shell_core::QuickControlKind::Wifi]);
    controller.open_section(SettingsSection::QuickControls);
    controller.edit(SettingsEdit::QuickControlVisibility {
        control: shell_core::QuickControlKind::Wifi,
        visible: false,
    })?;

    let scene = controller.scene();
    assert_eq!(scene.controls().len(), 1);
    assert!(scene.controls()[0].modified());
    Ok(())
}

#[test]
fn compact_keyboard_focus_reaches_back_and_footer_actions() -> Result<(), Box<dyn std::error::Error>>
{
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.update_surface(DipRect::new(0.0, 0.0, 680.0, 560.0));
    controller.open_section(SettingsSection::Dock);

    controller.handle_key(SettingsKey::Previous)?;
    assert_eq!(controller.scene().focus(), Some(SettingsFocus::Back));
    controller.handle_key(SettingsKey::Next)?;
    assert!(matches!(
        controller.scene().focus(),
        Some(SettingsFocus::Control(_))
    ));

    controller.edit(SettingsEdit::DockSpacing(12))?;
    for _ in 0..5 {
        controller.handle_key(SettingsKey::Next)?;
    }
    assert_eq!(controller.scene().focus(), Some(SettingsFocus::Reset));
    controller.handle_key(SettingsKey::Next)?;
    assert_eq!(controller.scene().focus(), Some(SettingsFocus::Cancel));
    controller.handle_key(SettingsKey::Next)?;
    assert_eq!(controller.scene().focus(), Some(SettingsFocus::Apply));
    assert_eq!(
        controller.handle_key(SettingsKey::Activate)?,
        vec![QueuedSettingsAction::CommitConfig]
    );
    Ok(())
}

#[test]
fn slider_pointer_maps_track_position_to_a_snapped_value() -> Result<(), Box<dyn std::error::Error>>
{
    let mut controller = SettingsController::new(ShellConfigV1::default());
    let surface = DipRect::new(0.0, 0.0, 992.0, 620.0);
    controller.update_surface(surface);
    controller.open_section(SettingsSection::Dock);
    let scene = controller.scene();
    let layout = layout_settings_scene(&scene, surface);
    let bounds = layout.controls()[0].bounds();
    let row_width = (bounds.width - 24.0).max(0.0);
    let track_right = bounds.x + 12.0 + row_width - 8.0;

    let actions = controller.handle_pointer(
        DipPoint::new(track_right - 0.5, bounds.y + bounds.height / 2.0),
        surface,
    )?;

    assert_eq!(controller.draft().dock().item_size(), 72);
    assert_eq!(
        actions,
        vec![
            QueuedSettingsAction::PreviewConfig,
            QueuedSettingsAction::Redraw,
        ]
    );
    Ok(())
}

#[test]
fn hiding_supported_control_preserves_absent_preferences() -> Result<(), Box<dyn std::error::Error>>
{
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.set_supported_quick_controls([
        shell_core::QuickControlKind::Wifi,
        shell_core::QuickControlKind::Focus,
    ]);
    controller.edit(SettingsEdit::QuickControlVisibility {
        control: shell_core::QuickControlKind::Focus,
        visible: false,
    })?;

    assert!(
        controller
            .preview()
            .quick_settings()
            .controls()
            .iter()
            .any(|item| item.kind() == shell_core::QuickControlKind::Bluetooth)
    );
    assert!(
        !controller
            .preview()
            .quick_settings()
            .controls()
            .iter()
            .find(|item| item.kind() == shell_core::QuickControlKind::Focus)
            .ok_or("focus preference missing")?
            .visible()
    );
    Ok(())
}

#[test]
fn topbar_visibility_and_order_follow_transactional_settings()
-> Result<(), Box<dyn std::error::Error>> {
    let original = ShellConfigV1::default();
    let mut controller = SettingsController::new(original.clone());

    controller.edit(SettingsEdit::TopbarVisibility {
        module: TopbarModuleKind::Network,
        visible: false,
    })?;
    controller.edit(SettingsEdit::TopbarReorder {
        module: TopbarModuleKind::Clock,
        before: Some(TopbarModuleKind::Network),
    })?;

    let preview = controller.preview().topbar_modules();
    assert_eq!(preview[1].kind(), TopbarModuleKind::Clock);
    assert_eq!(preview[2].kind(), TopbarModuleKind::Network);
    assert!(!preview[2].visible());
    assert_eq!(controller.committed(), &original);

    controller.cancel();
    assert_eq!(controller.preview(), &original);

    controller.edit(SettingsEdit::TopbarVisibility {
        module: TopbarModuleKind::Power,
        visible: false,
    })?;
    let applied = controller.apply()?;
    assert!(
        !applied
            .topbar_modules()
            .iter()
            .find(|module| module.kind() == TopbarModuleKind::Power)
            .ok_or("missing power module")?
            .visible()
    );
    Ok(())
}

#[test]
fn fixed_leading_topbar_modules_cannot_be_customized() {
    let mut controller = SettingsController::new(ShellConfigV1::default());

    let result = controller.edit(SettingsEdit::TopbarVisibility {
        module: TopbarModuleKind::Search,
        visible: false,
    });

    assert!(matches!(result, Err(SettingsError::Transition(_))));
    assert_eq!(controller.preview(), &ShellConfigV1::default());
}

#[test]
fn automation_invoke_shares_mouse_and_keyboard_reducers() -> Result<(), Box<dyn std::error::Error>>
{
    let surface = DipRect::new(0.0, 0.0, 992.0, 620.0);

    let mut pointer = SettingsController::new(ShellConfigV1::default());
    pointer.update_surface(surface);
    pointer.open_section(SettingsSection::Dock);
    let pointer_layout = layout_settings_scene(&pointer.scene(), surface);
    let toggle = pointer_layout
        .controls()
        .last()
        .copied()
        .ok_or("missing Dock toggle")?;
    let pointer_actions = pointer.handle_pointer(
        DipPoint::new(
            toggle.bounds().x + toggle.bounds().width / 2.0,
            toggle.bounds().y + toggle.bounds().height / 2.0,
        ),
        surface,
    )?;

    let mut automation = SettingsController::new(ShellConfigV1::default());
    automation.update_surface(surface);
    automation.open_section(SettingsSection::Dock);
    let dock_section = automation
        .scene()
        .active_section()
        .ok_or("missing Dock section")?;
    let automation_actions = automation.handle_automation(SettingsAutomationAction::Invoke {
        hit: SettingsHit::Control(toggle.id()),
        expected_section: Some(dock_section),
    })?;

    assert_eq!(automation_actions, pointer_actions);
    assert_eq!(automation.preview(), pointer.preview());

    let choice = automation.scene().controls()[2].id();
    let mut keyboard = SettingsController::new(ShellConfigV1::default());
    keyboard.update_surface(surface);
    keyboard.open_section(SettingsSection::Dock);
    keyboard.handle_key(SettingsKey::Next)?;
    keyboard.handle_key(SettingsKey::Next)?;
    let keyboard_actions = keyboard.handle_key(SettingsKey::Activate)?;

    let mut focused_automation = SettingsController::new(ShellConfigV1::default());
    focused_automation.update_surface(surface);
    focused_automation.open_section(SettingsSection::Dock);
    assert_eq!(
        focused_automation.handle_automation(SettingsAutomationAction::Focus {
            focus: SettingsFocus::Control(choice),
            expected_section: Some(dock_section),
        })?,
        vec![QueuedSettingsAction::Redraw]
    );
    let automation_actions =
        focused_automation.handle_automation(SettingsAutomationAction::Invoke {
            hit: SettingsHit::Control(choice),
            expected_section: Some(dock_section),
        })?;

    assert_eq!(automation_actions, keyboard_actions);
    assert_eq!(focused_automation.preview(), keyboard.preview());
    Ok(())
}

#[test]
fn automation_range_clamps_and_rejects_nan() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.open_section(SettingsSection::Dock);
    let section = controller
        .scene()
        .active_section()
        .ok_or("missing Dock section")?;
    let slider = controller.scene().controls()[0].id();

    let upper = controller.handle_automation(SettingsAutomationAction::SetRange {
        section,
        control: slider,
        position: 140.0,
    })?;
    assert_eq!(controller.preview().dock().item_size(), 72);
    assert_eq!(
        upper,
        vec![
            QueuedSettingsAction::PreviewConfig,
            QueuedSettingsAction::Redraw,
        ]
    );

    controller.handle_automation(SettingsAutomationAction::SetRange {
        section,
        control: slider,
        position: -20.0,
    })?;
    assert_eq!(controller.preview().dock().item_size(), 36);
    let before_nan = controller.preview().clone();
    assert!(matches!(
        controller.handle_automation(SettingsAutomationAction::SetRange {
            section,
            control: slider,
            position: f64::NAN,
        }),
        Err(SettingsError::InvalidAutomationRange)
    ));
    assert_eq!(controller.preview(), &before_nan);
    Ok(())
}

#[test]
fn automation_focus_scrolls_an_offscreen_control_into_view()
-> Result<(), Box<dyn std::error::Error>> {
    let surface = DipRect::new(0.0, 0.0, 992.0, 320.0);
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.update_surface(surface);
    controller.open_section(SettingsSection::Dock);
    let section = controller
        .scene()
        .active_section()
        .ok_or("missing Dock section")?;
    let last_control = controller
        .scene()
        .controls()
        .last()
        .map(shell_renderer::SettingsControl::id)
        .ok_or("missing Dock controls")?;
    let before = layout_settings_scene(&controller.scene(), surface);
    assert!(!before.controls().iter().any(|row| row.id() == last_control));

    assert_eq!(
        controller.handle_automation(SettingsAutomationAction::Focus {
            focus: SettingsFocus::Control(last_control),
            expected_section: Some(section),
        })?,
        vec![QueuedSettingsAction::Redraw]
    );

    let scene = controller.scene();
    assert_eq!(scene.focus(), Some(SettingsFocus::Control(last_control)));
    assert!(scene.control_scroll_offset() > 0);
    assert!(
        layout_settings_scene(&scene, surface)
            .controls()
            .iter()
            .any(|row| row.id() == last_control)
    );
    Ok(())
}

#[test]
fn automation_invoke_covers_navigation_back_and_footer_actions()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = SettingsController::new(ShellConfigV1::default());
    let appearance = controller.scene().navigation()[5].section();
    assert_eq!(
        controller.handle_automation(SettingsAutomationAction::Invoke {
            hit: SettingsHit::Navigation(appearance),
            expected_section: None,
        })?,
        vec![QueuedSettingsAction::Redraw]
    );
    assert_eq!(controller.scene().active_section(), Some(appearance));
    assert_eq!(
        controller.handle_automation(SettingsAutomationAction::Invoke {
            hit: SettingsHit::Back,
            expected_section: Some(appearance),
        })?,
        vec![QueuedSettingsAction::Redraw]
    );
    assert_eq!(controller.scene().active_section(), None);

    controller.edit(SettingsEdit::DockSpacing(12))?;
    assert_eq!(
        controller.handle_automation(SettingsAutomationAction::Invoke {
            hit: SettingsHit::Apply,
            expected_section: None,
        })?,
        vec![QueuedSettingsAction::CommitConfig]
    );
    assert_eq!(
        controller.handle_automation(SettingsAutomationAction::Invoke {
            hit: SettingsHit::Cancel,
            expected_section: None,
        })?,
        vec![
            QueuedSettingsAction::RevertConfig,
            QueuedSettingsAction::Redraw,
        ]
    );
    assert_eq!(controller.preview(), controller.committed());

    controller.edit(SettingsEdit::DockSpacing(12))?;
    assert_eq!(
        controller.handle_automation(SettingsAutomationAction::Invoke {
            hit: SettingsHit::Reset,
            expected_section: None,
        })?,
        vec![
            QueuedSettingsAction::PreviewConfig,
            QueuedSettingsAction::Redraw,
        ]
    );
    Ok(())
}

#[test]
fn stale_control_action_is_ignored_after_navigation_to_same_raw_id()
-> Result<(), Box<dyn std::error::Error>> {
    let mut controller = SettingsController::new(ShellConfigV1::default());
    controller.set_supported_quick_controls(shell_core::QuickControlKind::ALL);
    controller.open_section(SettingsSection::Dock);
    let dock_section = controller
        .scene()
        .active_section()
        .ok_or("missing Dock section")?;
    let stale_control = controller
        .scene()
        .controls()
        .last()
        .map(shell_renderer::SettingsControl::id)
        .ok_or("missing Dock controls")?;

    controller.open_section(SettingsSection::QuickControls);
    let quick_section = controller
        .scene()
        .active_section()
        .ok_or("missing Quick Controls section")?;
    assert_ne!(dock_section, quick_section);
    assert!(
        controller
            .scene()
            .controls()
            .iter()
            .any(|control| control.id() == stale_control),
        "fixture must exercise a raw ID collision"
    );
    let before = controller.preview().clone();

    let actions = controller.handle_automation(SettingsAutomationAction::Invoke {
        hit: SettingsHit::Control(stale_control),
        expected_section: Some(dock_section),
    })?;

    assert!(actions.is_empty());
    assert_eq!(controller.preview(), &before);
    assert_eq!(controller.scene().active_section(), Some(quick_section));
    Ok(())
}
