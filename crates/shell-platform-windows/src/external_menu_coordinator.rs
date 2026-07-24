#![deny(unsafe_code)]

use std::time::{Duration, Instant};

use crate::{BackgroundAppId, TrayActivationId};

/// An unobserved native activation is given one bounded opportunity to
/// produce a menu popup before the caller opens its shell-owned fallback.
pub(crate) const EXTERNAL_MENU_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(1);

/// Custom application menus do not always expose a popup-end event.  Once a
/// popup has been observed, this watchdog releases the hold without sending
/// input to the application.
pub(crate) const EXTERNAL_MENU_WATCHDOG: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalMenuPhase {
    Idle,
    Armed,
    Observed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalMenuEffect {
    /// The row visual state or focus changed and the popover should redraw.
    Redraw,
    /// Consume the one deactivation generated while handing focus to the
    /// application-owned menu.
    SuppressDismissOnce,
    /// The native popup was observed for this exact activation token.
    ActivationObserved { activation: TrayActivationId },
    /// No popup was observed in time.  Task 7 owns the compatibility decision
    /// for this token; this effect only carries the typed fallback signal.
    ActivationTimedOut {
        app: BackgroundAppId,
        activation: TrayActivationId,
    },
    /// Restore keyboard focus to the stable application row after a menu ends.
    RestoreFocus(BackgroundAppId),
    /// Clear the external-active visual hold.
    Release,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MenuHold {
    app: BackgroundAppId,
    activation: TrayActivationId,
    owner_pid: u32,
    generation: u64,
    armed_at: Instant,
    observed_at: Option<Instant>,
    suppress_dismissal: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Idle,
    Armed(MenuHold),
    Observed(MenuHold),
}

/// Pure lifecycle state for one application-owned menu hand-off.
///
/// The coordinator deliberately accepts `Instant` values from its caller.
/// Native owner-window timers and WinEvent callbacks can therefore drive it
/// without this type creating threads, timers, or Win32 handles.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ExternalMenuCoordinator {
    state: State,
    current_generation: Option<u64>,
}

impl Default for ExternalMenuCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalMenuCoordinator {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            state: State::Idle,
            current_generation: None,
        }
    }

    #[must_use]
    pub(crate) const fn phase(&self) -> ExternalMenuPhase {
        match self.state {
            State::Idle => ExternalMenuPhase::Idle,
            State::Armed(_) => ExternalMenuPhase::Armed,
            State::Observed(_) => ExternalMenuPhase::Observed,
        }
    }

    #[must_use]
    pub(crate) const fn app(&self) -> Option<BackgroundAppId> {
        match self.state {
            State::Idle => None,
            State::Armed(hold) | State::Observed(hold) => Some(hold.app),
        }
    }

    #[must_use]
    pub(crate) const fn activation(&self) -> Option<TrayActivationId> {
        match self.state {
            State::Idle => None,
            State::Armed(hold) | State::Observed(hold) => Some(hold.activation),
        }
    }

    #[must_use]
    pub(crate) const fn owner_pid(&self) -> Option<u32> {
        match self.state {
            State::Idle => None,
            State::Armed(hold) | State::Observed(hold) => Some(hold.owner_pid),
        }
    }

    #[must_use]
    pub(crate) const fn generation(&self) -> Option<u64> {
        match self.state {
            State::Idle => None,
            State::Armed(hold) | State::Observed(hold) => Some(hold.generation),
        }
    }

    #[must_use]
    pub(crate) const fn current_generation(&self) -> Option<u64> {
        self.current_generation
    }

    /// Records the generation currently represented by the Apps popover.
    /// Changing generations releases an older hold before accepting events for
    /// the new popover.  This is the coordinator-side counterpart to a
    /// popover replacement or a fresh background-apps load.
    pub(crate) fn set_generation(&mut self, generation: u64) -> Vec<ExternalMenuEffect> {
        if self.current_generation == Some(generation) {
            return Vec::new();
        }
        let effects = self.release_active();
        self.current_generation = Some(generation);
        effects
    }

    /// Arms the hold only for the current observation generation.
    pub(crate) fn arm(
        &mut self,
        app: BackgroundAppId,
        activation: TrayActivationId,
        owner_pid: u32,
        generation: u64,
        now: Instant,
    ) -> Vec<ExternalMenuEffect> {
        if self.current_generation != Some(generation) {
            return Vec::new();
        }

        let mut effects = self.release_active();
        self.state = State::Armed(MenuHold {
            app,
            activation,
            owner_pid,
            generation,
            armed_at: now,
            observed_at: None,
            suppress_dismissal: true,
        });
        effects.push(ExternalMenuEffect::Redraw);
        effects
    }

    /// Observes a popup start only when owner, activation token, and
    /// observation generation all match the currently armed hold.
    pub(crate) fn popup_start(
        &mut self,
        owner_pid: u32,
        activation: TrayActivationId,
        generation: u64,
        now: Instant,
    ) -> Vec<ExternalMenuEffect> {
        self.observe_popup(owner_pid, activation, generation, now)
    }

    /// Observes an app-owned foreground window for this exact activation.
    ///
    /// Some applications render a custom top-level menu without emitting the
    /// standard accessibility popup events. The runtime only calls this from
    /// the active one-second observation timer after revalidating the
    /// foreground window's owner PID.
    pub(crate) fn foreground_window_observed(
        &mut self,
        owner_pid: u32,
        activation: TrayActivationId,
        generation: u64,
        now: Instant,
    ) -> Vec<ExternalMenuEffect> {
        self.observe_popup(owner_pid, activation, generation, now)
    }

    fn observe_popup(
        &mut self,
        owner_pid: u32,
        activation: TrayActivationId,
        generation: u64,
        now: Instant,
    ) -> Vec<ExternalMenuEffect> {
        let State::Armed(mut hold) = self.state else {
            return Vec::new();
        };
        if !matches_hold(hold, owner_pid, activation, generation) {
            return Vec::new();
        }
        hold.observed_at = Some(now);
        self.state = State::Observed(hold);
        vec![
            ExternalMenuEffect::ActivationObserved {
                activation: hold.activation,
            },
            ExternalMenuEffect::Redraw,
        ]
    }

    /// A popup end releases only the hold that owns this process/generation.
    pub(crate) fn popup_end(
        &mut self,
        owner_pid: u32,
        activation: TrayActivationId,
        generation: u64,
        now: Instant,
    ) -> Vec<ExternalMenuEffect> {
        self.end_popup(owner_pid, activation, generation, now)
    }

    fn end_popup(
        &mut self,
        owner_pid: u32,
        activation: TrayActivationId,
        generation: u64,
        _now: Instant,
    ) -> Vec<ExternalMenuEffect> {
        let hold = match self.state {
            State::Armed(hold) | State::Observed(hold)
                if matches_hold(hold, owner_pid, activation, generation) =>
            {
                hold
            }
            State::Idle | State::Armed(_) | State::Observed(_) => return Vec::new(),
        };
        self.state = State::Idle;
        terminal_effects(hold.app)
    }

    /// Consumes exactly one `WA_INACTIVE` dismissal allowance.  The hold is
    /// retained; only the allowance is consumed so a later deactivation can
    /// dismiss the shell popover normally.
    pub(crate) fn on_wa_inactive(&mut self) -> Vec<ExternalMenuEffect> {
        let hold = match self.state {
            State::Armed(hold) | State::Observed(hold) if hold.suppress_dismissal => hold,
            State::Idle | State::Armed(_) | State::Observed(_) => return Vec::new(),
        };
        let hold = MenuHold {
            suppress_dismissal: false,
            ..hold
        };
        self.state = match hold.observed_at {
            Some(_) => State::Observed(hold),
            None => State::Armed(hold),
        };
        vec![ExternalMenuEffect::SuppressDismissOnce]
    }

    /// Releases an unobserved activation at one second, or an observed custom
    /// menu at the 30-second watchdog deadline.
    pub(crate) fn tick(&mut self, now: Instant) -> Vec<ExternalMenuEffect> {
        match self.state {
            State::Armed(hold)
                if now.saturating_duration_since(hold.armed_at)
                    >= EXTERNAL_MENU_OBSERVATION_TIMEOUT =>
            {
                self.state = State::Idle;
                vec![
                    ExternalMenuEffect::ActivationTimedOut {
                        app: hold.app,
                        activation: hold.activation,
                    },
                    ExternalMenuEffect::RestoreFocus(hold.app),
                    ExternalMenuEffect::Release,
                    ExternalMenuEffect::Redraw,
                ]
            }
            State::Observed(hold)
                if hold.observed_at.is_some_and(|observed_at| {
                    now.saturating_duration_since(observed_at) >= EXTERNAL_MENU_WATCHDOG
                }) =>
            {
                self.state = State::Idle;
                terminal_effects(hold.app)
            }
            State::Idle | State::Armed(_) | State::Observed(_) => Vec::new(),
        }
    }

    pub(crate) fn owner_exit(&mut self, owner_pid: u32) -> Vec<ExternalMenuEffect> {
        self.release_if(|hold| hold.owner_pid == owner_pid)
    }

    pub(crate) fn explorer_restart(&mut self) -> Vec<ExternalMenuEffect> {
        self.release_active()
    }

    pub(crate) fn escape(&mut self) -> Vec<ExternalMenuEffect> {
        self.release_active()
    }

    pub(crate) fn popover_replaced(&mut self) -> Vec<ExternalMenuEffect> {
        self.release_active()
    }

    pub(crate) fn shutdown(&mut self) -> Vec<ExternalMenuEffect> {
        self.release_active()
    }

    fn release_if(&mut self, predicate: impl Fn(MenuHold) -> bool) -> Vec<ExternalMenuEffect> {
        let hold = match self.state {
            State::Armed(hold) | State::Observed(hold) if predicate(hold) => hold,
            State::Idle | State::Armed(_) | State::Observed(_) => return Vec::new(),
        };
        self.state = State::Idle;
        terminal_effects(hold.app)
    }

    fn release_active(&mut self) -> Vec<ExternalMenuEffect> {
        let hold = match self.state {
            State::Armed(hold) | State::Observed(hold) => hold,
            State::Idle => return Vec::new(),
        };
        self.state = State::Idle;
        terminal_effects(hold.app)
    }
}

