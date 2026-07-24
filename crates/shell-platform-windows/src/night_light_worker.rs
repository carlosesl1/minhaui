use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

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

pub(super) fn request_night_light(request: NightLightRequest, wake_window: NativeWindowId) {
    std::thread::spawn(move || {
        let result = crate::win32_quick_settings_system::apply_night_light(
            request.active,
            request.minimum_timestamp,
        );
        queue_event(RoutedPlatformEvent::window_id(
            wake_window,
            PlatformEvent::NightLightCompleted(NightLightWorkerResult {
                request: request.id,
                result,
            }),
        ));
        wake_owner_window(wake_window);
    });
}

fn wake_owner_window(window: NativeWindowId) {
    let hwnd = HWND(window.value() as *mut core::ffi::c_void);
    // SAFETY: the copied HWND identifies the UI-thread window and no pointer payload crosses threads.
    let _ = unsafe { PostMessageW(Some(hwnd), NIGHT_LIGHT_WAKE_MESSAGE, WPARAM(0), LPARAM(0)) };
}
