use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::latest_request_worker::LatestRequestWorker;
use crate::night_light_coordinator::{
    NightLightApplyError, NightLightApplyResult, NightLightRequest,
};
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event};
use crate::{NativeWindowId, PlatformEvent};

pub(super) const NIGHT_LIGHT_WAKE_MESSAGE: u32 = WM_APP + 0x64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NightLightWorkerResult {
    pub(crate) request: u64,
    pub(crate) result: Result<NightLightApplyResult, NightLightApplyError>,
}

#[derive(Clone, Copy)]
struct NightLightWorkerRequest {
    request: NightLightRequest,
    wake_window: NativeWindowId,
}

pub(super) fn request_night_light(request: NightLightRequest, wake_window: NativeWindowId) {
    static WORKER: std::sync::OnceLock<Option<LatestRequestWorker<NightLightWorkerRequest>>> =
        std::sync::OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        LatestRequestWorker::spawn("night-light", |work: NightLightWorkerRequest| {
            complete_night_light(work);
        })
        .ok()
    });
    if let Some(worker) = worker {
        worker.submit(NightLightWorkerRequest {
            request,
            wake_window,
        });
    } else {
        std::thread::spawn(move || {
            complete_night_light(NightLightWorkerRequest {
                request,
                wake_window,
            });
        });
    }
}

fn complete_night_light(work: NightLightWorkerRequest) {
    let result = crate::win32_quick_settings_system::apply_night_light(
        work.request.active,
        work.request.minimum_timestamp,
    );
    queue_event(RoutedPlatformEvent::window_id(
        work.wake_window,
        PlatformEvent::NightLightCompleted(NightLightWorkerResult {
            request: work.request.id,
            result,
        }),
    ));
    wake_owner_window(work.wake_window);
}

fn wake_owner_window(window: NativeWindowId) {
    let hwnd = HWND(window.value() as *mut core::ffi::c_void);
    // SAFETY: the copied HWND identifies the UI-thread window and no pointer payload crosses threads.
    let _ = unsafe { PostMessageW(Some(hwnd), NIGHT_LIGHT_WAKE_MESSAGE, WPARAM(0), LPARAM(0)) };
}
