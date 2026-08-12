use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::brightness_coordinator::{
    BrightnessApplyError, BrightnessApplyResult, BrightnessRequest,
};
use crate::latest_request_worker::LatestRequestWorker;
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event};
use crate::{NativeWindowId, PlatformEvent};

pub(super) const BRIGHTNESS_WAKE_MESSAGE: u32 = WM_APP + 0x65;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrightnessWorkerResult {
    pub(crate) request: u64,
    pub(crate) result: Result<BrightnessApplyResult, BrightnessApplyError>,
}

#[derive(Clone, Copy)]
struct BrightnessWorkerRequest {
    request: BrightnessRequest,
    wake_window: NativeWindowId,
}

pub(super) fn request_brightness(request: BrightnessRequest, wake_window: NativeWindowId) {
    static WORKER: std::sync::OnceLock<Option<LatestRequestWorker<BrightnessWorkerRequest>>> =
        std::sync::OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        LatestRequestWorker::spawn("brightness", |work: BrightnessWorkerRequest| {
            complete_brightness(work);
        })
        .ok()
    });
    if let Some(worker) = worker {
        worker.submit(BrightnessWorkerRequest {
            request,
            wake_window,
        });
    } else {
        std::thread::spawn(move || {
            complete_brightness(BrightnessWorkerRequest {
                request,
                wake_window,
            });
        });
    }
}

fn complete_brightness(work: BrightnessWorkerRequest) {
    let result = crate::win32_brightness::apply(work.request.value);
    queue_event(RoutedPlatformEvent::window_id(
        work.wake_window,
        PlatformEvent::BrightnessCompleted(BrightnessWorkerResult {
            request: work.request.id,
            result,
        }),
    ));
    let hwnd = HWND(work.wake_window.value() as *mut core::ffi::c_void);
    // SAFETY: only a wake-up message is posted; no pointer payload crosses threads.
    let _ = unsafe { PostMessageW(Some(hwnd), BRIGHTNESS_WAKE_MESSAGE, WPARAM(0), LPARAM(0)) };
}
