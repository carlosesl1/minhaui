use std::collections::HashMap;
use std::sync::Arc;

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Storage::Streams::DataReader;
use windows::core::{HSTRING, Result};

use crate::media_session_types::{
    MediaArtworkBytes, MediaCommandAvailability, MediaPlaybackState, MediaSessionEntry,
    MediaSessionError, MediaSessionId, MediaSessionSnapshot, MediaTransportAction,
};

const MAX_ARTWORK_BYTES: u64 = 4 * 1024 * 1024;

pub(super) fn request_manager()
-> std::result::Result<GlobalSystemMediaTransportControlsSessionManager, MediaSessionError> {
    GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .and_then(|operation| operation.join())
        .map_err(media_error)
}

pub(super) fn session_id(
    session: &GlobalSystemMediaTransportControlsSession,
) -> std::result::Result<MediaSessionId, MediaSessionError> {
    session
        .SourceAppUserModelId()
        .map(|source| MediaSessionId::new(stable_hash(source.to_string().as_bytes())))
        .map_err(media_error)
}

pub(super) fn capture_snapshot(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
    generation: u64,
    activity: &HashMap<MediaSessionId, u64>,
) -> std::result::Result<MediaSessionSnapshot, MediaSessionError> {
    let sessions = manager.GetSessions().map_err(media_error)?;
    let current = manager
        .GetCurrentSession()
        .ok()
        .and_then(|session| session_id(&session).ok());
    let mut entries = Vec::with_capacity(sessions.Size().map_err(media_error)? as usize);
    for session in &sessions {
        let id = session_id(&session)?;
        entries.push(capture_entry(
            &session,
            id,
            activity.get(&id).copied().unwrap_or_default(),
        )?);
    }
    Ok(MediaSessionSnapshot {
        generation,
        sessions: entries,
        current,
    })
}

pub(super) fn apply_transport(
    session: &GlobalSystemMediaTransportControlsSession,
    action: MediaTransportAction,
) -> std::result::Result<(), MediaSessionError> {
    let accepted = match action {
        MediaTransportAction::Previous => session
            .TrySkipPreviousAsync()
            .and_then(|operation| operation.join()),
        MediaTransportAction::Next => session
            .TrySkipNextAsync()
            .and_then(|operation| operation.join()),
        MediaTransportAction::TogglePlayback => {
            let playing = session
                .GetPlaybackInfo()
                .and_then(|info| info.PlaybackStatus())
                .map(|status| {
                    status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
                })
                .map_err(media_error)?;
            if playing {
                session
                    .TryPauseAsync()
                    .and_then(|operation| operation.join())
            } else {
                session
                    .TryPlayAsync()
                    .and_then(|operation| operation.join())
            }
        }
    }
    .map_err(media_error)?;
    if accepted {
        Ok(())
    } else {
        Err(MediaSessionError::new(
            "The media app did not accept this command",
        ))
    }
}

fn capture_entry(
    session: &GlobalSystemMediaTransportControlsSession,
    id: MediaSessionId,
    activity_sequence: u64,
) -> std::result::Result<MediaSessionEntry, MediaSessionError> {
    let source_id = session.SourceAppUserModelId().map_err(media_error)?;
    let properties = session
        .TryGetMediaPropertiesAsync()
        .and_then(|operation| operation.join())
        .map_err(media_error)?;
    let playback = session.GetPlaybackInfo().map_err(media_error)?;
    let controls = playback.Controls().map_err(media_error)?;
    let status = playback.PlaybackStatus().map_err(media_error)?;
    let artwork = properties
        .Thumbnail()
        .and_then(|reference| read_artwork(&reference))
        .ok()
        .flatten();
    Ok(MediaSessionEntry {
        id,
        source: friendly_source(&source_id),
        title: fallback(
            properties.Title().map_err(media_error)?.to_string(),
            &source_id,
        ),
        artist: properties.Artist().map_err(media_error)?.to_string(),
        artwork,
        playback: playback_state(status),
        commands: MediaCommandAvailability {
            previous: controls.IsPreviousEnabled().unwrap_or(false),
            toggle: controls.IsPlayEnabled().unwrap_or(false)
                || controls.IsPauseEnabled().unwrap_or(false),
            next: controls.IsNextEnabled().unwrap_or(false),
        },
        activity_sequence,
    })
}

fn read_artwork(
    reference: &windows::Storage::Streams::IRandomAccessStreamReference,
) -> Result<Option<MediaArtworkBytes>> {
    let stream = reference.OpenReadAsync()?.join()?;
    let size = stream.Size()?;
    if size == 0 || size > MAX_ARTWORK_BYTES || size > u64::from(u32::MAX) {
        return Ok(None);
    }
    let input = stream.GetInputStreamAt(0)?;
    let reader = DataReader::CreateDataReader(&input)?;
    let loaded = reader.LoadAsync(size as u32)?.join()?;
    if u64::from(loaded) != size {
        return Ok(None);
    }
    let mut encoded = vec![0_u8; size as usize];
    reader.ReadBytes(&mut encoded)?;
    let generation = stable_hash(&encoded);
    Ok(Some(MediaArtworkBytes {
        generation,
        encoded: Arc::from(encoded),
    }))
}

fn playback_state(
    status: GlobalSystemMediaTransportControlsSessionPlaybackStatus,
) -> MediaPlaybackState {
    if status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
        MediaPlaybackState::Playing
    } else if status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused {
        MediaPlaybackState::Paused
    } else {
        MediaPlaybackState::Stopped
    }
}

fn fallback(title: String, source: &HSTRING) -> String {
    if title.trim().is_empty() {
        friendly_source(source)
    } else {
        title
    }
}

fn friendly_source(source: &HSTRING) -> String {
    let raw = source.to_string();
    raw.rsplit(['!', '.'])
        .find(|part| !part.trim().is_empty())
        .unwrap_or("Media")
        .to_owned()
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn media_error(error: windows::core::Error) -> MediaSessionError {
    MediaSessionError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::stable_hash;

    #[test]
    fn media_identity_hash_is_stable_and_sensitive() {
        assert_eq!(stable_hash(b"spotify"), stable_hash(b"spotify"));
        assert_ne!(stable_hash(b"spotify"), stable_hash(b"vlc"));
    }
}
