use windows::Win32::Foundation::E_FAIL;
use windows::core::{HRESULT, Result};

use crate::latest_request_worker::LatestRequestWorker;
use crate::win32_discovery::{WindowIdentityCache, discover_running_windows};
use crate::win32_preview_qa::seed_restricted_preview_for_qa;
use crate::win32_topbar_status::TopbarStatusReader;
use crate::{ObservedWindow, TopbarSnapshot, foreground_app_label};

#[cfg(windows)]
use windows::Win32::Foundation::{LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
#[cfg(windows)]
use windows::Win32::System::Threading::GetCurrentThreadId;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};

#[cfg(windows)]
use crate::PlatformEvent;
#[cfg(windows)]
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event_with_wake};

#[cfg(windows)]
pub(super) const SHELL_OBSERVATION_WAKE_MESSAGE: u32 = WM_APP + 0x63;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ShellObservation {
    windows: Vec<ObservedWindow>,
    topbar: TopbarSnapshot,
}

impl ShellObservation {
    pub(super) const fn new(windows: Vec<ObservedWindow>, topbar: TopbarSnapshot) -> Self {
        Self { windows, topbar }
    }

    pub(super) fn windows(&self) -> &[ObservedWindow] {
        &self.windows
    }

    pub(super) const fn topbar(&self) -> &TopbarSnapshot {
        &self.topbar
    }
}

pub(super) trait ShellObservationSource {
    fn initial_observation(&mut self, _now_ms: u64) -> ShellObservation {
        ShellObservation::new(Vec::new(), TopbarSnapshot::default())
    }

    fn capture(&mut self, now_ms: u64) -> Result<ShellObservation>;

