#[test]
fn crate_reports_identity_when_queried() {
    // Given: the shell-platform-windows crate is linked.
    // When: its stable identity is requested.
    let identity = crate::crate_identity();

    // Then: the package identity is returned.
    assert_eq!(identity, "shell-platform-windows");
}
