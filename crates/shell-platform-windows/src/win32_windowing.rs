use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};

use shell_renderer::{
    DipPoint, DipRect, Dpi, PhysicalRect, physical_from_dip, rounded_content_hit,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowRect, HTCLIENT, HTTRANSPARENT, MSG,
    PBT_APMRESUMEAUTOMATIC, PostQuitMessage, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
    TranslateMessage, WM_CLOSE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCHITTEST,
    WM_POWERBROADCAST, WM_TIMER,
};
use windows::core::Result;

use crate::PlatformEvent;
use crate::win32::{LIVE_WINDOWS, TASKBAR_CREATED, TIMER_ID};

static EVENT_QUEUE: OnceLock<Mutex<VecDeque<PlatformEvent>>> = OnceLock::new();

fn queue_event(event: PlatformEvent) {
    let queue = EVENT_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Ok(mut events) = queue.lock() {
        events.push_back(event);
    }
}

fn next_event() -> Option<PlatformEvent> {
    EVENT_QUEUE
        .get()
        .and_then(|queue| queue.lock().ok()?.pop_front())
}

pub(super) fn primary_work_area() -> Result<PhysicalRect> {
    // SAFETY: Category 8 (FFI boundary). The default-primary flag guarantees a valid
    // monitor handle even when the origin is outside a monitor.
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). `info` has the required size field and is
    // valid writable storage for the duration of the call.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(windows::core::Error::from_thread());
    }
    Ok(rect_from_win32(info.rcWork))
}

pub(super) fn message_loop(
    mut handle_event: impl FnMut(PlatformEvent) -> Result<bool>,
) -> Result<()> {
    let mut message = MSG::default();
    loop {
        // SAFETY: Category 8 (FFI boundary). `message` is writable for the call and
        // no HWND filter means this thread drains both owned windows deterministically.
        let status = unsafe { GetMessageW(&mut message, None, 0, 0) };
        match status.0 {
            -1 => return Err(windows::core::Error::from_thread()),
            0 => return Ok(()),
            _ => {
                // SAFETY: Category 8 (FFI boundary). The message was initialized by
                // GetMessageW and remains valid through translation and dispatch.
                let _ = unsafe { TranslateMessage(&message) };
                // SAFETY: Category 8 (FFI boundary). Same initialized-message
                // invariant; dispatch synchronously invokes the registered callback.
                unsafe { DispatchMessageW(&message) };
                while let Some(event) = next_event() {
                    if !handle_event(event)? {
                        return Ok(());
                    }
                }
            }
        }
    }
}

const fn rect_from_win32(rect: RECT) -> PhysicalRect {
    PhysicalRect::new(
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )
}

pub(super) unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == TASKBAR_CREATED.load(Ordering::Acquire) {
        queue_event(PlatformEvent::TaskbarCreated);
        return LRESULT(0);
    }
    match message {
        WM_NCHITTEST => hit_test(hwnd, lparam),
        WM_DPICHANGED => {
            // SAFETY: Category 8 (FFI boundary). Windows documents lParam for
            // WM_DPICHANGED as a valid RECT pointer for the duration of the callback.
            let recommended = unsafe { *(lparam.0 as *const RECT) };
            // SAFETY: Category 8 (FFI boundary). The suggested rectangle comes from
            // Windows and NOACTIVATE preserves shell focus behavior.
            let _ = unsafe {
                SetWindowPos(
                    hwnd,
                    None,
                    recommended.left,
                    recommended.top,
                    recommended.right - recommended.left,
                    recommended.bottom - recommended.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            queue_event(PlatformEvent::DpiChanged(rect_from_win32(recommended)));
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            queue_event(PlatformEvent::DisplayChanged);
            LRESULT(0)
        }
        WM_POWERBROADCAST if wparam.0 as u32 == PBT_APMRESUMEAUTOMATIC => {
            queue_event(PlatformEvent::PowerResumed);
            LRESULT(1)
        }
        WM_TIMER if wparam.0 == TIMER_ID => {
            queue_event(PlatformEvent::QaExitRequested);
            LRESULT(0)
        }
        WM_CLOSE => {
            queue_event(PlatformEvent::CloseRequested);
            LRESULT(0)
        }
        WM_DESTROY => {
            if LIVE_WINDOWS.fetch_sub(1, Ordering::AcqRel) == 1 {
                // SAFETY: Category 8 (FFI boundary). The final owned HWND has been
                // destroyed, so posting quit deterministically terminates this loop.
                unsafe { PostQuitMessage(0) };
            }
            queue_event(PlatformEvent::Destroyed);
            LRESULT(0)
        }
        _ => {
            // SAFETY: Category 8 (FFI boundary). Unhandled messages and their exact
            // parameters are forwarded unchanged to the documented default callback.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}

fn hit_test(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let mut rect = RECT::default();
    // SAFETY: Category 8 (FFI boundary). The callback supplies a live HWND and `rect`
    // is valid writable storage for the call.
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return LRESULT(HTTRANSPARENT as isize);
    }
    let screen_x = (lparam.0 as u16) as i16 as i32;
    let screen_y = ((lparam.0 >> 16) as u16) as i16 as i32;
    // SAFETY: Category 8 (FFI boundary). The window remains live during its callback,
    // so querying its effective DPI is valid.
    let dpi = Dpi::from_raw(unsafe { GetDpiForWindow(hwnd) }.max(96));
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let scale = dpi.scale();
    let point = DipPoint::new(
        (screen_x - rect.left) as f32 / scale,
        (screen_y - rect.top) as f32 / scale,
    );
    let bounds = DipRect::new(0.0, 0.0, width as f32 / scale, height as f32 / scale);
    let radius = if height <= physical_from_dip(40.0, dpi) {
        12.0
    } else {
        22.0
    };
    if rounded_content_hit(bounds, radius, point) {
        LRESULT(HTCLIENT as isize)
    } else {
        LRESULT(HTTRANSPARENT as isize)
    }
}