    fn worker_started(&mut self) -> Result<()> {
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct WindowsShellObservationSource {
    identities: WindowIdentityCache,
    topbar: TopbarStatusReader,
    worker_apartment: Option<WorkerComApartment>,
}

impl ShellObservationSource for WindowsShellObservationSource {
    fn initial_observation(&mut self, now_ms: u64) -> ShellObservation {
        ShellObservation::new(Vec::new(), self.topbar.snapshot(now_ms))
    }

    fn capture(&mut self, now_ms: u64) -> Result<ShellObservation> {
        let mut windows = discover_running_windows(&[], true, &mut self.identities)?;
        seed_restricted_preview_for_qa(&mut windows)?;
        let app_label = foreground_app_label(&windows);
        let topbar = self.topbar.snapshot(now_ms).with_app_label(&app_label);
        Ok(ShellObservation::new(windows, topbar))
    }

    fn worker_started(&mut self) -> Result<()> {
        self.worker_apartment = Some(WorkerComApartment::initialize()?);
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct ShellObservationRequest {
    generation: u64,
    now_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShellObservationLoadResult {
    generation: u64,
    captured: std::result::Result<ShellObservation, HRESULT>,
}

impl ShellObservationLoadResult {
    fn new(generation: u64, captured: std::result::Result<ShellObservation, HRESULT>) -> Self {
        Self {
            generation,
            captured,
        }
    }

    fn into_captured(self) -> std::result::Result<ShellObservation, HRESULT> {
        self.captured
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShellObservationUpdate {
    Changed,
    Unchanged,
    Stale,
    Failed(HRESULT),
}

pub(super) struct ShellObservationRuntime {
    minimum_interval_ms: u64,
    last_request_ms: u64,
    latest_requested_generation: u64,
    current: ShellObservation,
    worker: LatestRequestWorker<ShellObservationRequest>,
}

impl ShellObservationRuntime {
    pub(super) fn new(minimum_interval_ms: u64, initial_now_ms: u64) -> Result<Self> {
        // SAFETY: Category 8 (FFI boundary). The runtime is created on the UI
        // thread after its message queue exists; the numeric id remains valid
        // until the message loop exits and the owned worker is joined.
        let ui_thread_id = unsafe { GetCurrentThreadId() };
        Self::with_source(
            WindowsShellObservationSource::default(),
            minimum_interval_ms,
            initial_now_ms,
            move |result| deliver_native_result(result, ui_thread_id),
        )
    }
}

impl ShellObservationRuntime {
    pub(super) fn with_source<S, F>(
        mut source: S,
        minimum_interval_ms: u64,
        initial_now_ms: u64,
        mut deliver: F,
    ) -> Result<Self>
    where
        S: ShellObservationSource + Send + 'static,
        F: FnMut(ShellObservationLoadResult) + Send + 'static,
    {
        let current = source.initial_observation(initial_now_ms);
        let mut worker_started = false;
        let mut worker_start_error = None;
        let worker = LatestRequestWorker::spawn(
            "shell-observation",
            move |request: ShellObservationRequest| {
                if !worker_started {
                    worker_started = true;
                    if let Err(error) = source.worker_started() {
                        worker_start_error = Some(error.code());
                    }
                }
                let started_at = std::time::Instant::now();
                let captured = worker_start_error.map_or_else(
                    || source.capture(request.now_ms).map_err(|error| error.code()),
                    Err,
                );
                record_slow_observation(started_at.elapsed());
                deliver(ShellObservationLoadResult::new(
                    request.generation,
                    captured,
                ));
            },
        )
        .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))?;
        let runtime = Self {
            minimum_interval_ms,
            last_request_ms: initial_now_ms,
            latest_requested_generation: 1,
            current,
            worker,
        };
        runtime.worker.submit(ShellObservationRequest {
            generation: 1,
            now_ms: initial_now_ms,
        });
        Ok(runtime)
    }

    pub(super) fn request_refresh(&mut self, now_ms: u64) -> bool {
        if now_ms.saturating_sub(self.last_request_ms) < self.minimum_interval_ms {
            return false;
        }
        self.last_request_ms = now_ms;
        self.latest_requested_generation = self.latest_requested_generation.wrapping_add(1);
        self.worker.submit(ShellObservationRequest {
            generation: self.latest_requested_generation,
            now_ms,
        });
        true
    }

    pub(super) fn complete(
        &mut self,
        result: ShellObservationLoadResult,
    ) -> ShellObservationUpdate {
        if result.generation != self.latest_requested_generation {
            return ShellObservationUpdate::Stale;
        }
        let observation = match result.into_captured() {
            Ok(observation) => observation,
            Err(code) => return ShellObservationUpdate::Failed(code),
        };
        if self.current == observation {
            return ShellObservationUpdate::Unchanged;
        }
        self.current = observation;
        ShellObservationUpdate::Changed
    }

    pub(super) const fn current(&self) -> &ShellObservation {
        &self.current
    }
}

fn record_slow_observation(elapsed: std::time::Duration) {
    if elapsed < std::time::Duration::from_millis(16) {
        return;
    }
    let elapsed_us = elapsed.as_micros().to_string();
    crate::diagnostics::record(
        crate::diagnostics::DiagnosticModule::AppLifecycle,
        crate::diagnostics::LogLevel::Info,
        "performance.shell_observation",
        &[("elapsed_us", &elapsed_us)],
    );
}

#[cfg(windows)]
fn deliver_native_result(result: ShellObservationLoadResult, ui_thread_id: u32) {
    let event = RoutedPlatformEvent::broadcast(PlatformEvent::ShellObservationLoaded(result));
    let _ = queue_event_with_wake(event, || {
        // SAFETY: Category 8 (FFI boundary). The process runtime owns this
        // worker and joins it before the UI thread queue can terminate.
        unsafe {
            PostThreadMessageW(
                ui_thread_id,
                SHELL_OBSERVATION_WAKE_MESSAGE,
                WPARAM(0),
                LPARAM(0),
            )
            .is_ok()
        }
    });
}

struct WorkerComApartment;

impl WorkerComApartment {
    fn initialize() -> Result<Self> {
        // SAFETY: Category 8 (FFI boundary). The observation worker initializes
        // one MTA before package and shell identity calls on that same thread.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
        Ok(Self)
    }
}

impl Drop for WorkerComApartment {
    fn drop(&mut self) {
        // SAFETY: Balances the successful initialization on the worker thread.
        unsafe { CoUninitialize() };
    }
}
