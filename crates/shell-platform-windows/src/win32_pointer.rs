use shell_renderer::{DipPoint, Dpi};
use windows::Win32::Foundation::{HWND, LPARAM, POINT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

pub(super) fn track_mouse_leave(hwnd: HWND) {
    let mut event = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    // SAFETY: Category 8 (FFI boundary). The tracked HWND is live during message
    // dispatch and the structure contains the documented size and leave flag.
    let _ = unsafe { TrackMouseEvent(&mut event) };
}

pub(super) fn client_point(hwnd: HWND, lparam: LPARAM) -> DipPoint {
    let point = client_physical_point(lparam);
    // SAFETY: Category 8 (FFI boundary). The callback supplies a live HWND.
    let dpi = Dpi::from_raw(unsafe { GetDpiForWindow(hwnd) }.max(96));
    let scale = dpi.scale();
    DipPoint::new(point.x as f32 / scale, point.y as f32 / scale)
}

pub(super) fn client_physical_point(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam.0 as u16) as i16 as i32,
        y: ((lparam.0 >> 16) as u16) as i16 as i32,
    }
}

pub(super) fn cursor_client_point(hwnd: HWND) -> Option<DipPoint> {
    let mut point = POINT::default();
    // SAFETY: Category 8 (FFI boundary). The pointer to POINT is valid for the
    // synchronous cursor-position query.
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }
    // SAFETY: Category 8 (FFI boundary). The HWND is live during dispatch and
    // `point` is valid mutable storage for the screen-to-client conversion.
    if !unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
        return None;
    }
    // SAFETY: Category 8 (FFI boundary). The callback supplies a live HWND.
    let dpi = Dpi::from_raw(unsafe { GetDpiForWindow(hwnd) }.max(96));
    let scale = dpi.scale();
    Some(DipPoint::new(
        point.x as f32 / scale,
        point.y as f32 / scale,
    ))
}
