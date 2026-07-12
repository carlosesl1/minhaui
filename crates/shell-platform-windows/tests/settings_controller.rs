use shell_config::{
    AppearanceSettings, ConfigStore, ShellConfigV1, ThemeError, ThemePayload, export_theme,
};
use shell_platform_windows::{
    QueuedSettingsAction, SettingsController, SettingsEdit, SettingsError, SettingsKey,
    SettingsSection,
};

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
    for _ in 0..4 {
        assert_eq!(
            controller.handle_key(SettingsKey::Next)?,
            vec![QueuedSettingsAction::Redraw]
        );
    }
    let actions = controller.handle_key(SettingsKey::Activate)?;

    // Then: activation emits the typed section instead of doing nothing.
    assert_eq!(
        actions,
        vec![QueuedSettingsAction::OpenSection(
            SettingsSection::Appearance
        )]
    );

    // When: Escape is pressed after a draft edit.
    controller.edit(SettingsEdit::DockSpacing(12))?;
    let escape = controller.handle_key(SettingsKey::Escape)?;

    // Then: the draft is cancelled and the native settings surface can close.
    assert_eq!(escape, vec![QueuedSettingsAction::Dismiss]);
    assert_eq!(controller.preview(), controller.committed());
    Ok(())
}