fn matches_hold(
    hold: MenuHold,
    owner_pid: u32,
    activation: TrayActivationId,
    generation: u64,
) -> bool {
    hold.owner_pid == owner_pid && hold.generation == generation && hold.activation == activation
}

fn terminal_effects(app: BackgroundAppId) -> Vec<ExternalMenuEffect> {
    vec![
        ExternalMenuEffect::RestoreFocus(app),
        ExternalMenuEffect::Release,
        ExternalMenuEffect::Redraw,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        NativeWindowId, TrayActivationCoordinator, TrayActivationRequest, TrayCallbackSink,
    };

    #[derive(Default)]
    struct RecordingSink;

    impl TrayCallbackSink for RecordingSink {
        fn post(&mut self, _target: NativeWindowId, _message: crate::TrayMessage) -> bool {
            true
        }
    }

    fn at(base: Instant, seconds: u64) -> Instant {
        base + Duration::from_secs(seconds)
    }

    fn id(value: u64) -> TrayActivationId {
        let mut activations = TrayActivationCoordinator::new();
        let mut sink = RecordingSink;
        let mut result = None;
        for _ in 0..value {
            let activation = activations.begin(
                TrayActivationRequest::new(
                    "example.exe",
                    NativeWindowId::new(1),
                    1,
                    0x8001,
                    4,
                    (0, 0),
                ),
                &mut sink,
            );
            let id = activation.id().expect("test activation id");
            result = Some(id);
            activations.cancel(id);
        }
        result.expect("non-zero test activation id")
    }

    const APP: BackgroundAppId = BackgroundAppId::new(7);
    const OWNER: u32 = 41;
    const GENERATION: u64 = 9;

    #[test]
    fn arm_requires_current_generation_and_keeps_origin_row() {
        let base = Instant::now();
        let mut coordinator = ExternalMenuCoordinator::new();
        coordinator.set_generation(GENERATION);
        assert!(
            coordinator
                .arm(APP, id(1), OWNER, GENERATION - 1, at(base, 0))
                .is_empty()
        );
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Idle);

        let effects = coordinator.arm(APP, id(1), OWNER, GENERATION, at(base, 0));
        assert_eq!(effects, vec![ExternalMenuEffect::Redraw]);
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Armed);
        assert_eq!(coordinator.app(), Some(APP));
    }

    #[test]
    fn matching_popup_start_observes_token_and_stale_owner_or_token_is_ignored() {
        let base = Instant::now();
        let mut coordinator = ExternalMenuCoordinator::new();
        coordinator.set_generation(GENERATION);
        coordinator.arm(APP, id(2), OWNER, GENERATION, at(base, 0));

        assert!(
            coordinator
                .popup_start(OWNER + 1, id(2), GENERATION, at(base, 1))
                .is_empty()
        );
        assert!(
            coordinator
                .popup_start(OWNER, id(1), GENERATION, at(base, 1))
                .is_empty()
        );
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Armed);

        assert_eq!(
            coordinator.popup_start(OWNER, id(2), GENERATION, at(base, 1)),
            vec![
                ExternalMenuEffect::ActivationObserved { activation: id(2) },
                ExternalMenuEffect::Redraw,
            ]
        );
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Observed);
    }

    #[test]
    fn matching_foreground_window_observes_only_the_armed_generation() {
        let base = Instant::now();
        let mut coordinator = ExternalMenuCoordinator::new();
        coordinator.set_generation(GENERATION);
        coordinator.arm(APP, id(21), OWNER, GENERATION, at(base, 0));

        assert!(
            coordinator
                .foreground_window_observed(OWNER + 1, id(21), GENERATION, at(base, 1))
                .is_empty()
        );
        assert_eq!(
            coordinator.foreground_window_observed(OWNER, id(21), GENERATION, at(base, 1)),
            vec![
                ExternalMenuEffect::ActivationObserved { activation: id(21) },
                ExternalMenuEffect::Redraw,
            ]
        );
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Observed);
    }

    #[test]
    fn popup_end_releases_and_restores_stable_row_focus() {
        let base = Instant::now();
        let mut coordinator = ExternalMenuCoordinator::new();
        coordinator.set_generation(GENERATION);
        coordinator.arm(APP, id(3), OWNER, GENERATION, at(base, 0));
        coordinator.popup_start(OWNER, id(3), GENERATION, at(base, 1));

        assert_eq!(
            coordinator.popup_end(OWNER, id(3), GENERATION, at(base, 2)),
            vec![
                ExternalMenuEffect::RestoreFocus(APP),
                ExternalMenuEffect::Release,
                ExternalMenuEffect::Redraw,
            ]
        );
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Idle);
        assert!(
            coordinator
                .popup_end(OWNER, id(3), GENERATION, at(base, 3))
                .is_empty()
        );
    }

    #[test]
    fn only_one_wa_inactive_is_suppressed_while_hold_is_live() {
        let base = Instant::now();
        let mut coordinator = ExternalMenuCoordinator::new();
        coordinator.set_generation(GENERATION);
        coordinator.arm(APP, id(4), OWNER, GENERATION, at(base, 0));
        assert_eq!(
            coordinator.on_wa_inactive(),
            vec![ExternalMenuEffect::SuppressDismissOnce]
        );
        assert!(coordinator.on_wa_inactive().is_empty());
        coordinator.popup_start(OWNER, id(4), GENERATION, at(base, 1));
        assert!(coordinator.on_wa_inactive().is_empty());
        coordinator.popup_end(OWNER, id(4), GENERATION, at(base, 2));
        assert!(coordinator.on_wa_inactive().is_empty());
    }

    #[test]
    fn unobserved_timeout_carries_app_and_activation_then_clears_state() {
        let base = Instant::now();
        let mut coordinator = ExternalMenuCoordinator::new();
        coordinator.set_generation(GENERATION);
        coordinator.arm(APP, id(5), OWNER, GENERATION, at(base, 0));

        assert!(
            coordinator
                .tick(at(base, 0) + EXTERNAL_MENU_OBSERVATION_TIMEOUT - Duration::from_millis(1))
                .is_empty()
        );
        assert_eq!(
            coordinator.tick(at(base, 1)),
            vec![
                ExternalMenuEffect::ActivationTimedOut {
                    app: APP,
                    activation: id(5),
                },
                ExternalMenuEffect::RestoreFocus(APP),
                ExternalMenuEffect::Release,
                ExternalMenuEffect::Redraw,
            ]
        );
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Idle);
    }

    #[test]
    fn observed_watchdog_releases_without_input() {
        let base = Instant::now();
        let mut coordinator = ExternalMenuCoordinator::new();
        coordinator.set_generation(GENERATION);
        coordinator.arm(APP, id(6), OWNER, GENERATION, at(base, 0));
        coordinator.popup_start(OWNER, id(6), GENERATION, at(base, 1));
        assert!(
            coordinator
                .tick(at(base, 1) + EXTERNAL_MENU_WATCHDOG - Duration::from_millis(1))
                .is_empty()
        );
        assert_eq!(
            coordinator.tick(at(base, 31)),
            vec![
                ExternalMenuEffect::RestoreFocus(APP),
                ExternalMenuEffect::Release,
                ExternalMenuEffect::Redraw,
            ]
        );
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Idle);
    }

    #[test]
    fn terminal_events_release_only_matching_owner() {
        let base = Instant::now();
        let mut coordinator = ExternalMenuCoordinator::new();
        coordinator.set_generation(GENERATION);
        coordinator.arm(APP, id(7), OWNER, GENERATION, at(base, 0));
        assert!(coordinator.owner_exit(OWNER + 1).is_empty());
        assert_eq!(coordinator.owner_pid(), Some(OWNER));

        assert!(!coordinator.escape().is_empty());
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Idle);

        coordinator.set_generation(GENERATION);
        coordinator.arm(APP, id(8), OWNER, GENERATION, at(base, 0));
        assert!(!coordinator.explorer_restart().is_empty());
        assert_eq!(coordinator.phase(), ExternalMenuPhase::Idle);
    }

    #[test]
    fn stale_events_cannot_mutate_newer_state() {
        let base = Instant::now();
        let mut coordinator = ExternalMenuCoordinator::new();
        coordinator.set_generation(GENERATION);
        coordinator.arm(APP, id(9), OWNER, GENERATION, at(base, 0));
        coordinator.escape();
        coordinator.set_generation(GENERATION + 1);
        coordinator.arm(APP, id(10), OWNER + 1, GENERATION + 1, at(base, 2));

        assert!(
            coordinator
                .popup_start(OWNER, id(9), GENERATION, at(base, 3))
                .is_empty()
        );
        assert_eq!(coordinator.activation(), Some(id(10)));
        assert_eq!(coordinator.owner_pid(), Some(OWNER + 1));
    }
}
