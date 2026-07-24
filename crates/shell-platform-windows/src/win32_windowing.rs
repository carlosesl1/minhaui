use std::sync::atomic::Ordering;

use shell_renderer::{DipPoint, PhysicalRect};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, ReleaseCapture, SetCapture, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT,
    VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, GetMessageW, MSG, PBT_APMRESUMEAUTOMATIC, PostQuitMessage,
    SWP_NOACTIVATE, SWP_NOZORDER, SetTimer, SetWindowPos, TranslateMessage, WA_INACTIVE,
    WM_ACTIVATE, WM_CANCELMODE, WM_CAPTURECHANGED, WM_CLOSE, WM_DESTROY, WM_DEVICECHANGE,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_DROPFILES, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCHITTEST, WM_POWERBROADCAST, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_TIMER,
};
use windows::core::Result;

use crate::background_apps_worker::BACKGROUND_APPS_WAKE_MESSAGE;
use crate::brightness_worker::BRIGHTNESS_WAKE_MESSAGE;
use crate::media_session_worker::MEDIA_SESSION_WAKE_MESSAGE;
use crate::night_light_worker::NIGHT_LIGHT_WAKE_MESSAGE;
use crate::win32::{
    DOCK_ANIMATION_TIMER_ID, DOCK_EDGE_PROBE_TIMER_ID, DRAG_ESCAPE_TIMER_ID, LIVE_WINDOWS,
    PREVIEW_TIMER_ID, SYNC_TIMER_ID, TASKBAR_CREATED, TIMER_ID,
};
use crate::win32_appbar::{is_position_notification, notify_activation, notify_window_position};
use crate::win32_drop::first_drop_path;
use crate::win32_event_queue::{
    RoutedPlatformEvent, is_dragging, next_event, queue_event, set_dragging,
};
use crate::win32_hit_test::hit_test;
use crate::win32_pointer::{client_point, track_mouse_leave};
pub(super) use crate::win32_work_area::{
    monitor_placement_inputs, window_monitor_bounds, window_monitor_id, window_work_area,
};
use crate::{
    DockKey, DockPointerPhase, DockPointerSample, PlatformEvent, PopoverKey, SettingsKey,
    TopbarKey, TopbarPointerPhase, TopbarPointerSample, popover_activation_event,
};

