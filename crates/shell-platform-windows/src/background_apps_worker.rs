use crate::background_apps::BackgroundAppEntry;
use crate::win32_background_apps::{BackgroundAppsError, capture_background_apps};

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

#[cfg(windows)]
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event};
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

#[cfg(windows)]
pub(super) fn request_background_apps(generation: u64, wake_window: NativeWindowId) {
    std::thread::spawn(move || {
        let result = load_request(&NativeBackgroundAppsSource, generation);
        queue_event(RoutedPlatformEvent::window_id(
            wake_window,
            PlatformEvent::BackgroundAppsLoaded(result),
        ));
        wake_owner_window(wake_window);
    });
}

#[cfg(windows)]
fn wake_owner_window(window: NativeWindowId) {
    let hwnd = HWND(window.value() as *mut core::ffi::c_void);
    // SAFETY: Category 8 (FFI boundary). The HWND is the copied owner-window
    // identity supplied by the UI thread. Posting is asynchronous and carries
    // no borrowed pointer or process data.
    let _ = unsafe {
        PostMessageW(
            Some(hwnd),
            BACKGROUND_APPS_WAKE_MESSAGE,
            WPARAM(0),
            LPARAM(0),
        )
    };
}

#[cfg(test)]
mod tests {
    use crate::background_apps::{
        BackgroundAppEntry, NotificationRegistration, RunningProcess, build_background_apps,
    };
    use crate::win32_background_apps::BackgroundAppsError;

    use super::{BackgroundAppsSource, load_request, request_background_apps};
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

    #[test]
    fn native_worker_has_a_one_shot_entrypoint() {
        let request: fn(u64, NativeWindowId) = request_background_apps;
        let _ = request;
    }
}
