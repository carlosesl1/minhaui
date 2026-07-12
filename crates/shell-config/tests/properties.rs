use proptest::prelude::*;
use shell_config::{
    ConfigLoad, ShellConfigV1, ThemeError, ThemePayload, decode_config, encode_config,
    export_theme, import_theme,
};

proptest! {
    #[test]
    fn arbitrary_valid_configs_roundtrip(autohide in any::<bool>()) {
        // Given: an arbitrary valid V1 config.
        let config = ShellConfigV1::default().with_autohide(autohide);

        // When: it crosses the JSON boundary twice.
        let encoded = encode_config(&config)?;
        let decoded = decode_config(&encoded);

        // Then: the typed value is unchanged.
        prop_assert_eq!(decoded, ConfigLoad::Current(config));
    }

    #[test]
    fn arbitrary_valid_themes_roundtrip(radius in 6_u16..=24) {
        // Given: an arbitrary bounded original token value.
        let theme = ThemePayload::default().with_dock_radius(radius);

        // When: it crosses a docktheme envelope.
        let exported = export_theme(&theme)?;
        let imported = import_theme(&exported)?;

        // Then: the checksum-protected payload is unchanged.
        prop_assert_eq!(imported, theme);
    }

    #[test]
    fn malformed_config_bytes_always_recover_without_panicking(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        // Given: arbitrary untrusted bytes below the public size bound.
        // When: the config parser handles them at the boundary.
        let loaded = decode_config(&bytes);

        // Then: it either produces a validated config or an explicit recovery report.
        match loaded {
            ConfigLoad::Current(config)
            | ConfigLoad::Migrated { config, .. }
            | ConfigLoad::Recovered { config, .. } => prop_assert!(config.validate().is_ok()),
        }
    }

    #[test]
    fn unsafe_theme_asset_references_are_inert(reference in r"(?i)(https?://|file://|[a-z]:\\|/|\\|\.\./).{0,80}") {
        // Given: a theme payload containing URL, absolute, drive, or traversal syntax.
        let theme = ThemePayload::default().with_asset(&reference);

        // When: the payload is exported.
        let result = export_theme(&theme);

        // Then: unsafe references never enter an importable theme envelope.
        prop_assert_eq!(result, Err(ThemeError::ForbiddenAssetReference));
    }
}
