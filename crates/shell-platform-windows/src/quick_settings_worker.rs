use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::latest_request_worker::LatestRequestWorker;
use crate::{
    AudioPanelSnapshot, NativeWindowId, QuickControlCapability, QuickSettingsCapabilities,
    QuickSettingsIntent, SystemRoute,
};

#[cfg(windows)]
use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

#[cfg(windows)]
use crate::PlatformEvent;
#[cfg(windows)]
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event_with_wake};

#[cfg(windows)]
pub(super) const QUICK_SETTINGS_WAKE_MESSAGE: u32 = WM_APP + 0x68;

pub(crate) trait QuickSettingsSource {
    fn worker_started(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn read_capabilities(&mut self) -> QuickSettingsCapabilities;
    fn read_audio(&mut self) -> Result<AudioPanelSnapshot, String>;
    fn apply_intent(
        &mut self,
        _intent: &QuickSettingsIntent,
        _capability: Option<&QuickControlCapability>,
    ) -> QuickSettingsWorkerAction {
        QuickSettingsWorkerAction::NoChange
    }
}

#[derive(Default)]
struct NativeQuickSettingsSource {
    #[cfg(windows)]
    apartment: Option<WorkerComApartment>,
}

impl QuickSettingsSource for NativeQuickSettingsSource {
    fn worker_started(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.apartment =
                Some(WorkerComApartment::initialize().map_err(|error| error.to_string())?);
        }
        Ok(())
    }

    fn read_capabilities(&mut self) -> QuickSettingsCapabilities {
        crate::win32_quick_settings_capabilities::read_capabilities()
    }

    fn read_audio(&mut self) -> Result<AudioPanelSnapshot, String> {
        crate::win32_audio_panel::read().map_err(|error| error.to_string())
    }

