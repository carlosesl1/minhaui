use crate::background_apps::BackgroundAppEntry;
use crate::win32_background_apps::{BackgroundAppsError, capture_background_apps};

#[cfg(windows)]
use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::latest_request_worker::LatestRequestWorker;
#[cfg(windows)]
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event_with_wake};
#[cfg(windows)]
use crate::{NativeWindowId, PlatformEvent};

#[cfg(windows)]
pub(super) const BACKGROUND_APPS_WAKE_MESSAGE: u32 = WM_APP + 0x62;

pub(crate) trait BackgroundAppsSource {
    fn capture(&self) -> Result<Vec<BackgroundAppEntry>, BackgroundAppsError>;
}

struct NativeBackgroundAppsSource;

impl BackgroundAppsSource for NativeBackgroundAppsSource {
    fn capture(&self) -> Result<Vec<BackgroundAppEntry>, BackgroundAppsError> {
        capture_background_apps()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BackgroundAppsLoadResult {
    generation: u64,
    snapshot: Result<Vec<BackgroundAppEntry>, BackgroundAppsError>,
}

impl BackgroundAppsLoadResult {
    fn new(
        generation: u64,
        snapshot: Result<Vec<BackgroundAppEntry>, BackgroundAppsError>,
    ) -> Self {
        Self {
            generation,
            snapshot,
        }
    }

    #[cfg(test)]
    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    #[cfg(test)]
    pub(crate) const fn snapshot(&self) -> &Result<Vec<BackgroundAppEntry>, BackgroundAppsError> {
        &self.snapshot
    }

    pub(crate) fn into_parts(self) -> (u64, Result<Vec<BackgroundAppEntry>, BackgroundAppsError>) {
        (self.generation, self.snapshot)
    }
}

pub(crate) fn load_request(
    source: &impl BackgroundAppsSource,
    generation: u64,
) -> BackgroundAppsLoadResult {
    BackgroundAppsLoadResult::new(generation, source.capture())
}

#[derive(Clone, Copy)]
struct BackgroundAppsRequest {
    generation: u64,
    wake_window: NativeWindowId,
}

pub(crate) struct BackgroundAppsWorker {
    worker: LatestRequestWorker<BackgroundAppsRequest>,
}

impl BackgroundAppsWorker {
    fn with_source<S, F>(source: S, mut deliver: F) -> std::io::Result<Self>
    where
        S: BackgroundAppsSource + Send + 'static,
        F: FnMut(BackgroundAppsLoadResult, NativeWindowId) + Send + 'static,
    {
        let worker = LatestRequestWorker::spawn(
            "background-apps",
            move |request: BackgroundAppsRequest| {
                deliver(
                    load_request(&source, request.generation),
                    request.wake_window,
                );
            },
        )?;
        Ok(Self { worker })
    }

    pub(crate) fn request(&self, generation: u64, wake_window: NativeWindowId) {
        self.worker.submit(BackgroundAppsRequest {
            generation,
            wake_window,
        });
    }
}

#[cfg(windows)]
impl BackgroundAppsWorker {
    pub(super) fn new() -> windows::core::Result<Self> {
        Self::with_source(NativeBackgroundAppsSource, |result, wake_window| {
            let event = RoutedPlatformEvent::window_id(
                wake_window,
                PlatformEvent::BackgroundAppsLoaded(result),
            );
            let _ = queue_event_with_wake(event, || wake_owner_window(wake_window));
        })
        .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))
    }
}

