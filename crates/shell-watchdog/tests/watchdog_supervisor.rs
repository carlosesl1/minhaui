use shell_watchdog::{
    ChildCleanup, ChildEvent, RestartDelay, SupervisorAction, SupervisorConfig, SupervisorState,
};

#[test]
fn watchdog_uses_bounded_exponential_backoff_for_missed_heartbeats() {
    let given_config = SupervisorConfig::new(
        RestartDelay::from_millis(250),
        RestartDelay::from_millis(1_000),
        4,
    );
    let mut given_state = SupervisorState::default();

    let when_first = given_state.observe(ChildEvent::HeartbeatMissed, given_config);
    let when_second = given_state.observe(ChildEvent::HeartbeatMissed, given_config);
    let when_third = given_state.observe(ChildEvent::HeartbeatMissed, given_config);

    assert_eq!(
        [when_first, when_second, when_third],
        [
            SupervisorAction::Restart {
                delay: RestartDelay::from_millis(250),
            },
            SupervisorAction::Restart {
                delay: RestartDelay::from_millis(500),
            },
            SupervisorAction::Restart {
                delay: RestartDelay::from_millis(1_000),
            },
        ]
    );
}

#[test]
fn watchdog_enters_safe_mode_instead_of_infinite_restart_loop() {
    let given_config = SupervisorConfig::new(
        RestartDelay::from_millis(100),
        RestartDelay::from_millis(1_000),
        2,
    );
    let mut given_state = SupervisorState::default();
    let given_restart = given_state.observe(ChildEvent::ChildCrashed, given_config);

    let when_safe = given_state.observe(ChildEvent::ChildCrashed, given_config);

    assert_eq!(
        given_restart,
        SupervisorAction::Restart {
            delay: RestartDelay::from_millis(100),
        }
    );
    assert_eq!(when_safe, SupervisorAction::EnterSafeMode);
    assert!(given_state.safe_mode_active());
}

#[test]
fn shutdown_cleanup_escalates_after_bounded_grace_period() {
    let given_cleanup = ChildCleanup::new(RestartDelay::from_millis(250));

    let when_requested = given_cleanup.action_after_shutdown(0);
    let when_expired = given_cleanup.action_after_shutdown(251);

    assert_eq!(when_requested, SupervisorAction::Continue);
    assert_eq!(when_expired, SupervisorAction::EnterSafeMode);
}

#[test]
fn one_heartbeat_does_not_erase_a_crash_loop() {
    let config = SupervisorConfig::new(
        RestartDelay::from_millis(100),
        RestartDelay::from_millis(1_000),
        2,
    );
    let mut state = SupervisorState::default();

    assert!(matches!(
        state.observe(ChildEvent::ChildCrashed, config),
        SupervisorAction::Restart { .. }
    ));
    assert_eq!(
        state.observe(ChildEvent::HeartbeatReceived, config),
        SupervisorAction::Continue
    );
    assert_eq!(
        state.observe(ChildEvent::ChildCrashed, config),
        SupervisorAction::EnterSafeMode
    );
}

#[test]
fn a_sustained_healthy_window_resets_the_crash_budget() {
    let config = SupervisorConfig::new(
        RestartDelay::from_millis(100),
        RestartDelay::from_millis(1_000),
        2,
    );
    let mut state = SupervisorState::default();

    let _ = state.observe(ChildEvent::ChildCrashed, config);
    let _ = state.observe(ChildEvent::HealthyWindowElapsed, config);

    assert!(matches!(
        state.observe(ChildEvent::ChildCrashed, config),
        SupervisorAction::Restart { .. }
    ));
}
