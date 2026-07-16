use shell_core::{ShellState, TopbarModuleKind};

#[test]
fn keeps_clock_at_the_end_of_the_default_status_cluster() {
    // Given: a new shell state with the default topbar configuration.
    let state = ShellState::default();

    // When: its visible module order is read.
    let last = state.topbar_modules().last().map(|module| module.kind());

    // Then: the clock anchors the right edge of the status cluster.
    assert_eq!(last, Some(TopbarModuleKind::Clock));
}
