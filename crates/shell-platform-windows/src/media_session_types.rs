use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct MediaSessionId(u64);

impl MediaSessionId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaPlaybackState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MediaCommandAvailability {
    pub(crate) previous: bool,
    pub(crate) toggle: bool,
    pub(crate) next: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MediaArtworkBytes {
    pub(crate) generation: u64,
    pub(crate) encoded: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MediaSessionEntry {
    pub(crate) id: MediaSessionId,
    pub(crate) source: String,
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) artwork: Option<MediaArtworkBytes>,
    pub(crate) playback: MediaPlaybackState,
    pub(crate) commands: MediaCommandAvailability,
    pub(crate) activity_sequence: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MediaSessionSnapshot {
    pub(crate) generation: u64,
    pub(crate) sessions: Vec<MediaSessionEntry>,
    pub(crate) current: Option<MediaSessionId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaTransportAction {
    Previous,
    TogglePlayback,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaWorkerCommand {
    Refresh,
    Transport {
        request: u64,
        session: MediaSessionId,
        action: MediaTransportAction,
    },
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub(crate) struct MediaSessionError {
    message: String,
}

impl MediaSessionError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MediaTransportResult {
    pub(crate) request: u64,
    pub(crate) session: MediaSessionId,
    pub(crate) action: MediaTransportAction,
    pub(crate) result: Result<(), MediaSessionError>,
}
