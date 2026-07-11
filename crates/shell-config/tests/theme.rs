use shell_config::{MAX_THEME_BYTES, ThemeError, ThemePayload, export_theme, import_theme};

#[test]
fn theme_export_import_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
    // Given: original bounded theme tokens.
    let theme = ThemePayload::default();

    // When: a docktheme is exported and imported.
    let bytes = export_theme(&theme)?;
    let imported = import_theme(&bytes)?;

    // Then: the exact typed payload round-trips.
    assert_eq!(imported, theme);
    Ok(())
}

#[test]
fn checksum_tampering_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // Given: an exported docktheme whose payload bytes are changed.
    let bytes = export_theme(&ThemePayload::default())?;
    let mut text = String::from_utf8(bytes)?;
    text = text.replace("#11151BEF", "#21151BEF");

    // When: it is imported.
    let result = import_theme(text.as_bytes());

    // Then: the deterministic checksum detects tampering.
    assert_eq!(result, Err(ThemeError::ChecksumMismatch));
    Ok(())
}

#[test]
fn traversal_and_external_asset_references_are_rejected() {
    // Given: theme payloads with unsafe asset references.
    let references = [
        "../secret.png",
        "folder\\..\\secret.png",
        "https://example.test/a.png",
    ];

    // When: each payload is exported.
    for reference in references {
        let theme = ThemePayload::default().with_asset(reference);
        let result = export_theme(&theme);

        // Then: unsafe references never enter a docktheme envelope.
        assert_eq!(result, Err(ThemeError::ForbiddenAssetReference));
    }
}

#[test]
fn oversized_theme_is_rejected() {
    // Given: an envelope larger than the theme bound.
    let bytes = vec![b' '; MAX_THEME_BYTES + 1];

    // When: it is imported.
    let result = import_theme(&bytes);

    // Then: parsing is refused before allocation-heavy JSON work.
    assert_eq!(result, Err(ThemeError::Oversized));
}
