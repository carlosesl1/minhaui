#[test]
fn crate_reports_identity_when_queried() {
    // Given: the shell-renderer crate is linked.
    // When: its stable identity is requested.
    let identity = shell_renderer::crate_identity();

    // Then: the package identity is returned.
    assert_eq!(identity, "shell-renderer");
}