pub(super) fn message_loop(
    mut handle_event: impl FnMut(RoutedPlatformEvent) -> Result<bool>,
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
        queue_event(RoutedPlatformEvent::broadcast(
            PlatformEvent::TaskbarCreated,
        ));
        return LRESULT(0);
    }
    if message == BACKGROUND_APPS_WAKE_MESSAGE
        || message == MEDIA_SESSION_WAKE_MESSAGE
        || message == NIGHT_LIGHT_WAKE_MESSAGE
        || message == BRIGHTNESS_WAKE_MESSAGE
    {
        return LRESULT(0);
    }
    if is_position_notification(message, wparam.0) {
        queue_event(RoutedPlatformEvent::window(
            hwnd,
            PlatformEvent::AppBarPositionChanged,
        ));
        return LRESULT(0);
    }
    match message {
        WM_MOUSEWHEEL if is_popover_window(hwnd) => {
            let delta = ((wparam.0 >> 16) as u16) as i16;
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PopoverScroll(-isize::from(delta.signum())),
            ));
            LRESULT(0)
        }
        WM_MOUSEMOVE if is_dock_window(hwnd) => {
            track_mouse_leave(hwnd);
            if is_dragging(hwnd) && escape_pressed() {
                cancel_dock_capture(hwnd);
                return LRESULT(0);
            }
            let phase = if is_dragging(hwnd) {
                DockPointerPhase::Dragged
            } else {
                DockPointerPhase::Moved
            };
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DockPointer(DockPointerSample::new(
                    phase,
                    client_point(hwnd, lparam),
                )),
            ));
            LRESULT(0)
        }
        WM_MOUSELEAVE if is_dock_window(hwnd) => {
            if let Some(phase) = dock_capture_termination(message, is_dragging(hwnd)) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::DockPointer(DockPointerSample::new(
                        phase,
                        DipPoint::new(-1.0, -1.0),
                    )),
                ));
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED if is_dock_window(hwnd) => {
            if let Some(phase) = dock_capture_termination(message, is_dragging(hwnd)) {
                end_dock_drag_tracking(hwnd);
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::DockPointer(DockPointerSample::new(
                        phase,
                        DipPoint::new(-1.0, -1.0),
                    )),
                ));
            }
            LRESULT(0)
        }
        WM_CANCELMODE if is_dock_window(hwnd) => {
            if is_dragging(hwnd) {
                cancel_dock_capture(hwnd);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN if is_dock_window(hwnd) => {
            set_dragging(hwnd, true);
            // SAFETY: Category 8 (FFI boundary). The live dock HWND belongs to this
            // UI thread and retains capture only until its matching release/cancel.
            unsafe { SetCapture(hwnd) };
            // SAFETY: Category 8 (FFI boundary). The live dock owns this numeric
            // timer only for the bounded duration of native mouse capture.
            unsafe { SetTimer(Some(hwnd), DRAG_ESCAPE_TIMER_ID, 16, None) };
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DockPointer(DockPointerSample::new(
                    DockPointerPhase::Pressed,
                    client_point(hwnd, lparam),
                )),
            ));
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::DismissTransientOverlays,
            ));
            LRESULT(0)
        }
        WM_LBUTTONUP if is_dock_window(hwnd) => {
            release_dock_capture(hwnd);
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DockPointer(DockPointerSample::new(
                    DockPointerPhase::Released,
                    client_point(hwnd, lparam),
                )),
            ));
            LRESULT(0)
        }
        WM_MOUSEMOVE if is_topbar_window(hwnd) => {
            track_mouse_leave(hwnd);
            queue_topbar_pointer(hwnd, TopbarPointerPhase::Moved, client_point(hwnd, lparam));
            LRESULT(0)
        }
        WM_MOUSELEAVE if is_topbar_window(hwnd) => {
            queue_topbar_pointer(hwnd, TopbarPointerPhase::Exited, DipPoint::new(-1.0, -1.0));
            LRESULT(0)
        }
        WM_LBUTTONDOWN if is_topbar_window(hwnd) => {
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::DismissTransientOverlays,
            ));
            queue_topbar_pointer(
                hwnd,
                TopbarPointerPhase::Pressed,
                client_point(hwnd, lparam),
            );
            LRESULT(0)
        }
        WM_LBUTTONUP if is_topbar_window(hwnd) => {
            queue_topbar_pointer(
                hwnd,
                TopbarPointerPhase::Released,
                client_point(hwnd, lparam),
            );
            LRESULT(0)
        }
        WM_LBUTTONDOWN if is_popover_window(hwnd) => {
            // SAFETY: Category 8 (FFI boundary). Capture stays with the live popover
            // only for the matching slider/button release, so drags keep reporting.
            unsafe { SetCapture(hwnd) };
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PopoverPointerPressed(client_point(hwnd, lparam)),
            ));
            LRESULT(0)
        }
        WM_LBUTTONUP if is_popover_window(hwnd) => {
            // SAFETY: Category 8 (FFI boundary). This releases the bounded capture
            // acquired by the matching popover pointer press above.
            let _ = unsafe { ReleaseCapture() };
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PopoverPointer(client_point(hwnd, lparam)),
            ));
            LRESULT(0)
        }
        WM_MOUSEMOVE if is_preview_window(hwnd) => {
            track_mouse_leave(hwnd);
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PreviewPointerMoved(client_point(hwnd, lparam)),
            ));
            LRESULT(0)
        }
        WM_MOUSELEAVE if is_preview_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PreviewPointerMoved(DipPoint::new(-1.0, -1.0)),
            ));
            LRESULT(0)
        }
        WM_LBUTTONDOWN if is_preview_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PreviewPointerPressed(client_point(hwnd, lparam)),
            ));
            LRESULT(0)
        }
        WM_LBUTTONUP if is_preview_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PreviewPointerReleased(client_point(hwnd, lparam)),
            ));
            LRESULT(0)
        }
        WM_KILLFOCUS if is_preview_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PreviewDismissed,
            ));
            LRESULT(0)
        }
        WM_MOUSEMOVE if is_popover_window(hwnd) => {
            track_mouse_leave(hwnd);
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PopoverPointerMoved(client_point(hwnd, lparam)),
            ));
            LRESULT(0)
        }
        WM_MOUSELEAVE if is_popover_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PopoverPointerMoved(DipPoint::new(-1.0, -1.0)),
            ));
            LRESULT(0)
        }
        WM_KILLFOCUS if is_popover_window(hwnd) => {
            // SAFETY: Category 8 (FFI boundary). Releasing capture is idempotent
            // and prevents an interrupted slider drag from retaining the pointer.
            let _ = unsafe { ReleaseCapture() };
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DismissTransientOverlays,
            ));
            LRESULT(0)
        }
        WM_ACTIVATE if is_popover_window(hwnd) => {
            if let Some(event) = popover_activation_event(wparam.0 & 0xffff != WA_INACTIVE as usize)
            {
                queue_event(RoutedPlatformEvent::broadcast(event));
            }
            // SAFETY: Category 8 (FFI boundary). Default activation processing is
            // required after observing the unchanged popup activation message.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_KILLFOCUS if is_settings_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::SettingsKey(SettingsKey::Escape),
            ));
            LRESULT(0)
        }
        WM_KEYDOWN if is_popover_window(hwnd) => {
            if let Some(key) = popover_key(wparam) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::PopoverKey(key),
                ));
            }
            LRESULT(0)
        }
        WM_KEYDOWN if is_dock_window(hwnd) => {
            if let Some(key) = dock_key(wparam) {
                if key == DockKey::Escape && is_dragging(hwnd) {
                    release_dock_capture(hwnd);
                }
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::DockKey(key),
                ));
            }
            LRESULT(0)
        }
        WM_KEYDOWN if is_topbar_window(hwnd) => {
            if let Some(key) = topbar_key(wparam) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::TopbarKey(key),
                ));
            }
            LRESULT(0)
        }
        WM_KEYDOWN if is_settings_window(hwnd) => {
            if let Some(key) = settings_key(wparam) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::SettingsKey(key),
                ));
            }
            LRESULT(0)
        }
        WM_RBUTTONUP if is_dock_window(hwnd) => {
            let point = client_point(hwnd, lparam);
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DockContextMenuRequested { point },
            ));
            LRESULT(0)
        }
        WM_RBUTTONDOWN if is_dock_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DockPointer(DockPointerSample::new(
                    DockPointerPhase::Moved,
                    client_point(hwnd, lparam),
                )),
            ));
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::DismissTransientOverlays,
            ));
            LRESULT(0)
        }
        WM_RBUTTONDOWN if is_topbar_window(hwnd) => {
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::DismissTransientOverlays,
            ));
            LRESULT(0)
        }
        WM_DROPFILES if is_dock_window(hwnd) => {
            if let Some(path) = first_drop_path(wparam) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::DockDrop {
                        point: DipPoint::new(0.0, 0.0),
                        path,
                    },
                ));
            }
            LRESULT(0)
        }
        WM_NCHITTEST => hit_test(hwnd, lparam),
        windows::Win32::UI::WindowsAndMessaging::WM_WINDOWPOSCHANGED if is_topbar_window(hwnd) => {
            notify_window_position(hwnd);
            // SAFETY: Category 8 (FFI boundary). Default handling remains required
            // after the AppBar bookkeeping notification.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        windows::Win32::UI::WindowsAndMessaging::WM_ACTIVATE if is_topbar_window(hwnd) => {
            notify_activation(hwnd, wparam.0 & 0xffff != 0);
            // SAFETY: Category 8 (FFI boundary). Default activation processing
            // remains valid for the unchanged message parameters.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
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
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DpiChanged(rect_from_win32(recommended)),
            ));
            if is_topbar_window(hwnd) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::AppBarPositionChanged,
                ));
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::DisplayChanged,
            ));
            LRESULT(0)
        }
        WM_DEVICECHANGE => {
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::QuickSettingsRefresh(crate::RefreshScope::Devices),
            ));
            LRESULT(0)
        }
        WM_POWERBROADCAST if wparam.0 as u32 == PBT_APMRESUMEAUTOMATIC => {
            queue_event(RoutedPlatformEvent::broadcast(PlatformEvent::PowerResumed));
            LRESULT(1)
        }
        WM_TIMER if wparam.0 == TIMER_ID => {
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::QaExitRequested,
            ));
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == SYNC_TIMER_ID => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::SyncWindows,
            ));
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == DOCK_ANIMATION_TIMER_ID && is_dock_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DockAnimationFrame,
            ));
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == DOCK_EDGE_PROBE_TIMER_ID && is_dock_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DockEdgeProbe,
            ));
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == PREVIEW_TIMER_ID && is_dock_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PreviewTimer,
            ));
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == DRAG_ESCAPE_TIMER_ID && is_dock_window(hwnd) => {
            if is_dragging(hwnd) && escape_pressed() {
                cancel_dock_capture(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE if is_preview_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PreviewDismissed,
            ));
            LRESULT(0)
        }
        WM_CLOSE => {
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::CloseRequested,
            ));
            LRESULT(0)
        }
        WM_DESTROY => {
            set_dragging(hwnd, false);
            if LIVE_WINDOWS.fetch_sub(1, Ordering::AcqRel) == 1 {
                // SAFETY: Category 8 (FFI boundary). The final owned HWND has been
                // destroyed, so posting quit deterministically terminates this loop.
                unsafe { PostQuitMessage(0) };
            }
            queue_event(RoutedPlatformEvent::window(hwnd, PlatformEvent::Destroyed));
            LRESULT(0)
        }
        _ => {
            // SAFETY: Category 8 (FFI boundary). Unhandled messages and their exact
            // parameters are forwarded unchanged to the documented default callback.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}

const fn dock_capture_termination(message: u32, dragging: bool) -> Option<DockPointerPhase> {
    if dragging && (message == WM_CAPTURECHANGED || message == WM_CANCELMODE) {
        Some(DockPointerPhase::Cancelled)
    } else if !dragging && message == WM_MOUSELEAVE {
        Some(DockPointerPhase::Exited)
    } else {
        None
    }
}

fn is_dock_window(hwnd: HWND) -> bool {
    crate::win32::is_dock_window(hwnd)
}

fn is_topbar_window(hwnd: HWND) -> bool {
    crate::win32::is_topbar_window(hwnd)
}

fn is_popover_window(hwnd: HWND) -> bool {
    crate::win32::is_popover_window(hwnd)
}

fn is_preview_window(hwnd: HWND) -> bool {
    crate::win32::is_preview_window(hwnd)
}

fn is_settings_window(hwnd: HWND) -> bool {
    crate::win32::is_settings_window(hwnd)
}

fn queue_topbar_pointer(hwnd: HWND, phase: TopbarPointerPhase, point: DipPoint) {
    queue_event(RoutedPlatformEvent::window(
        hwnd,
        PlatformEvent::TopbarPointer(TopbarPointerSample::new(phase, point)),
    ));
}

fn release_dock_capture(hwnd: HWND) {
    end_dock_drag_tracking(hwnd);
    // SAFETY: Category 8 (FFI boundary). This UI thread established capture on
    // dock button-down; button-up and Escape are its local termination paths.
    let _ = unsafe { ReleaseCapture() };
}

fn end_dock_drag_tracking(hwnd: HWND) {
    set_dragging(hwnd, false);
    // SAFETY: Category 8 (FFI boundary). The numeric timer belongs to this dock
    // HWND and KillTimer is idempotent when capture already ended.
    let _ = unsafe {
        windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(hwnd), DRAG_ESCAPE_TIMER_ID)
    };
}

