use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use shell_renderer::DipPoint;
use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::latest_request_worker::LatestRequestWorker;
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event_with_wake};
use crate::{BackgroundAppId, NativeWindowId, PlatformEvent};

pub(super) const SHELL_MENU_WAKE_MESSAGE: u32 = WM_APP + 0x67;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellMenuActivationOutcome {
    Activated,
    Unavailable,
    Superseded,
    Failed,
}

impl ShellMenuActivationOutcome {
    const fn code(self) -> &'static str {
        match self {
            Self::Activated => "activated",
            Self::Unavailable => "unavailable",
            Self::Superseded => "superseded",
            Self::Failed => "windows_error",
        }
    }
}

pub(crate) trait ShellMenuSource {
    fn activate(
        &self,
        label: &str,
        executable: &str,
        is_current: &dyn Fn() -> bool,
    ) -> ShellMenuActivationOutcome;
}

struct NativeShellMenuSource;

impl ShellMenuSource for NativeShellMenuSource {
    fn activate(
        &self,
        label: &str,
        executable: &str,
        is_current: &dyn Fn() -> bool,
    ) -> ShellMenuActivationOutcome {
        let Ok(_com) = ComThreadGuard::initialize() else {
            return ShellMenuActivationOutcome::Failed;
        };
        crate::win32_tray_activation::activate_windows_shell_app_context_menu_if_current(
            label, executable, is_current,
        )
        .unwrap_or(ShellMenuActivationOutcome::Failed)
    }
}

struct ComThreadGuard;

impl ComThreadGuard {
    fn initialize() -> windows::core::Result<Self> {
        // SAFETY: The worker owns this thread and balances successful COM
        // initialization with CoUninitialize from this guard's Drop.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok()?;
        Ok(Self)
    }
}

impl Drop for ComThreadGuard {
    fn drop(&mut self) {
        // SAFETY: This balances the successful CoInitializeEx call on the same
        // owned worker thread.
        unsafe { CoUninitialize() };
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShellMenuActivationResult {
    id: u64,
    app: BackgroundAppId,
    generation: u64,
    point: Option<DipPoint>,
    outcome: ShellMenuActivationOutcome,
}

impl ShellMenuActivationResult {
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn app(&self) -> BackgroundAppId {
        self.app
    }

    #[must_use]
    const fn id(&self) -> u64 {
        self.id
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn outcome(&self) -> ShellMenuActivationOutcome {
        self.outcome
    }

    pub(crate) const fn into_parts(
        self,
    ) -> (
        u64,
        BackgroundAppId,
        u64,
        Option<DipPoint>,
        ShellMenuActivationOutcome,
    ) {
        (self.id, self.app, self.generation, self.point, self.outcome)
    }
}

#[derive(Clone)]
pub(crate) struct ShellMenuRequest {
    id: u64,
    app: BackgroundAppId,
    generation: u64,
    label: String,
    executable: String,
    point: Option<DipPoint>,
    wake_window: NativeWindowId,
}

impl ShellMenuRequest {
    pub(crate) fn new(
        app: BackgroundAppId,
        generation: u64,
        label: &str,
        executable: &str,
        point: Option<DipPoint>,
        wake_window: NativeWindowId,
    ) -> Self {
        Self {
            id: 0,
            app,
            generation,
            label: label.to_owned(),
            executable: executable.to_owned(),
            point,
            wake_window,
        }
    }
}

pub(crate) struct ShellMenuWorker {
    worker: LatestRequestWorker<ShellMenuRequest>,
    latest_request: Arc<AtomicU64>,
}

impl ShellMenuWorker {
    fn with_source<S, F>(source: S, mut deliver: F) -> std::io::Result<Self>
    where
        S: ShellMenuSource + Send + 'static,
        F: FnMut(ShellMenuActivationResult, NativeWindowId) + Send + 'static,
    {
        let latest_request = Arc::new(AtomicU64::new(0));
        let worker_latest = Arc::clone(&latest_request);
        let worker = LatestRequestWorker::spawn("shell-menu", move |request: ShellMenuRequest| {
            let request_id = request.id;
            let is_current = || worker_latest.load(Ordering::Acquire) == request_id;
            let outcome = source.activate(&request.label, &request.executable, &is_current);
            if !is_current() || outcome == ShellMenuActivationOutcome::Superseded {
                return;
            }
            crate::diagnostics::record(
                crate::diagnostics::DiagnosticModule::AppLifecycle,
                crate::diagnostics::LogLevel::Info,
                "background_app_shell_menu",
                &[("outcome", outcome.code())],
            );
            deliver(
                ShellMenuActivationResult {
                    id: request.id,
                    app: request.app,
                    generation: request.generation,
                    point: request.point,
                    outcome,
                },
                request.wake_window,
            );
        })?;
        Ok(Self {
            worker,
            latest_request,
        })
    }

    pub(crate) fn request(&self, mut request: ShellMenuRequest) {
        request.id = self
            .latest_request
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        self.worker.submit(request);
    }

    pub(crate) fn cancel_pending(&self) {
        self.latest_request.fetch_add(1, Ordering::AcqRel);
    }

    #[must_use]
    pub(crate) fn is_current(&self, result: &ShellMenuActivationResult) -> bool {
        self.latest_request.load(Ordering::Acquire) == result.id()
    }
}

impl ShellMenuWorker {
    pub(super) fn new() -> windows::core::Result<Self> {
        Self::with_source(NativeShellMenuSource, |result, wake_window| {
            let event = RoutedPlatformEvent::window_id(
                wake_window,
                PlatformEvent::ShellMenuActivationCompleted(result),
            );
            let _ = queue_event_with_wake(event, || wake_owner_window(wake_window));
        })
        .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))
    }
}

fn wake_owner_window(window: NativeWindowId) -> bool {
    let hwnd = HWND(window.value() as *mut core::ffi::c_void);
    // SAFETY: The copied owner HWND is used only for one asynchronous wake;
    // request data remains owned by the event queue.
    unsafe { PostMessageW(Some(hwnd), SHELL_MENU_WAKE_MESSAGE, WPARAM(0), LPARAM(0)).is_ok() }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex, mpsc};
    use std::time::Duration;

