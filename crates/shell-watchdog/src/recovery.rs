use shell_core::TaskbarPolicy;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplorerTaskbarState {
    Visible,
    AutoHide,
    Hidden,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Consent {
    Missing,
    Granted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskbarMutation {
    Enabled,
    DisabledForTest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryHook {
    NormalExit,
    CrashDetected,
    FailedStartup,
    Update,
    Uninstall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskbarAction {
    LeaveUntouched,
    Apply(ExplorerTaskbarState),
    Restore(ExplorerTaskbarState),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafeModeProfile {
    active: bool,
}

impl SafeModeProfile {
    #[must_use]
    pub const fn normal() -> Self {
        Self { active: false }
    }

    #[must_use]
    pub const fn safe() -> Self {
        Self { active: true }
    }

    #[must_use]
    pub const fn taskbar_mutation_blocked(self) -> bool {
        self.active
    }

    #[must_use]
    pub const fn blur_disabled(self) -> bool {
        self.active
    }

    #[must_use]
    pub const fn animations_disabled(self) -> bool {
        self.active
    }

    #[must_use]
    pub const fn third_party_themes_disabled(self) -> bool {
        self.active
    }

    #[must_use]
    pub const fn optional_network_adapters_disabled(self) -> bool {
        self.active
    }

    #[must_use]
    pub const fn status_action(self) -> &'static str {
        if self.active {
            "Exit safe mode"
        } else {
            "Safe mode inactive"
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskbarRequest {
    policy: TaskbarPolicy,
    consent: Consent,
    mutation: TaskbarMutation,
    safe_mode: SafeModeProfile,
}

impl TaskbarRequest {
    #[must_use]
    pub const fn new(
        policy: TaskbarPolicy,
        consent: Consent,
        mutation: TaskbarMutation,
        safe_mode: SafeModeProfile,
    ) -> Self {
        Self {
            policy,
            consent,
            mutation,
            safe_mode,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryShortcut {
    hotkey_registered: bool,
    shortcut_installed: bool,
}

impl RecoveryShortcut {
    #[must_use]
    pub const fn always_available() -> Self {
        Self {
            hotkey_registered: true,
            shortcut_installed: true,
        }
    }

    #[must_use]
    pub const fn is_always_available(self) -> bool {
        self.hotkey_registered && self.shortcut_installed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskbarTransaction {
    original: ExplorerTaskbarState,
    startup_action: TaskbarAction,
    recovery: RecoveryShortcut,
    safe_mode: SafeModeProfile,
}

impl TaskbarTransaction {
    #[must_use]
    pub const fn original(self) -> ExplorerTaskbarState {
        self.original
    }

    #[must_use]
    pub const fn startup_action(self) -> TaskbarAction {
        self.startup_action
    }

    #[must_use]
    pub const fn recovery(self) -> RecoveryShortcut {
        self.recovery
    }

    #[must_use]
    pub const fn safe_mode(self) -> SafeModeProfile {
        self.safe_mode
    }

    #[must_use]
    pub const fn restore_action(self, hook: RecoveryHook) -> TaskbarAction {
        match hook {
            RecoveryHook::NormalExit
            | RecoveryHook::CrashDetected
            | RecoveryHook::FailedStartup
            | RecoveryHook::Update
            | RecoveryHook::Uninstall => TaskbarAction::Restore(self.original),
        }
    }
}

#[must_use]
pub const fn begin_taskbar_transaction(
    original: ExplorerTaskbarState,
    request: TaskbarRequest,
) -> TaskbarTransaction {
    TaskbarTransaction {
        original,
        startup_action: startup_action(request),
        recovery: RecoveryShortcut::always_available(),
        safe_mode: request.safe_mode,
    }
}

const fn startup_action(request: TaskbarRequest) -> TaskbarAction {
    match (
        request.safe_mode.active,
        request.mutation,
        request.consent,
        request.policy,
    ) {
        (true, _, _, _)
        | (_, TaskbarMutation::DisabledForTest, _, _)
        | (_, _, Consent::Missing, _)
        | (_, _, _, TaskbarPolicy::Off) => TaskbarAction::LeaveUntouched,
        (false, TaskbarMutation::Enabled, Consent::Granted, TaskbarPolicy::AutoHide) => {
            TaskbarAction::Apply(ExplorerTaskbarState::AutoHide)
        }
        (false, TaskbarMutation::Enabled, Consent::Granted, TaskbarPolicy::Hide) => {
            TaskbarAction::Apply(ExplorerTaskbarState::Hidden)
        }
    }
}