    fn apply_intent(
        &mut self,
        intent: &QuickSettingsIntent,
        capability: Option<&QuickControlCapability>,
    ) -> QuickSettingsWorkerAction {
        crate::win32_quick_settings_actions::apply_quick_settings_intent(intent, capability).into()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum QuickSettingsRequestKind {
    Refresh {
        include_audio: bool,
    },
    Action {
        intent: QuickSettingsIntent,
        capability: Option<QuickControlCapability>,
    },
}

pub(crate) const fn runs_on_worker(intent: &QuickSettingsIntent) -> bool {
    matches!(
        intent,
        QuickSettingsIntent::SetDoNotDisturbMode(_)
            | QuickSettingsIntent::SetAudioSessionVolume { .. }
            | QuickSettingsIntent::SelectAudioOutput(_)
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum QuickSettingsWorkerAction {
    Applied(QuickControlCapability),
    AudioChanged(AudioPanelSnapshot),
    OpenSystemRoute(SystemRoute),
    NoChange,
    Failed(String),
}

impl From<crate::win32_quick_settings_actions::QuickSettingsActionResult>
    for QuickSettingsWorkerAction
{
    fn from(value: crate::win32_quick_settings_actions::QuickSettingsActionResult) -> Self {
        match value {
            crate::win32_quick_settings_actions::QuickSettingsActionResult::Applied(control) => {
                Self::Applied(control)
            }
            crate::win32_quick_settings_actions::QuickSettingsActionResult::AudioChanged(
                snapshot,
            ) => Self::AudioChanged(snapshot),
            crate::win32_quick_settings_actions::QuickSettingsActionResult::OpenSystemRoute(
                route,
            ) => Self::OpenSystemRoute(route),
            crate::win32_quick_settings_actions::QuickSettingsActionResult::NoChange => {
                Self::NoChange
            }
            crate::win32_quick_settings_actions::QuickSettingsActionResult::Failed(error) => {
                Self::Failed(error.to_string())
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum QuickSettingsWorkerPayload {
    Refreshed {
        capabilities: QuickSettingsCapabilities,
        audio: Option<AudioPanelSnapshot>,
    },
    Action(QuickSettingsWorkerAction),
}

pub(crate) fn execute_request(
    source: &mut impl QuickSettingsSource,
    request: QuickSettingsRequestKind,
) -> QuickSettingsWorkerPayload {
    match request {
        QuickSettingsRequestKind::Refresh { include_audio } => {
            let audio = include_audio.then(|| source.read_audio().ok()).flatten();
            QuickSettingsWorkerPayload::Refreshed {
                capabilities: source.read_capabilities(),
                audio,
            }
        }
        QuickSettingsRequestKind::Action { intent, capability } => {
            QuickSettingsWorkerPayload::Action(source.apply_intent(&intent, capability.as_ref()))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuickSettingsWorkerResult {
    generation: u64,
    payload: QuickSettingsWorkerPayload,
}

impl QuickSettingsWorkerResult {
    fn new(generation: u64, payload: QuickSettingsWorkerPayload) -> Self {
        Self {
            generation,
            payload,
        }
    }

    pub(crate) fn into_parts(self) -> (u64, QuickSettingsWorkerPayload) {
        (self.generation, self.payload)
    }

    const fn is_refresh(&self) -> bool {
        matches!(&self.payload, QuickSettingsWorkerPayload::Refreshed { .. })
    }
}

#[derive(Clone)]
struct QuickSettingsRequest {
    generation: u64,
    wake_window: NativeWindowId,
    kind: QuickSettingsRequestKind,
}

pub(crate) struct QuickSettingsWorker {
    generation: AtomicU64,
    worker: LatestRequestWorker<QuickSettingsRequest>,
}

impl QuickSettingsWorker {
    fn with_source<S, F>(mut source: S, mut deliver: F) -> std::io::Result<Self>
    where
        S: QuickSettingsSource + Send + 'static,
        F: FnMut(QuickSettingsWorkerResult, NativeWindowId) + Send + 'static,
    {
        let mut started = false;
        let worker =
            LatestRequestWorker::spawn("quick-settings", move |request: QuickSettingsRequest| {
                if !started {
                    started = true;
                    let _ = source.worker_started();
                }
                let started_at = std::time::Instant::now();
                let payload = execute_request(&mut source, request.kind);
                record_slow_request(started_at.elapsed());
                deliver(
                    QuickSettingsWorkerResult::new(request.generation, payload),
                    request.wake_window,
                );
            })?;
        Ok(Self {
            generation: AtomicU64::new(0),
            worker,
        })
    }

    fn submit(&self, kind: QuickSettingsRequestKind, wake_window: NativeWindowId) -> u64 {
        let generation = self
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        self.worker.submit(QuickSettingsRequest {
            generation,
            wake_window,
            kind,
        });
        generation
    }

    pub(crate) fn request_refresh(&self, include_audio: bool, wake_window: NativeWindowId) -> u64 {
        self.submit(
            QuickSettingsRequestKind::Refresh { include_audio },
            wake_window,
        )
    }

    pub(crate) fn request_initial_refresh(&self, wake_window: NativeWindowId) -> u64 {
        static INITIAL_GENERATION: AtomicU64 = AtomicU64::new(0);
        let existing = INITIAL_GENERATION.load(Ordering::Acquire);
        if existing != 0 {
            return existing;
        }
        let generation = self.request_refresh(true, wake_window);
        let _ =
            INITIAL_GENERATION.compare_exchange(0, generation, Ordering::AcqRel, Ordering::Acquire);
        INITIAL_GENERATION.load(Ordering::Acquire)
    }

    pub(crate) fn request_action(
        &self,
        intent: QuickSettingsIntent,
        capability: Option<QuickControlCapability>,
        wake_window: NativeWindowId,
    ) -> u64 {
        self.submit(
            QuickSettingsRequestKind::Action { intent, capability },
            wake_window,
        )
    }
}

fn record_slow_request(elapsed: std::time::Duration) {
    if elapsed < std::time::Duration::from_millis(16) {
        return;
    }
    let elapsed_us = elapsed.as_micros().to_string();
    crate::diagnostics::record(
        crate::diagnostics::DiagnosticModule::AppLifecycle,
        crate::diagnostics::LogLevel::Info,
        "performance.quick_settings_worker",
        &[("elapsed_us", &elapsed_us)],
    );
}

#[cfg(windows)]
impl QuickSettingsWorker {
    pub(super) fn shared() -> windows::core::Result<Arc<Self>> {
        static INSTANCE: std::sync::OnceLock<Arc<QuickSettingsWorker>> = std::sync::OnceLock::new();
        if let Some(worker) = INSTANCE.get() {
            return Ok(Arc::clone(worker));
        }
        let worker = Arc::new(
            Self::with_source(
                NativeQuickSettingsSource::default(),
                |result, wake_window| {
                    let event = if result.is_refresh() {
                        RoutedPlatformEvent::broadcast(PlatformEvent::QuickSettingsWorkerCompleted(
                            result,
                        ))
                    } else {
                        RoutedPlatformEvent::window_id(
                            wake_window,
                            PlatformEvent::QuickSettingsWorkerCompleted(result),
                        )
                    };
                    let _ = queue_event_with_wake(event, || wake_owner_window(wake_window));
                },
            )
            .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))?,
        );
        let _ = INSTANCE.set(Arc::clone(&worker));
        Ok(INSTANCE.get().map_or(worker, Arc::clone))
    }
}

#[cfg(windows)]
fn wake_owner_window(window: NativeWindowId) -> bool {
    let hwnd = HWND(window.value() as *mut core::ffi::c_void);
    // SAFETY: the copied HWND is used only for an asynchronous wake-up.
    unsafe {
        PostMessageW(
            Some(hwnd),
            QUICK_SETTINGS_WAKE_MESSAGE,
            WPARAM(0),
            LPARAM(0),
        )
        .is_ok()
    }
}

#[cfg(windows)]
struct WorkerComApartment;

#[cfg(windows)]
impl WorkerComApartment {
    fn initialize() -> windows::core::Result<Self> {
        // SAFETY: initializes one MTA for the lifetime of the worker thread.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for WorkerComApartment {
    fn drop(&mut self) {
        // SAFETY: balances the successful initialization on this worker thread.
        unsafe { CoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AudioOutputId, AudioOutputSnapshot, AudioPanelSnapshot, QuickControlAvailability,
        QuickControlCapability, QuickSettingsCapabilities,
    };
    use shell_core::QuickControlKind;

    use super::{
        QuickSettingsRequestKind, QuickSettingsSource, QuickSettingsWorkerPayload, execute_request,
        runs_on_worker,
    };

    struct FixedSource {
        capabilities: QuickSettingsCapabilities,
        audio: AudioPanelSnapshot,
    }

    impl QuickSettingsSource for FixedSource {
        fn read_capabilities(&mut self) -> QuickSettingsCapabilities {
            self.capabilities.clone()
        }

        fn read_audio(&mut self) -> Result<AudioPanelSnapshot, String> {
            Ok(self.audio.clone())
        }
    }

    #[test]
    fn refresh_loads_capabilities_and_audio_in_one_worker_request() {
        let capabilities = QuickSettingsCapabilities::new(vec![QuickControlCapability::new(
            QuickControlKind::Volume,
            QuickControlAvailability::Available { active: true },
            "Volume",
            "Speakers",
            Some(47),
        )]);
        let audio = AudioPanelSnapshot::new(
            Vec::new(),
            vec![AudioOutputSnapshot::new(
                AudioOutputId::new(1),
                "Speakers",
                "Current output",
                true,
            )],
            false,
        );
        let mut source = FixedSource {
            capabilities: capabilities.clone(),
            audio: audio.clone(),
        };

        let QuickSettingsWorkerPayload::Refreshed {
            capabilities: loaded_capabilities,
            audio: loaded_audio,
        } = execute_request(
            &mut source,
            QuickSettingsRequestKind::Refresh {
                include_audio: true,
            },
        )
        else {
            panic!("refresh request returned an action payload");
        };

        assert_eq!(loaded_capabilities, capabilities);
        assert_eq!(loaded_audio, Some(audio));
    }

    #[test]
    fn capabilities_only_refresh_skips_audio_enumeration() {
        struct AudioMustNotRun;

        impl QuickSettingsSource for AudioMustNotRun {
            fn read_capabilities(&mut self) -> QuickSettingsCapabilities {
                QuickSettingsCapabilities::default()
            }

            fn read_audio(&mut self) -> Result<AudioPanelSnapshot, String> {
                panic!("audio enumeration must remain lazy")
            }
        }

        let QuickSettingsWorkerPayload::Refreshed { audio, .. } = execute_request(
            &mut AudioMustNotRun,
            QuickSettingsRequestKind::Refresh {
                include_audio: false,
            },
        ) else {
            panic!("refresh request returned an action payload");
        };

        assert_eq!(audio, None);
    }

    #[test]
    fn only_blocking_native_intents_are_routed_to_the_worker() {
        assert!(runs_on_worker(
            &crate::QuickSettingsIntent::SetDoNotDisturbMode(crate::DoNotDisturbMode::PriorityOnly)
        ));
        assert!(runs_on_worker(
            &crate::QuickSettingsIntent::SetAudioSessionVolume {
                id: crate::AudioSessionId::new(1),
                value: 72,
            }
        ));
        assert!(!runs_on_worker(
            &crate::QuickSettingsIntent::OpenSoundSettings
        ));
    }

    #[test]
    fn native_worker_is_shared_across_monitor_slots() {
        let first = super::QuickSettingsWorker::shared().expect("shared worker should start");
        let second = super::QuickSettingsWorker::shared().expect("shared worker should be reused");

        assert!(std::sync::Arc::ptr_eq(&first, &second));
    }
}