fn cancel_dock_capture(hwnd: HWND) {
    release_dock_capture(hwnd);
    queue_event(RoutedPlatformEvent::window(
        hwnd,
        PlatformEvent::DockPointer(DockPointerSample::new(
            DockPointerPhase::Cancelled,
            DipPoint::new(-1.0, -1.0),
        )),
    ));
}

fn escape_pressed() -> bool {
    // SAFETY: Category 8 (FFI boundary). GetAsyncKeyState reads process-global
    // keyboard state and does not dereference application memory.
    unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) < 0 }
}

fn popover_key(wparam: WPARAM) -> Option<PopoverKey> {
    let code = wparam.0 as u16;
    if code == VK_TAB.0 || code == VK_DOWN.0 {
        Some(PopoverKey::Next)
    } else if code == VK_UP.0 {
        Some(PopoverKey::Previous)
    } else if code == VK_RETURN.0 || code == VK_SPACE.0 {
        Some(PopoverKey::Activate)
    } else if code == VK_ESCAPE.0 {
        Some(PopoverKey::Escape)
    } else {
        None
    }
}

fn dock_key(wparam: WPARAM) -> Option<DockKey> {
    let code = wparam.0 as u16;
    if code == VK_TAB.0 || code == VK_DOWN.0 {
        Some(DockKey::Next)
    } else if code == VK_UP.0 {
        Some(DockKey::Previous)
    } else if code == VK_SPACE.0 {
        Some(DockKey::Preview)
    } else if code == VK_RETURN.0 {
        Some(DockKey::Activate)
    } else if code == VK_ESCAPE.0 {
        Some(DockKey::Escape)
    } else {
        None
    }
}

