use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::brightness_coordinator::{
    BrightnessApplyError, BrightnessApplyResult, BrightnessRequest,
};
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event};
use crate::{NativeWindowId, PlatformEvent};

pub(super) const BRIGHTNESS_WAKE_MESSAGE: u32 = WM_APP + 0x65;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrightnessWorkerResult {
    pub(crate) request: u64,
    pub(crate) result: Result<BrightnessApplyResult, BrightnessApplyError>,
}

pub(super) fn request_brightness(request: BrightnessRequest, wake_window: NativeWindowId) {
    std::thread::spawn(move || {
        let result = crate::win32_brightness::apply(request.value);
        queue_event(RoutedPlatformEvent::window_id(
            wake_window,
            PlatformEvent::BrightnessCompleted(BrightnessWorkerResult {
                request: request.id,
                result,
            }),
        ));
        let hwnd = HWND(wake_window.value() as *mut core::ffi::c_void);
        // SAFETY: only a wake-up message is posted; no pointer payload crosses threads.
        let _ = unsafe { PostMessageW(Some(hwnd), BRIGHTNESS_WAKE_MESSAGE, WPARAM(0), LPARAM(0)) };
    });
}
