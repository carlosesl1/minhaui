use shell_config::{
    ConfigLoad, MAX_CONFIG_BYTES, PerformancePreset, RecoveryKind, ShellConfigV1, decode_config,
    encode_config,
};
use shell_core::TaskbarPolicy;

#[test]
fn defaults_are_safe_and_valid() {
    // Given: no user configuration.
    // When: defaults are created.
    let config = ShellConfigV1::default();

    // Then: the taskbar remains untouched and validation succeeds.
    assert_eq!(config.taskbar_policy(), TaskbarPolicy::Off);
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
