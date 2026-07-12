use std::sync::atomic::Ordering;

use shell_renderer::{DipPoint, PhysicalRect};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_DOWN, VK_ESCAPE, VK_RETURN, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, GetMessageW, MSG, PBT_APMRESUMEAUTOMATIC, PostQuitMessage,
    SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, TranslateMessage, WM_CLOSE, WM_COMMAND, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_DROPFILES, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_NCHITTEST, WM_POWERBROADCAST, WM_RBUTTONUP, WM_TIMER,
};
use windows::core::Result;

use crate::win32::{LIVE_WINDOWS, SYNC_TIMER_ID, TASKBAR_CREATED, TIMER_ID};
use crate::win32_context_menu::track_dock_context_menu;
use crate::win32_drop::first_drop_path;
use crate::win32_event_queue::{
    RoutedPlatformEvent, is_dragging, last_context_point, next_event, queue_event, set_dragging,
    set_last_context_point,
};
use crate::win32_hit_test::hit_test;
use crate::win32_pointer::{
    client_physical_point, client_point, cursor_client_point, track_mouse_leave,
};
pub(super) use crate::win32_work_area::{
    monitor_placement_inputs, window_monitor_id, window_work_area,
};
use crate::{
    ContextMenuCommand, DockPointerPhase, DockPointerSample, PlatformEvent, PopoverKey,
    TopbarPointerPhase, TopbarPointerSample,
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
    match message {
        WM_MOUSEMOVE if is_dock_window(hwnd) => {
            track_mouse_leave(hwnd);
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
            set_dragging(hwnd, false);
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DockPointer(DockPointerSample::new(
                    DockPointerPhase::Exited,
                    DipPoint::new(-1.0, -1.0),
                )),
            ));
            LRESULT(0)
        }
        WM_LBUTTONDOWN if is_dock_window(hwnd) => {
            set_dragging(hwnd, true);
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DockPointer(DockPointerSample::new(
                    DockPointerPhase::Pressed,
                    client_point(hwnd, lparam),
                )),
            ));
            LRESULT(0)
        }
        WM_LBUTTONUP if is_dock_window(hwnd) => {
            set_dragging(hwnd, false);
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
        WM_KEYDOWN if is_popover_window(hwnd) => {
            if let Some(key) = popover_key(wparam) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::PopoverKey(key),
                ));
            }
            LRESULT(0)
        }
        WM_KEYDOWN if is_settings_window(hwnd) => {
            if let Some(key) = popover_key(wparam) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::SettingsKey(key),
                ));
            }
            LRESULT(0)
        }
        WM_RBUTTONUP if is_dock_window(hwnd) => {
            let point = client_point(hwnd, lparam);
            set_last_context_point(point);
            if let Some(command) = track_dock_context_menu(hwnd, client_physical_point(lparam)) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::DockContextMenu { point, command },
                ));
            }
            LRESULT(0)
        }
        WM_COMMAND if is_dock_window(hwnd) => {
            if let Some(command) = ContextMenuCommand::from_native_id((wparam.0 & 0xffff) as u16) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::DockContextMenu {
                        point: cursor_client_point(hwnd)
                            .or_else(last_context_point)
                            .unwrap_or(DipPoint::new(0.0, 0.0)),
                        command,
                    },
                ));
            }
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
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::DisplayChanged,
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
        WM_CLOSE => {
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::CloseRequested,
            ));
            LRESULT(0)
        }
        WM_DESTROY => {
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

fn is_dock_window(hwnd: HWND) -> bool {
    crate::win32::is_dock_window(hwnd)
}

fn is_topbar_window(hwnd: HWND) -> bool {
    crate::win32::is_topbar_window(hwnd)
}

fn is_popover_window(hwnd: HWND) -> bool {
    crate::win32::is_popover_window(hwnd)
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
