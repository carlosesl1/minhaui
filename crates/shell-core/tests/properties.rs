use proptest::prelude::*;
use shell_core::{DockItemId, ShellEvent, ShellState, reduce};

proptest! {
    #[test]
    fn arbitrary_event_sequences_never_break_state_invariants(
        events in prop::collection::vec(0_u8..=5, 0..128)
    ) {
        // Given: a valid default state and arbitrary harmless UI events.
        let mut state = ShellState::default();

        // When: every event is reduced in order.
        for event in events {
            let shell_event = match event {
                0 => ShellEvent::EnableAutohide(true),
                1 => ShellEvent::EnableAutohide(false),
                2 => ShellEvent::HideDock,
                3 => ShellEvent::RevealDock,
                4 => ShellEvent::DismissPopover,
                5 => ShellEvent::Unpin(DockItemId::new(99)),
                _ => ShellEvent::EnterSafeMode,
            };
            if let Ok(transition) = reduce(&state, shell_event) {
                state = transition.state;
            }
            // Then: every accepted intermediate state remains valid.
            prop_assert!(state.validate().is_ok());
        }
    }
}