    use shell_renderer::DipPoint;

    use super::{ShellMenuActivationOutcome, ShellMenuRequest, ShellMenuSource, ShellMenuWorker};
    use crate::{BackgroundAppId, NativeWindowId};

    struct BlockingSource {
        first_started: mpsc::Sender<()>,
        release_first: Arc<(Mutex<bool>, Condvar)>,
        first: AtomicBool,
        activated: Arc<Mutex<Vec<String>>>,
    }

    impl ShellMenuSource for BlockingSource {
        fn activate(
            &self,
            label: &str,
            _executable: &str,
            is_current: &dyn Fn() -> bool,
        ) -> ShellMenuActivationOutcome {
            if self.first.swap(false, Ordering::AcqRel) {
                let _ = self.first_started.send(());
                let (released, wake) = &*self.release_first;
                let mut released = released
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                while !*released {
                    released = wake
                        .wait(released)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            }
            if !is_current() {
                return ShellMenuActivationOutcome::Superseded;
            }
            self.activated
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(label.to_owned());
            ShellMenuActivationOutcome::Activated
        }
    }

    #[test]
    fn worker_keeps_ui_requests_nonblocking_and_only_activates_the_latest_app() {
        let release_first = Arc::new((Mutex::new(false), Condvar::new()));
        let activated = Arc::new(Mutex::new(Vec::new()));
        let (first_started_tx, first_started_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let source = BlockingSource {
            first_started: first_started_tx,
            release_first: Arc::clone(&release_first),
            first: AtomicBool::new(true),
            activated: Arc::clone(&activated),
        };
        let worker = ShellMenuWorker::with_source(source, move |result, _wake_window| {
            let _ = result_tx.send(result);
        })
        .expect("worker should start");
        let wake_window = NativeWindowId::new(7);

        worker.request(ShellMenuRequest::new(
            BackgroundAppId::new(1),
            11,
            "First",
            "first.exe",
            Some(DipPoint::new(1.0, 1.0)),
            wake_window,
        ));
        first_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("first discovery should start");
        worker.request(ShellMenuRequest::new(
            BackgroundAppId::new(2),
            11,
            "Second",
            "second.exe",
            Some(DipPoint::new(2.0, 2.0)),
            wake_window,
        ));
        {
            let (released, wake) = &*release_first;
            *released
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
            wake.notify_all();
        }

        let result = result_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("latest result should arrive");
        assert_eq!(result.app(), BackgroundAppId::new(2));
        assert_eq!(result.generation(), 11);
        assert_eq!(result.outcome(), ShellMenuActivationOutcome::Activated);
        assert_eq!(
            *activated
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec!["Second".to_owned()]
        );
        assert_eq!(
            result_rx.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Timeout)
        );
    }
}
