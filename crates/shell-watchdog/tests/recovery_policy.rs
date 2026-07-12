use shell_core::TaskbarPolicy;
use shell_watchdog::{
    Consent, ExplorerTaskbarState, RecoveryHook, SafeModeProfile, TaskbarAction, TaskbarMutation,
    TaskbarRequest, begin_taskbar_transaction,
};

#[test]
fn taskbar_policy_stays_off_when_consent_is_missing() {
    let given_request = TaskbarRequest::new(
        TaskbarPolicy::Hide,
        Consent::Missing,
        TaskbarMutation::Enabled,
        SafeModeProfile::normal(),
    );

    let when_transaction = begin_taskbar_transaction(ExplorerTaskbarState::Visible, given_request);

    assert_eq!(when_transaction.original(), ExplorerTaskbarState::Visible);
    assert_eq!(
        when_transaction.startup_action(),
        TaskbarAction::LeaveUntouched
    );
    assert!(when_transaction.recovery().is_always_available());
}

#[test]
fn restore_transaction_restores_original_state_for_recovery_hooks() {
    let given_request = TaskbarRequest::new(
        TaskbarPolicy::AutoHide,
        Consent::Granted,
        TaskbarMutation::Enabled,
        SafeModeProfile::normal(),
    );
    let given_transaction = begin_taskbar_transaction(ExplorerTaskbarState::Visible, given_request);

    let when_hooks = [
        RecoveryHook::NormalExit,
        RecoveryHook::CrashDetected,
        RecoveryHook::FailedStartup,
        RecoveryHook::Update,
        RecoveryHook::Uninstall,
    ];

    for hook in when_hooks {
        assert_eq!(
            given_transaction.restore_action(hook),
            TaskbarAction::Restore(ExplorerTaskbarState::Visible)
        );
    }
}

#[test]
fn safe_mode_blocks_taskbar_mutation_and_disables_optional_features() {
    let given_request = TaskbarRequest::new(
        TaskbarPolicy::Hide,
        Consent::Granted,
        TaskbarMutation::DisabledForTest,
        SafeModeProfile::safe(),
    );

    let when_transaction = begin_taskbar_transaction(ExplorerTaskbarState::AutoHide, given_request);

    assert_eq!(
        when_transaction.startup_action(),
        TaskbarAction::LeaveUntouched
    );
    assert!(when_transaction.safe_mode().taskbar_mutation_blocked());
    assert!(when_transaction.safe_mode().blur_disabled());
    assert!(when_transaction.safe_mode().animations_disabled());
    assert!(when_transaction.safe_mode().third_party_themes_disabled());
    assert!(
        when_transaction
            .safe_mode()
            .optional_network_adapters_disabled()
    );
    assert_eq!(
        when_transaction.safe_mode().status_action(),
        "Exit safe mode"
    );
}