fn topbar_key(wparam: WPARAM) -> Option<TopbarKey> {
    let code = wparam.0 as u16;
    if code == VK_TAB.0 || code == VK_DOWN.0 {
        Some(TopbarKey::Next)
    } else if code == VK_UP.0 {
        Some(TopbarKey::Previous)
    } else if code == VK_RETURN.0 || code == VK_SPACE.0 {
        Some(TopbarKey::Activate)
    } else if code == VK_ESCAPE.0 {
        Some(TopbarKey::Escape)
    } else {
        None
    }
}

fn settings_key(wparam: WPARAM) -> Option<SettingsKey> {
    let code = wparam.0 as u16;
    if code == VK_TAB.0 || code == VK_DOWN.0 {
        Some(SettingsKey::Next)
    } else if code == VK_UP.0 {
        Some(SettingsKey::Previous)
    } else if code == VK_RETURN.0 || code == VK_SPACE.0 {
        Some(SettingsKey::Activate)
    } else if code == VK_ESCAPE.0 {
        Some(SettingsKey::Escape)
    } else if code == VK_LEFT.0 {
        Some(SettingsKey::MoveUp)
    } else if code == VK_RIGHT.0 {
        Some(SettingsKey::MoveDown)
    } else {
        None
    }
}

#[cfg(test)]
mod drag_capture_tests {
    use super::dock_capture_termination;
    use crate::DockPointerPhase;
    use windows::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows::Win32::UI::WindowsAndMessaging::{WM_CANCELMODE, WM_CAPTURECHANGED};

    #[test]
    fn capture_keeps_drag_alive_on_leave_and_cancels_on_native_termination() {
        assert_eq!(dock_capture_termination(WM_MOUSELEAVE, true), None);
        assert_eq!(
            dock_capture_termination(WM_MOUSELEAVE, false),
            Some(DockPointerPhase::Exited)
        );
        assert_eq!(
            dock_capture_termination(WM_CAPTURECHANGED, true),
            Some(DockPointerPhase::Cancelled)
        );
        assert_eq!(
            dock_capture_termination(WM_CANCELMODE, true),
            Some(DockPointerPhase::Cancelled)
        );
    }
}
