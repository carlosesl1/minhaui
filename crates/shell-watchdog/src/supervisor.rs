#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestartDelay {
    millis: u64,
}

impl RestartDelay {
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self { millis }
    }

    #[must_use]
    pub const fn millis(self) -> u64 {
        self.millis
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupervisorConfig {
    base_delay: RestartDelay,
    max_delay: RestartDelay,
    crash_loop_threshold: u32,
}

impl SupervisorConfig {
    #[must_use]
    pub const fn new(
        base_delay: RestartDelay,
        max_delay: RestartDelay,
        crash_loop_threshold: u32,
    ) -> Self {
        Self {
            base_delay,
            max_delay,
            crash_loop_threshold,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildEvent {
    HeartbeatReceived,
    HealthyWindowElapsed,
    HeartbeatMissed,
    ChildCrashed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorAction {
    Continue,
    Restart { delay: RestartDelay },
    EnterSafeMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChildCleanup {
    grace_period: RestartDelay,
}

impl ChildCleanup {
    #[must_use]
    pub const fn new(grace_period: RestartDelay) -> Self {
        Self { grace_period }
    }

    #[must_use]
    pub const fn action_after_shutdown(self, elapsed_ms: u64) -> SupervisorAction {
        if elapsed_ms > self.grace_period.millis {
            SupervisorAction::EnterSafeMode
        } else {
            SupervisorAction::Continue
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SupervisorState {
    restart_count: u32,
    safe_mode_active: bool,
}

impl SupervisorState {
    #[must_use]
    pub const fn safe_mode_active(self) -> bool {
        self.safe_mode_active
    }

    pub fn observe(&mut self, event: ChildEvent, config: SupervisorConfig) -> SupervisorAction {
        if self.safe_mode_active {
            return SupervisorAction::EnterSafeMode;
        }
        match event {
            ChildEvent::HeartbeatReceived => SupervisorAction::Continue,
            ChildEvent::HealthyWindowElapsed => {
                self.restart_count = 0;
                SupervisorAction::Continue
            }
            ChildEvent::HeartbeatMissed | ChildEvent::ChildCrashed => self.restart(config),
        }
    }

    fn restart(&mut self, config: SupervisorConfig) -> SupervisorAction {
        let next_count = self.restart_count.saturating_add(1);
        if next_count >= config.crash_loop_threshold {
            self.safe_mode_active = true;
            return SupervisorAction::EnterSafeMode;
        }
        self.restart_count = next_count;
        SupervisorAction::Restart {
            delay: restart_delay(config, next_count),
        }
    }
}

fn restart_delay(config: SupervisorConfig, count: u32) -> RestartDelay {
    let mut delay = config.base_delay.millis;
    let mut step = 1;
    while step < count {
        delay = delay.saturating_mul(2).min(config.max_delay.millis);
        step += 1;
    }
    RestartDelay::from_millis(delay)
}
