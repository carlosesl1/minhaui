use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager, MediaPropertiesChangedEventArgs,
    PlaybackInfoChangedEventArgs, SessionsChangedEventArgs, TimelinePropertiesChangedEventArgs,
};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::media_session_types::{
    MediaSessionError, MediaSessionId, MediaTransportResult, MediaWorkerCommand,
};
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event};
use crate::{NativeWindowId, PlatformEvent};

pub(super) const MEDIA_SESSION_WAKE_MESSAGE: u32 = WM_APP + 0x63;

enum WorkerSignal {
    Command(MediaWorkerCommand),
    Refresh(Option<MediaSessionId>),
}

pub(super) struct MediaSessionWorker {
    sender: Sender<WorkerSignal>,
    thread: Option<JoinHandle<()>>,
}

impl MediaSessionWorker {
    pub(super) fn start(wake_window: NativeWindowId) -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker_sender = sender.clone();
        let thread = std::thread::spawn(move || run_worker(receiver, worker_sender, wake_window));
        Self {
            sender,
            thread: Some(thread),
        }
    }

    pub(super) fn send(&self, command: MediaWorkerCommand) {
        let _ = self.sender.send(WorkerSignal::Command(command));
    }
}

impl Drop for MediaSessionWorker {
    fn drop(&mut self) {
        let _ = self
            .sender
            .send(WorkerSignal::Command(MediaWorkerCommand::Stop));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct SessionSubscription {
    session: GlobalSystemMediaTransportControlsSession,
    media: i64,
    playback: i64,
    timeline: i64,
}

impl Drop for SessionSubscription {
    fn drop(&mut self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.media);
        let _ = self.session.RemovePlaybackInfoChanged(self.playback);
        let _ = self.session.RemoveTimelinePropertiesChanged(self.timeline);
    }
}

fn run_worker(
    receiver: Receiver<WorkerSignal>,
    sender: Sender<WorkerSignal>,
    wake_window: NativeWindowId,
) {
    let _apartment = match WinRtApartment::initialize() {
        Ok(apartment) => apartment,
        Err(error) => {
            publish_snapshot(Err(MediaSessionError::new(error.to_string())), wake_window);
            return;
        }
    };
    let manager = match crate::win32_media_sessions::request_manager() {
        Ok(manager) => manager,
        Err(error) => {
            publish_snapshot(Err(error), wake_window);
            return;
        }
    };
    let refresh_pending = Arc::new(AtomicBool::new(false));
    let manager_tokens = match subscribe_manager(&manager, &sender, &refresh_pending) {
        Ok(tokens) => tokens,
        Err(error) => {
            publish_snapshot(Err(error), wake_window);
            return;
        }
    };
    let mut subscriptions = HashMap::new();
    let mut activity = HashMap::new();
    let mut sequence = 0_u64;
    let mut generation = 0_u64;
    let _ = sender.send(WorkerSignal::Refresh(None));

    while let Ok(signal) = receiver.recv() {
        match signal {
            WorkerSignal::Command(MediaWorkerCommand::Stop) => break,
            WorkerSignal::Command(MediaWorkerCommand::Refresh) | WorkerSignal::Refresh(None) => {
                refresh_pending.store(false, Ordering::Release);
                if let Ok(session) = manager.GetCurrentSession() {
                    if let Ok(current) = crate::win32_media_sessions::session_id(&session) {
                        sequence = sequence.saturating_add(1);
                        activity.insert(current, sequence);
                    }
                }
                publish_refreshed(
                    &manager,
                    &sender,
                    &refresh_pending,
                    &mut subscriptions,
                    &activity,
                    &mut generation,
                    wake_window,
                );
            }
            WorkerSignal::Refresh(Some(id)) => {
                refresh_pending.store(false, Ordering::Release);
                sequence = sequence.saturating_add(1);
                activity.insert(id, sequence);
                publish_refreshed(
                    &manager,
                    &sender,
                    &refresh_pending,
                    &mut subscriptions,
                    &activity,
                    &mut generation,
                    wake_window,
                );
            }
            WorkerSignal::Command(MediaWorkerCommand::Transport {
                request,
                session,
                action,
            }) => {
                let result = subscriptions
                    .get(&session)
                    .ok_or_else(|| MediaSessionError::new("The selected media session ended"))
                    .and_then(|subscription| {
                        crate::win32_media_sessions::apply_transport(&subscription.session, action)
                    });
                queue_event(RoutedPlatformEvent::window_id(
                    wake_window,
                    PlatformEvent::MediaTransportCompleted(MediaTransportResult {
                        request,
                        session,
                        action,
                        result,
                    }),
                ));
                wake_owner_window(wake_window);
            }
        }
    }
    let _ = manager.RemoveCurrentSessionChanged(manager_tokens.0);
    let _ = manager.RemoveSessionsChanged(manager_tokens.1);
}

fn publish_refreshed(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
    sender: &Sender<WorkerSignal>,
    refresh_pending: &Arc<AtomicBool>,
    subscriptions: &mut HashMap<MediaSessionId, SessionSubscription>,
    activity: &HashMap<MediaSessionId, u64>,
    generation: &mut u64,
    wake_window: NativeWindowId,
) {
    let result =
        sync_subscriptions(manager, sender, refresh_pending, subscriptions).and_then(|()| {
            *generation = generation.saturating_add(1);
            crate::win32_media_sessions::capture_snapshot(manager, *generation, activity)
        });
    publish_snapshot(result, wake_window);
}

fn sync_subscriptions(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
    sender: &Sender<WorkerSignal>,
    refresh_pending: &Arc<AtomicBool>,
    subscriptions: &mut HashMap<MediaSessionId, SessionSubscription>,
) -> Result<(), MediaSessionError> {
    let sessions = manager
        .GetSessions()
        .map_err(|error| MediaSessionError::new(error.to_string()))?;
    let mut live = Vec::new();
    for session in &sessions {
        let id = crate::win32_media_sessions::session_id(&session)?;
        live.push(id);
        if let std::collections::hash_map::Entry::Vacant(entry) = subscriptions.entry(id) {
            entry.insert(subscribe_session(session, id, sender, refresh_pending)?);
        }
    }
    subscriptions.retain(|id, _| live.contains(id));
    Ok(())
}

fn subscribe_manager(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
    sender: &Sender<WorkerSignal>,
    refresh_pending: &Arc<AtomicBool>,
) -> Result<(i64, i64), MediaSessionError> {
    let current_sender = sender.clone();
    let current_pending = Arc::clone(refresh_pending);
    let current = manager
        .CurrentSessionChanged(&TypedEventHandler::<
            GlobalSystemMediaTransportControlsSessionManager,
            CurrentSessionChangedEventArgs,
        >::new(move |_, _| {
            request_refresh(&current_sender, &current_pending, None);
            Ok(())
        }))
        .map_err(|error| MediaSessionError::new(error.to_string()))?;
    let sessions_sender = sender.clone();
    let sessions_pending = Arc::clone(refresh_pending);
    let sessions = manager
        .SessionsChanged(&TypedEventHandler::<
            GlobalSystemMediaTransportControlsSessionManager,
            SessionsChangedEventArgs,
        >::new(move |_, _| {
            request_refresh(&sessions_sender, &sessions_pending, None);
            Ok(())
        }))
        .map_err(|error| MediaSessionError::new(error.to_string()))?;
    Ok((current, sessions))
}

fn subscribe_session(
    session: GlobalSystemMediaTransportControlsSession,
    id: MediaSessionId,
    sender: &Sender<WorkerSignal>,
    refresh_pending: &Arc<AtomicBool>,
) -> Result<SessionSubscription, MediaSessionError> {
    let media_sender = sender.clone();
    let media_pending = Arc::clone(refresh_pending);
    let media = session
        .MediaPropertiesChanged(&TypedEventHandler::<
            GlobalSystemMediaTransportControlsSession,
            MediaPropertiesChangedEventArgs,
        >::new(move |_, _| {
            request_refresh(&media_sender, &media_pending, Some(id));
            Ok(())
        }))
        .map_err(|error| MediaSessionError::new(error.to_string()))?;
    let playback_sender = sender.clone();
    let playback_pending = Arc::clone(refresh_pending);
    let playback = session
        .PlaybackInfoChanged(&TypedEventHandler::<
            GlobalSystemMediaTransportControlsSession,
            PlaybackInfoChangedEventArgs,
        >::new(move |_, _| {
            request_refresh(&playback_sender, &playback_pending, Some(id));
            Ok(())
        }))
        .map_err(|error| MediaSessionError::new(error.to_string()))?;
    let timeline_sender = sender.clone();
    let timeline_pending = Arc::clone(refresh_pending);
    let timeline = session
        .TimelinePropertiesChanged(&TypedEventHandler::<
            GlobalSystemMediaTransportControlsSession,
            TimelinePropertiesChangedEventArgs,
        >::new(move |_, _| {
            request_refresh(&timeline_sender, &timeline_pending, Some(id));
            Ok(())
        }))
        .map_err(|error| MediaSessionError::new(error.to_string()))?;
    Ok(SessionSubscription {
        session,
        media,
        playback,
        timeline,
    })
}

fn request_refresh(
    sender: &Sender<WorkerSignal>,
    pending: &AtomicBool,
    session: Option<MediaSessionId>,
) {
    if !pending.swap(true, Ordering::AcqRel) {
        let _ = sender.send(WorkerSignal::Refresh(session));
    }
}

fn publish_snapshot(
    result: Result<crate::media_session_types::MediaSessionSnapshot, MediaSessionError>,
    wake_window: NativeWindowId,
) {
    queue_event(RoutedPlatformEvent::window_id(
        wake_window,
        PlatformEvent::MediaSessionsChanged(result),
    ));
    wake_owner_window(wake_window);
}

fn wake_owner_window(window: NativeWindowId) {
    let hwnd = HWND(window.value() as *mut core::ffi::c_void);
    // SAFETY: the copied HWND identifies the UI-thread window and no pointer payload crosses threads.
    let _ = unsafe { PostMessageW(Some(hwnd), MEDIA_SESSION_WAKE_MESSAGE, WPARAM(0), LPARAM(0)) };
}

struct WinRtApartment;

impl WinRtApartment {
    fn initialize() -> windows::core::Result<Self> {
        // SAFETY: initializes WinRT exactly once for this newly spawned worker thread.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
        Ok(Self)
    }
}

impl Drop for WinRtApartment {
    fn drop(&mut self) {
        // SAFETY: balances the successful RoInitialize call on the same worker thread.
        unsafe { RoUninitialize() };
    }
}
