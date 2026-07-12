use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};

use shell_renderer::{DipPoint, Dpi, PhysicalRect};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, GetMessageW, MSG, PBT_APMRESUMEAUTOMATIC, PostQuitMessage,
    SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, TranslateMessage, WM_CLOSE, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_DROPFILES, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_NCHITTEST, WM_POWERBROADCAST, WM_RBUTTONUP, WM_TIMER,
};
use windows::core::Result;

use crate::win32::{
    DOCK_DRAGGING, DOCK_WINDOW, LIVE_WINDOWS, SYNC_TIMER_ID, TASKBAR_CREATED, TIMER_ID,
};
use crate::win32_context_menu::track_dock_context_menu;
use crate::win32_drop::first_drop_path;
use crate::win32_hit_test::hit_test;
pub(super) use crate::win32_work_area::primary_work_area;
use crate::{DockPointerPhase, DockPointerSample, PlatformEvent};

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
        WM_MOUSEMOVE if is_dock_window(hwnd) => {
            track_mouse_leave(hwnd);
            let phase = if DOCK_DRAGGING.load(Ordering::Acquire) {
                DockPointerPhase::Dragged
            } else {
                DockPointerPhase::Moved
            };
            queue_event(PlatformEvent::DockPointer(DockPointerSample::new(
                phase,
                client_point(hwnd, lparam),
            )));
            LRESULT(0)
        }
        WM_MOUSELEAVE if is_dock_window(hwnd) => {
            DOCK_DRAGGING.store(false, Ordering::Release);
            queue_event(PlatformEvent::DockPointer(DockPointerSample::new(
                DockPointerPhase::Exited,
                DipPoint::new(-1.0, -1.0),
            )));
            LRESULT(0)
        }
        WM_LBUTTONDOWN if is_dock_window(hwnd) => {
            DOCK_DRAGGING.store(true, Ordering::Release);
            queue_event(PlatformEvent::DockPointer(DockPointerSample::new(
                DockPointerPhase::Pressed,
                client_point(hwnd, lparam),
            )));
            LRESULT(0)
        }
        WM_LBUTTONUP if is_dock_window(hwnd) => {
            DOCK_DRAGGING.store(false, Ordering::Release);
            queue_event(PlatformEvent::DockPointer(DockPointerSample::new(
                DockPointerPhase::Released,
                client_point(hwnd, lparam),
            )));
            LRESULT(0)
        }
        WM_RBUTTONUP if is_dock_window(hwnd) => {
            if let Some(command) = track_dock_context_menu(hwnd, client_physical_point(lparam)) {
                queue_event(PlatformEvent::DockContextMenu {
                    point: client_point(hwnd, lparam),
                    command,
                });
            }
            LRESULT(0)
        }
        WM_DROPFILES if is_dock_window(hwnd) => {
            if let Some(path) = first_drop_path(wparam) {
                queue_event(PlatformEvent::DockDrop {
                    point: DipPoint::new(0.0, 0.0),
                    path,
                });
            }
            LRESULT(0)
        }
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
        WM_TIMER if wparam.0 == SYNC_TIMER_ID => {
            queue_event(PlatformEvent::SyncWindows);
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

fn is_dock_window(hwnd: HWND) -> bool {
    DOCK_WINDOW.load(Ordering::Acquire) == hwnd.0 as isize
}

fn track_mouse_leave(hwnd: HWND) {
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

fn client_point(hwnd: HWND, lparam: LPARAM) -> DipPoint {
    let point = client_physical_point(lparam);
    // SAFETY: Category 8 (FFI boundary). The callback supplies a live HWND.
    let dpi = Dpi::from_raw(unsafe { GetDpiForWindow(hwnd) }.max(96));
    let scale = dpi.scale();
    DipPoint::new(point.x as f32 / scale, point.y as f32 / scale)
}

fn client_physical_point(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam.0 as u16) as i16 as i32,
        y: ((lparam.0 >> 16) as u16) as i16 as i32,
    }
}
