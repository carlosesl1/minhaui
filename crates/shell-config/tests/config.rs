use shell_config::{
    ConfigLoad, MAX_CONFIG_BYTES, PerformancePreset, RecoveryKind, ShellConfigV1, decode_config,
    encode_config,
};
use shell_core::{
    AppId, DockItem, DockItemId, DockLayoutEntry, DockSeparatorId, TaskbarPolicy, TopbarModule,
    TopbarModuleKind,
};

#[test]
fn defaults_are_safe_and_valid() {
    // Given: no user configuration.
    // When: defaults are created.
    let config = ShellConfigV1::default();

    // Then: the taskbar remains untouched and validation succeeds.
    assert_eq!(config.taskbar_policy(), TaskbarPolicy::Off);
    assert_eq!(config.dock().item_size(), 36);
    assert_eq!(config.dock().spacing(), 9);
    assert!(config.validate().is_ok());
}

#[test]
fn current_schema_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a current valid configuration.
    let config = ShellConfigV1::default().with_autohide(true);

    // When: it is encoded and decoded.
    let encoded = encode_config(&config)?;
    let loaded = decode_config(&encoded);

    // Then: no recovery is needed and all data round-trips.
    assert_eq!(loaded, ConfigLoad::Current(config));
    Ok(())
}

#[test]
fn topbar_order_and_visibility_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let modules = vec![
        TopbarModule::new(TopbarModuleKind::SystemMenu, true),
        TopbarModule::new(TopbarModuleKind::AppIdentity, true),
        TopbarModule::new(TopbarModuleKind::Clock, true),
        TopbarModule::new(TopbarModuleKind::Search, false),
        TopbarModule::new(TopbarModuleKind::Network, true),
        TopbarModule::new(TopbarModuleKind::Volume, true),
        TopbarModule::new(TopbarModuleKind::Power, true),
        TopbarModule::new(TopbarModuleKind::Notifications, false),
    ];
    let config = ShellConfigV1::default().with_topbar_modules(modules.clone());

    let ConfigLoad::Current(decoded) = decode_config(&encode_config(&config)?) else {
        return Err("topbar configuration must remain current".into());
    };

    assert_eq!(
        decoded.topbar_modules(),
        modules
            .iter()
            .filter(|module| module.kind().is_v1_persisted())
            .copied()
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn v1_encoding_never_writes_new_fixed_leading_module_variants()
-> Result<(), Box<dyn std::error::Error>> {
    let config = ShellConfigV1::default().with_topbar_modules(vec![
        TopbarModule::new(TopbarModuleKind::SystemMenu, true),
        TopbarModule::new(TopbarModuleKind::AppIdentity, true),
        TopbarModule::new(TopbarModuleKind::Search, true),
        TopbarModule::new(TopbarModuleKind::Clock, true),
        TopbarModule::new(TopbarModuleKind::Network, true),
        TopbarModule::new(TopbarModuleKind::Volume, true),
        TopbarModule::new(TopbarModuleKind::Power, true),
        TopbarModule::new(TopbarModuleKind::Notifications, true),
    ]);
    let encoded = String::from_utf8(encode_config(&config)?)?;

    assert!(!encoded.contains("app_identity"));
    assert!(!encoded.contains("\"search\""));
    assert_eq!(
        decode_config(encoded.as_bytes()),
        ConfigLoad::Current(config)
    );
    Ok(())
}

#[test]
fn separator_layout_roundtrips_and_old_v1_documents_default_to_legacy_order()
-> Result<(), Box<dyn std::error::Error>> {
    let separator = DockLayoutEntry::Separator(DockSeparatorId::new(90));
    let config = ShellConfigV1::default().with_dock_layout(vec![separator]);

    let encoded = encode_config(&config)?;
    let ConfigLoad::Current(decoded) = decode_config(&encoded) else {
        return Err("separator layout must decode as current config".into());
    };
    assert_eq!(decoded.dock_layout(), &[separator]);

    let legacy = encode_config(&ShellConfigV1::default())?;
    let mut legacy: serde_json::Value = serde_json::from_slice(&legacy)?;
    legacy
        .as_object_mut()
        .ok_or("config must encode as an object")?
        .remove("dock_layout");
    let ConfigLoad::Current(decoded) = decode_config(&serde_json::to_vec(&legacy)?) else {
        return Err("legacy V1 must remain readable".into());
    };
    assert!(decoded.dock_layout().is_empty());
    Ok(())
}

#[test]
fn app_and_separator_cannot_share_a_renderer_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = DockItemId::new(7);
    let config = ShellConfigV1::default()
        .with_dock_items(vec![DockItem::pinned(id, AppId::parse("safe.exe")?)])
        .with_dock_layout(vec![
            DockLayoutEntry::App(id),
            DockLayoutEntry::Separator(DockSeparatorId::new(id.value())),
        ]);

    assert!(config.validate().is_err());
    assert!(matches!(
        decode_config(&serde_json::to_vec(&config)?),
        ConfigLoad::Recovered { .. }
    ));
    Ok(())
}

#[test]
fn v0_fixture_migrates_to_v1() -> Result<(), Box<dyn std::error::Error>> {
    // Given: the committed V0 fixture.
    let bytes = include_bytes!("fixtures/v0.json");

    // When: it is loaded.
    let loaded = decode_config(bytes);

    // Then: its behavior is preserved in a valid V1 config.
    let ConfigLoad::Migrated { config, from } = loaded else {
        return Err("V0 fixture must migrate".into());
    };
    assert_eq!(from, 0);
    assert!(config.autohide());
    assert_eq!(config.taskbar_policy(), TaskbarPolicy::AutoHide);
    assert!(config.accessibility().reduced_motion());
    assert_eq!(config.performance(), PerformancePreset::BatterySaver);
    Ok(())
}

#[test]
fn malformed_and_truncated_input_recovers_to_safe_defaults()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: malformed and truncated JSON documents.
    let cases: [&[u8]; 2] = [b"not-json", br#"{"schema_version":1,"autohide""#];

    // When: each document is decoded.
    for bytes in cases {
        let loaded = decode_config(bytes);

        // Then: recovery is explicit and returns safe validated defaults.
        let ConfigLoad::Recovered { config, report } = loaded else {
            return Err("invalid config must recover".into());
        };
        assert_eq!(report.kind(), RecoveryKind::Malformed);
        assert_eq!(config, ShellConfigV1::default());
    }
    Ok(())
}

#[test]
fn future_schema_is_rejected_explicitly() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a syntactically valid future schema.
    let bytes = br#"{"schema_version":99}"#;

    // When: it is decoded.
    let loaded = decode_config(bytes);

    // Then: safe defaults are returned with a future-version report.
    let ConfigLoad::Recovered { config, report } = loaded else {
        return Err("future config must recover".into());
    };
    assert_eq!(report.kind(), RecoveryKind::FutureVersion);
    assert_eq!(report.detected_version(), Some(99));
    assert_eq!(config, ShellConfigV1::default());
    Ok(())
}

#[test]
fn oversized_input_is_rejected_before_json_parsing() -> Result<(), Box<dyn std::error::Error>> {
    // Given: input larger than the public bound.
    let bytes = vec![b' '; MAX_CONFIG_BYTES + 1];

    // When: it is decoded.
    let loaded = decode_config(&bytes);

    // Then: recovery records the size violation.
    let ConfigLoad::Recovered { report, .. } = loaded else {
        return Err("oversized config must recover".into());
    };
    assert_eq!(report.kind(), RecoveryKind::Oversized);
    Ok(())
}
