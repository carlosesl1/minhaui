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

#[test]
fn priority_one_modules_have_a_stable_default_order() {
    let kinds = ShellState::default()
        .topbar_modules()
        .iter()
        .map(|module| module.kind())
        .collect::<Vec<_>>();

    assert_eq!(
        kinds,
        vec![
            TopbarModuleKind::SystemMenu,
            TopbarModuleKind::AppIdentity,
            TopbarModuleKind::Search,
            TopbarModuleKind::Network,
            TopbarModuleKind::Volume,
            TopbarModuleKind::Power,
            TopbarModuleKind::Notifications,
            TopbarModuleKind::BackgroundApps,
            TopbarModuleKind::Clock,
        ]
    );
}

#[test]
fn background_apps_is_runtime_only_and_precedes_clock() {
    let state = ShellState::default();
    let kinds = state
        .topbar_modules()
        .iter()
        .map(|module| module.kind())
        .collect::<Vec<_>>();
    let background = kinds
        .iter()
        .position(|kind| *kind == TopbarModuleKind::BackgroundApps)
        .expect("background apps module");
    let clock = kinds
        .iter()
        .position(|kind| *kind == TopbarModuleKind::Clock)
        .expect("clock module");

    assert_eq!(background + 1, clock);
    assert!(!TopbarModuleKind::BackgroundApps.is_v1_persisted());
    assert!(!TopbarModuleKind::BackgroundApps.is_customizable());
}