#[cfg(windows)]
fn wake_owner_window(window: NativeWindowId) -> bool {
    let hwnd = HWND(window.value() as *mut core::ffi::c_void);
    // SAFETY: Category 8 (FFI boundary). The HWND is the copied owner-window
    // identity supplied by the UI thread. Posting is asynchronous and carries
    // no borrowed pointer or process data.
    unsafe {
        PostMessageW(
            Some(hwnd),
            BACKGROUND_APPS_WAKE_MESSAGE,
            WPARAM(0),
            LPARAM(0),
        )
        .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex, mpsc};
    use std::time::Duration;

    use crate::background_apps::{
        BackgroundAppEntry, NotificationRegistration, RunningProcess, build_background_apps,
    };
    use crate::win32_background_apps::BackgroundAppsError;

    use super::{BackgroundAppsSource, BackgroundAppsWorker, load_request};
    use crate::NativeWindowId;

    struct FixedBackgroundAppsSource {
        snapshot: Result<Vec<BackgroundAppEntry>, BackgroundAppsError>,
    }

    impl BackgroundAppsSource for FixedBackgroundAppsSource {
        fn capture(&self) -> Result<Vec<BackgroundAppEntry>, BackgroundAppsError> {
            self.snapshot.clone()
        }
    }

    #[test]
    fn worker_returns_the_matching_generation() {
        let registrations = [NotificationRegistration::new(r"C:\Apps\Steam.exe", "Steam")];
        let processes = [RunningProcess::new(7, r"D:\Live\Steam.exe", "Steam")];
        let source = FixedBackgroundAppsSource {
            snapshot: Ok(build_background_apps(&registrations, &processes)),
        };

        let result = load_request(&source, 41);

        assert_eq!(result.generation(), 41);
        assert_eq!(result.snapshot().as_ref().unwrap()[0].label(), "Steam");
        let (generation, snapshot) = result.into_parts();
        assert_eq!(generation, 41);
        assert_eq!(snapshot.unwrap().len(), 1);
    }

    struct BlockingBackgroundAppsSource {
        captures: Arc<AtomicUsize>,
        first_started: mpsc::Sender<()>,
        release_first: Arc<(Mutex<bool>, Condvar)>,
        capture_active: Arc<AtomicBool>,
        concurrent_capture: Arc<AtomicBool>,
    }

    impl BackgroundAppsSource for BlockingBackgroundAppsSource {
        fn capture(&self) -> Result<Vec<BackgroundAppEntry>, BackgroundAppsError> {
            if self.capture_active.swap(true, Ordering::AcqRel) {
                self.concurrent_capture.store(true, Ordering::Release);
            }
            let capture = self.captures.fetch_add(1, Ordering::AcqRel);
            if capture == 0 {
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
            self.capture_active.store(false, Ordering::Release);
            Ok(Vec::new())
        }
    }

    #[test]
    fn worker_serializes_scans_and_coalesces_reopens_to_the_latest_generation() {
        let captures = Arc::new(AtomicUsize::new(0));
        let capture_active = Arc::new(AtomicBool::new(false));
        let concurrent_capture = Arc::new(AtomicBool::new(false));
        let release_first = Arc::new((Mutex::new(false), Condvar::new()));
        let (first_started_tx, first_started_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let source = BlockingBackgroundAppsSource {
            captures: Arc::clone(&captures),
            first_started: first_started_tx,
            release_first: Arc::clone(&release_first),
            capture_active: Arc::clone(&capture_active),
            concurrent_capture: Arc::clone(&concurrent_capture),
        };
        let worker = BackgroundAppsWorker::with_source(source, move |result, _wake_window| {
            let _ = result_tx.send(result.generation());
        })
        .expect("worker should start");
        let wake_window = NativeWindowId::new(7);

        worker.request(1, wake_window);
        first_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("first capture should start");
        worker.request(2, wake_window);
        worker.request(3, wake_window);
        {
            let (released, wake) = &*release_first;
            *released
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
            wake.notify_all();
        }

        assert_eq!(
            result_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("first result should arrive"),
            1
        );
        assert_eq!(
            result_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("coalesced result should arrive"),
            3
        );
        assert_eq!(captures.load(Ordering::Acquire), 2);
        assert!(!concurrent_capture.load(Ordering::Acquire));
        drop(worker);
        assert_eq!(
            result_rx.recv_timeout(Duration::from_millis(10)),
            Err(mpsc::RecvTimeoutError::Disconnected),
            "dropping the owner must join the worker and release its sink"
        );
    }
}
