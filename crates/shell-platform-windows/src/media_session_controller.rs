use crate::media_session_types::{
    MediaSessionEntry, MediaSessionId, MediaSessionSnapshot, MediaTransportAction,
    MediaTransportResult, MediaWorkerCommand,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingMediaCommand {
    request: u64,
    session: MediaSessionId,
    action: MediaTransportAction,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MediaSessionController {
    snapshot: MediaSessionSnapshot,
    selected: Option<MediaSessionId>,
    manually_selected: bool,
    pending: Option<PendingMediaCommand>,
    next_request: u64,
    last_error: String,
}

impl MediaSessionController {
    pub(crate) fn apply_snapshot(&mut self, snapshot: MediaSessionSnapshot) -> bool {
        let had_player = self.selected_entry().is_some();
        self.snapshot = snapshot;
        let manual_is_live = self
            .selected
            .is_some_and(|id| self.snapshot.sessions.iter().any(|entry| entry.id == id));
        if !(self.manually_selected && manual_is_live) {
            self.manually_selected = false;
            self.selected = self
                .snapshot
                .current
                .filter(|id| self.snapshot.sessions.iter().any(|entry| entry.id == *id))
                .or_else(|| {
                    self.snapshot
                        .sessions
                        .iter()
                        .max_by_key(|entry| entry.activity_sequence)
                        .map(|entry| entry.id)
                });
        }
        if self.pending.is_some_and(|pending| {
            !self
                .snapshot
                .sessions
                .iter()
                .any(|entry| entry.id == pending.session)
        }) {
            self.pending = None;
        }
        self.last_error.clear();
        had_player != self.selected_entry().is_some()
    }

    pub(crate) fn select(&mut self, id: MediaSessionId) -> bool {
        if self.snapshot.sessions.iter().any(|entry| entry.id == id) {
            self.selected = Some(id);
            self.manually_selected = true;
            self.last_error.clear();
            true
        } else {
            false
        }
    }

    pub(crate) fn begin_transport(
        &mut self,
        action: MediaTransportAction,
    ) -> Option<MediaWorkerCommand> {
        if self.pending.is_some() {
            return None;
        }
        let entry = self.selected_entry()?;
        let enabled = match action {
            MediaTransportAction::Previous => entry.commands.previous,
            MediaTransportAction::TogglePlayback => entry.commands.toggle,
            MediaTransportAction::Next => entry.commands.next,
        };
        if !enabled {
            return None;
        }
        let session = entry.id;
        self.next_request = self.next_request.saturating_add(1).max(1);
        let pending = PendingMediaCommand {
            request: self.next_request,
            session,
            action,
        };
        self.pending = Some(pending);
        self.last_error.clear();
        Some(MediaWorkerCommand::Transport {
            request: pending.request,
            session: pending.session,
            action,
        })
    }

    pub(crate) fn complete_transport(&mut self, result: MediaTransportResult) -> bool {
        let Some(pending) = self.pending else {
            return false;
        };
        if pending.request != result.request
            || pending.session != result.session
            || pending.action != result.action
        {
            return false;
        }
        self.pending = None;
        self.last_error = result
            .result
            .err()
            .map_or_else(String::new, |error| error.to_string());
        true
    }

    pub(crate) fn selected_entry(&self) -> Option<&MediaSessionEntry> {
        let selected = self.selected?;
        self.snapshot
            .sessions
            .iter()
            .find(|entry| entry.id == selected)
    }

    pub(crate) fn sessions(&self) -> &[MediaSessionEntry] {
        &self.snapshot.sessions
    }

    pub(crate) const fn selected(&self) -> Option<MediaSessionId> {
        self.selected
    }

    pub(crate) fn pending_action(&self) -> Option<MediaTransportAction> {
        self.pending.map(|pending| pending.action)
    }

    pub(crate) fn last_error(&self) -> &str {
        &self.last_error
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::MediaSessionController;
    use crate::media_session_types::{
        MediaArtworkBytes, MediaCommandAvailability, MediaPlaybackState, MediaSessionEntry,
        MediaSessionError, MediaSessionId, MediaSessionSnapshot, MediaTransportAction,
        MediaTransportResult,
    };

    fn entry(id: u64, activity: u64, playback: MediaPlaybackState) -> MediaSessionEntry {
        MediaSessionEntry {
            id: MediaSessionId::new(id),
            source: format!("App {id}"),
            title: format!("Track {id}"),
            artist: "Artist".to_owned(),
            artwork: Some(MediaArtworkBytes {
                generation: id,
                encoded: Arc::from([1_u8, 2, 3]),
            }),
            playback,
            commands: MediaCommandAvailability {
                previous: true,
                toggle: true,
                next: true,
            },
            activity_sequence: activity,
        }
    }

    fn snapshot(current: Option<u64>, sessions: Vec<MediaSessionEntry>) -> MediaSessionSnapshot {
        MediaSessionSnapshot {
            generation: 1,
            sessions,
            current: current.map(MediaSessionId::new),
        }
    }

    #[test]
    fn manual_paused_selection_survives_other_session_updates() {
        let mut controller = MediaSessionController::default();
        controller.apply_snapshot(snapshot(
            Some(1),
            vec![
                entry(1, 1, MediaPlaybackState::Playing),
                entry(2, 2, MediaPlaybackState::Paused),
            ],
        ));
        assert!(controller.select(MediaSessionId::new(2)));

        controller.apply_snapshot(snapshot(
            Some(1),
            vec![
                entry(1, 9, MediaPlaybackState::Playing),
                entry(2, 2, MediaPlaybackState::Paused),
            ],
        ));

        assert_eq!(controller.selected(), Some(MediaSessionId::new(2)));
    }

    #[test]
    fn closed_manual_session_falls_back_to_most_recent_live_session() {
        let mut controller = MediaSessionController::default();
        controller.apply_snapshot(snapshot(
            None,
            vec![
                entry(1, 3, MediaPlaybackState::Paused),
                entry(2, 7, MediaPlaybackState::Paused),
            ],
        ));
        assert!(controller.select(MediaSessionId::new(1)));

        controller.apply_snapshot(snapshot(
            None,
            vec![entry(2, 7, MediaPlaybackState::Paused)],
        ));

        assert_eq!(controller.selected(), Some(MediaSessionId::new(2)));
    }

    #[test]
    fn transport_is_single_flight_and_ignores_stale_completion() {
        let mut controller = MediaSessionController::default();
        controller.apply_snapshot(snapshot(
            Some(1),
            vec![entry(1, 1, MediaPlaybackState::Playing)],
        ));
        let command = controller
            .begin_transport(MediaTransportAction::TogglePlayback)
            .expect("first command");
        assert!(
            controller
                .begin_transport(MediaTransportAction::Next)
                .is_none()
        );
        assert!(!controller.complete_transport(MediaTransportResult {
            request: 999,
            session: MediaSessionId::new(1),
            action: MediaTransportAction::TogglePlayback,
            result: Err(MediaSessionError::new("stale")),
        }));
        let crate::media_session_types::MediaWorkerCommand::Transport {
            request,
            session,
            action,
        } = command
        else {
            panic!("transport command")
        };
        assert!(controller.complete_transport(MediaTransportResult {
            request,
            session,
            action,
            result: Ok(()),
        }));
        assert_eq!(controller.pending_action(), None);
    }
}
