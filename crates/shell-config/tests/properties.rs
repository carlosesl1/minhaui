use proptest::prelude::*;
use shell_config::{
    ConfigLoad, ShellConfigV1, ThemePayload, decode_config, encode_config, export_theme,
    import_theme,
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
}
