use std::sync::atomic::Ordering;

use shell_renderer::native::DESKTOP_BLUR_WAKE_MESSAGE;
use shell_renderer::{DipPoint, PhysicalRect};
use windows::Win32::Foundation::{
    E_UNEXPECTED, HANDLE, HWND, LPARAM, LRESULT, RECT, WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0,
    WAIT_TIMEOUT, WPARAM,
};
use windows::Win32::System::Threading::INFINITE;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, ReleaseCapture, SetCapture, VK_APPS, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_F10,
    VK_LEFT, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, GetMessageW, MSG, MWMO_INPUTAVAILABLE,
    MsgWaitForMultipleObjectsEx, PBT_APMRESUMEAUTOMATIC, PM_REMOVE, PeekMessageW, PostQuitMessage,
    QS_ALLINPUT, SWP_NOACTIVATE, SWP_NOZORDER, SetTimer, SetWindowPos, TranslateMessage,
    WA_INACTIVE, WM_ACTIVATE, WM_CANCELMODE, WM_CAPTURECHANGED, WM_CLOSE, WM_DESTROY,
    WM_DEVICECHANGE, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_DROPFILES, WM_GETOBJECT, WM_KEYDOWN,
    WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCHITTEST,
    WM_POWERBROADCAST, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SHOWWINDOW, WM_SIZE, WM_TIMER,
};
use windows::core::Result;

use crate::background_apps_worker::BACKGROUND_APPS_WAKE_MESSAGE;
use crate::brightness_worker::BRIGHTNESS_WAKE_MESSAGE;
use crate::dock_icon_worker::DOCK_ICON_WAKE_MESSAGE;
use crate::media_session_worker::MEDIA_SESSION_WAKE_MESSAGE;
use crate::night_light_worker::NIGHT_LIGHT_WAKE_MESSAGE;
use crate::quick_settings_worker::QUICK_SETTINGS_WAKE_MESSAGE;
use crate::shell_menu_worker::SHELL_MENU_WAKE_MESSAGE;
use crate::win32::{
    ACTIVATE_INSTANCE, DOCK_ANIMATION_TIMER_ID, DOCK_EDGE_PROBE_TIMER_ID, DRAG_ESCAPE_TIMER_ID,
    EXTERNAL_MENU_TIMER_ID, LIVE_WINDOWS, PREVIEW_TIMER_ID, SYNC_TIMER_ID, TASKBAR_CREATED,
    TIMER_ID,
};
use crate::win32_appbar::{is_position_notification, notify_activation, notify_window_position};
use crate::win32_drop::first_drop_path;
use crate::win32_event_queue::{
    RoutedPlatformEvent, is_dragging, next_event, queue_event, set_dragging,
};
use crate::win32_external_menu_events::EXTERNAL_MENU_WAKE_MESSAGE;
use crate::win32_hit_test::hit_test;
use crate::win32_pointer::{client_point, track_mouse_leave};
use crate::win32_settings_uia::SETTINGS_UIA_WAKE_MESSAGE;
pub(super) use crate::win32_work_area::{
    monitor_placement_inputs, window_monitor_bounds, window_monitor_id, window_work_area,
};
use crate::{
    DockKey, DockPointerPhase, DockPointerSample, PlatformEvent, PopoverKey, SettingsKey,
    TopbarKey, TopbarPointerPhase, TopbarPointerSample, popover_activation_event,
};

#[derive(Clone, Copy)]
pub(super) struct DockFrameWaitable {
    handle: HANDLE,
    hwnd: HWND,
}

impl DockFrameWaitable {
    pub(super) const fn new(handle: HANDLE, hwnd: HWND) -> Self {
        Self { handle, hwnd }
    }
}

pub(super) enum MessageLoopAction<'waitables> {
    CollectDockFrameWaitables(&'waitables mut Vec<DockFrameWaitable>),
    Dispatch(RoutedPlatformEvent),
}

const NATIVE_MESSAGE_BATCH_LIMIT: usize = 32;
const ROUTED_EVENT_BATCH_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MessageWaitWake {
    DockFrame(usize),
    Message,
    TimedOut,
    Failed,
    Unexpected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingMessageBatch {
    Empty,
    Drained,
    BudgetExhausted,
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DispatchBatch {
    Drained,
    BudgetExhausted,
    Stop,
}

pub(super) fn message_loop<F>(mut handle_action: F) -> Result<()>
where
    F: for<'waitables> FnMut(MessageLoopAction<'waitables>) -> Result<bool>,
{
    let mut message = MSG::default();
    let mut waitables = Vec::new();
    let mut handles = Vec::new();
    let mut routed_events_pending = false;
    loop {
        // Alternate bounded native and routed batches. Processing several native
        // messages first lets the routed queue coalesce mouse-move floods, while
        // both limits prevent either source from monopolizing the UI thread.
        let pending_messages = dispatch_pending_messages(&mut message);
        if pending_messages == PendingMessageBatch::Quit {
            return Ok(());
        }
        if pending_messages != PendingMessageBatch::Empty || routed_events_pending {
            match dispatch_routed_events(&mut handle_action)? {
                DispatchBatch::Drained => routed_events_pending = false,
                DispatchBatch::BudgetExhausted => routed_events_pending = true,
                DispatchBatch::Stop => return Ok(()),
            }
        }
        let work_pending =
            pending_messages == PendingMessageBatch::BudgetExhausted || routed_events_pending;

        waitables.clear();
        if !handle_action(MessageLoopAction::CollectDockFrameWaitables(&mut waitables))? {
            return Ok(());
        }
        if waitables.is_empty() {
            if work_pending {
                continue;
            }
            // SAFETY: Category 8 (FFI boundary). `message` is writable for the call
            // and no HWND filter means the idle UI thread sleeps until real work.
            let status = unsafe { GetMessageW(&mut message, None, 0, 0) };
            match status.0 {
                -1 => return Err(windows::core::Error::from_thread()),
                0 => return Ok(()),
                _ => {
                    dispatch_message(&message);
                    routed_events_pending = true;
                }
            }
            continue;
        }

        handles.clear();
        handles.reserve(waitables.len());
        handles.extend(waitables.iter().map(|waitable| waitable.handle));
        // SAFETY: Category 8 (FFI boundary). DXGI owns every borrowed handle for
        // the duration of this wait; the UI thread also wakes for queued messages.
        let wake = unsafe {
            MsgWaitForMultipleObjectsEx(
                Some(handles.as_slice()),
                if work_pending { 0 } else { INFINITE },
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        match classify_message_wait_wake(wake, waitables.len()) {
            MessageWaitWake::DockFrame(index) => {
                let target = waitables[index];
                if !handle_action(MessageLoopAction::Dispatch(RoutedPlatformEvent::window(
                    target.hwnd,
                    PlatformEvent::DockAnimationFrame,
                )))? {
                    return Ok(());
                }
                routed_events_pending = true;
            }
            MessageWaitWake::Message => {}
            MessageWaitWake::TimedOut => {}
            MessageWaitWake::Failed => return Err(windows::core::Error::from_thread()),
            MessageWaitWake::Unexpected => {
                return Err(windows::core::Error::new(
                    E_UNEXPECTED,
                    "unexpected native message wait result",
                ));
            }
        }
    }
}

fn dispatch_pending_messages(message: &mut MSG) -> PendingMessageBatch {
    let mut dispatched = 0;
    while dispatched < NATIVE_MESSAGE_BATCH_LIMIT {
        // SAFETY: Category 8 (FFI boundary). `message` is writable, and each call
        // removes at most one message owned by the current UI thread.
        if !unsafe { PeekMessageW(message, None, 0, 0, PM_REMOVE) }.as_bool() {
            return if dispatched == 0 {
                PendingMessageBatch::Empty
            } else {
                PendingMessageBatch::Drained
            };
        }
        if message.message == WM_QUIT {
            return PendingMessageBatch::Quit;
        }
        dispatch_message(message);
        dispatched += 1;
    }
    PendingMessageBatch::BudgetExhausted
}

fn dispatch_message(message: &MSG) {
    // SAFETY: Category 8 (FFI boundary). The message was initialized by
    // PeekMessageW/GetMessageW and remains valid through synchronous dispatch.
    let _ = unsafe { TranslateMessage(message) };
    // SAFETY: The same initialized-message invariant applies here.
    unsafe { DispatchMessageW(message) };
}

fn dispatch_routed_events<F>(handle_action: &mut F) -> Result<DispatchBatch>
where
    F: for<'waitables> FnMut(MessageLoopAction<'waitables>) -> Result<bool>,
{
    dispatch_batch(ROUTED_EVENT_BATCH_LIMIT, next_event, |event| {
        handle_action(MessageLoopAction::Dispatch(event))
    })
}

fn dispatch_batch<T>(
    limit: usize,
    mut next: impl FnMut() -> Option<T>,
    mut dispatch: impl FnMut(T) -> Result<bool>,
) -> Result<DispatchBatch> {
    debug_assert!(limit > 0);
    for _ in 0..limit {
        let Some(event) = next() else {
            return Ok(DispatchBatch::Drained);
        };
        if !dispatch(event)? {
            return Ok(DispatchBatch::Stop);
        }
    }
    Ok(DispatchBatch::BudgetExhausted)
}

const fn classify_message_wait_wake(wake: WAIT_EVENT, frame_count: usize) -> MessageWaitWake {
    if wake.0 == WAIT_FAILED.0 {
        return MessageWaitWake::Failed;
    }
    if wake.0 == WAIT_TIMEOUT.0 {
        return MessageWaitWake::TimedOut;
    }
    let offset = wake.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
    if offset < frame_count {
        MessageWaitWake::DockFrame(offset)
    } else if offset == frame_count {
        MessageWaitWake::Message
    } else {
        MessageWaitWake::Unexpected
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
    let activate_instance = ACTIVATE_INSTANCE.load(Ordering::Acquire);
    if activate_instance != 0 && message == activate_instance {
        queue_event(RoutedPlatformEvent::window(
            hwnd,
            PlatformEvent::ActivateExistingInstance,
        ));
        return LRESULT(1);
    }
    if message == TASKBAR_CREATED.load(Ordering::Acquire) {
        queue_event(RoutedPlatformEvent::broadcast(
            PlatformEvent::TaskbarCreated,
        ));
        return LRESULT(0);
    }
    if message == WM_GETOBJECT
        && is_settings_window(hwnd)
        && let Some(result) = crate::win32_settings_uia::handle_wm_getobject(hwnd, wparam, lparam)
    {
        return result;
    }
    if message == BACKGROUND_APPS_WAKE_MESSAGE
        || message == MEDIA_SESSION_WAKE_MESSAGE
        || message == NIGHT_LIGHT_WAKE_MESSAGE
        || message == BRIGHTNESS_WAKE_MESSAGE
        || message == EXTERNAL_MENU_WAKE_MESSAGE
        || message == SHELL_MENU_WAKE_MESSAGE
        || message == QUICK_SETTINGS_WAKE_MESSAGE
        || message == DOCK_ICON_WAKE_MESSAGE
    {
        return LRESULT(0);
    }
    if message == DESKTOP_BLUR_WAKE_MESSAGE && is_popover_window(hwnd) {
        queue_event(RoutedPlatformEvent::window(
            hwnd,
            PlatformEvent::DesktopBlurReady,
        ));
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
        WM_SHOWWINDOW if is_popover_window(hwnd) && wparam.0 == 0 => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::DesktopBlurPrefetch,
            ));
            // SAFETY: Default visibility bookkeeping still receives the original
            // pointer-free message after the prefetch hint has been queued.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_MOUSEWHEEL if is_popover_window(hwnd) => {
            let delta = ((wparam.0 >> 16) as u16) as i16;
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PopoverScroll(-isize::from(delta.signum())),
            ));
            LRESULT(0)
        }
        WM_MOUSEWHEEL if is_settings_window(hwnd) => {
            let delta = ((wparam.0 >> 16) as u16) as i16;
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::SettingsScroll(-isize::from(delta.signum())),
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
        WM_RBUTTONDOWN if is_popover_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PopoverContextPressed(client_point(hwnd, lparam)),
            ));
            LRESULT(0)
        }
        WM_RBUTTONUP if is_popover_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::PopoverContextRequested(client_point(hwnd, lparam)),
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
        WM_MOUSEMOVE if is_app_menu_window(hwnd) => {
            track_mouse_leave(hwnd);
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::AppMenuPointerMoved(client_point(hwnd, lparam)),
            ));
            LRESULT(0)
        }
        WM_MOUSELEAVE if is_app_menu_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::AppMenuPointerMoved(DipPoint::new(-1.0, -1.0)),
            ));
            LRESULT(0)
        }
        WM_LBUTTONUP if is_app_menu_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::AppMenuPointerReleased(client_point(hwnd, lparam)),
            ));
            LRESULT(0)
        }
        WM_KILLFOCUS if is_app_menu_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::AppMenuDismissed,
            ));
            LRESULT(0)
        }
        WM_ACTIVATE if is_app_menu_window(hwnd) => {
            if wparam.0 & 0xffff == WA_INACTIVE as usize {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::AppMenuDismissed,
                ));
            }
            // SAFETY: Category 8 (FFI boundary). Default activation processing is
            // retained after observing the nonmodal tool window's state.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
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
        WM_LBUTTONUP if is_settings_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::SettingsPointerActivated(client_point(hwnd, lparam)),
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
        WM_KEYDOWN if is_app_menu_window(hwnd) => {
            if let Some(key) = popover_key(wparam) {
                queue_event(RoutedPlatformEvent::window(
                    hwnd,
                    PlatformEvent::AppMenuKey(key),
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
        WM_NCHITTEST if is_settings_window(hwnd) => {
            // SAFETY: Settings is a standard resizable top-level window; default
            // non-client hit testing owns its caption, resize borders and buttons.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_NCHITTEST => hit_test(hwnd, lparam),
        WM_SIZE if is_settings_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::SettingsResized,
            ));
            LRESULT(0)
        }
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
        WM_TIMER if wparam.0 == EXTERNAL_MENU_TIMER_ID => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::ExternalMenuTimer,
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
        WM_CLOSE if is_app_menu_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::AppMenuDismissed,
            ));
            LRESULT(0)
        }
        WM_CLOSE if is_settings_window(hwnd) => {
            queue_event(RoutedPlatformEvent::window(
                hwnd,
                PlatformEvent::SettingsKey(SettingsKey::Dismiss),
            ));
            LRESULT(0)
        }
        WM_CLOSE => {
            queue_event(RoutedPlatformEvent::broadcast(
                PlatformEvent::CloseRequested,
            ));
            LRESULT(0)
        }
        SETTINGS_UIA_WAKE_MESSAGE if is_settings_window(hwnd) => LRESULT(0),
        WM_DESTROY => {
            set_dragging(hwnd, false);
            if is_settings_window(hwnd) {
                crate::win32_settings_uia::unregister(hwnd);
            }
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

fn is_app_menu_window(hwnd: HWND) -> bool {
    crate::win32::is_app_menu_window(hwnd)
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
    } else if code == VK_APPS.0 || (code == VK_F10.0 && shift_pressed()) {
        Some(PopoverKey::ContextMenu)
    } else if code == VK_ESCAPE.0 {
        Some(PopoverKey::Escape)
    } else {
        None
    }
}

fn shift_pressed() -> bool {
    // SAFETY: Category 8 (FFI boundary). GetAsyncKeyState only reads the
    // process-global keyboard state and does not dereference application memory.
    unsafe { GetAsyncKeyState(VK_SHIFT.0 as i32) < 0 }
}

fn control_pressed() -> bool {
    // SAFETY: GetAsyncKeyState only reads process-global keyboard state.
    unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 }
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
    if code == VK_RETURN.0 && control_pressed() {
        Some(SettingsKey::Apply)
    } else if code == VK_TAB.0 && shift_pressed() {
        Some(SettingsKey::Previous)
    } else if code == VK_TAB.0 || code == VK_DOWN.0 {
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
    use std::collections::VecDeque;

    use crate::DockPointerPhase;
    use windows::Win32::Foundation::{WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows::Win32::UI::WindowsAndMessaging::{WM_CANCELMODE, WM_CAPTURECHANGED};

    use super::{
        DispatchBatch, MessageWaitWake, classify_message_wait_wake, dispatch_batch,
        dock_capture_termination,
    };

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

    #[test]
    fn message_wait_classifies_each_dock_handle_before_the_message_queue() {
        assert_eq!(
            classify_message_wait_wake(WAIT_OBJECT_0, 2),
            MessageWaitWake::DockFrame(0)
        );
        assert_eq!(
            classify_message_wait_wake(WAIT_EVENT(WAIT_OBJECT_0.0 + 1), 2),
            MessageWaitWake::DockFrame(1)
        );
        assert_eq!(
            classify_message_wait_wake(WAIT_EVENT(WAIT_OBJECT_0.0 + 2), 2),
            MessageWaitWake::Message
        );
    }

    #[test]
    fn message_wait_rejects_failed_and_out_of_range_results() {
        assert_eq!(
            classify_message_wait_wake(WAIT_FAILED, 1),
            MessageWaitWake::Failed
        );
        assert_eq!(
            classify_message_wait_wake(WAIT_EVENT(WAIT_OBJECT_0.0 + 3), 1),
            MessageWaitWake::Unexpected
        );
        assert_eq!(
            classify_message_wait_wake(WAIT_TIMEOUT, 1),
            MessageWaitWake::TimedOut
        );
    }

    #[test]
    fn routed_dispatch_budget_preserves_remaining_fifo_work() {
        let mut queued = VecDeque::from([1_u8, 2, 3, 4]);
        let mut handled = Vec::new();

        let first = dispatch_batch(
            2,
            || queued.pop_front(),
            |event| {
                handled.push(event);
                Ok(true)
            },
        )
        .expect("first batch");
        assert_eq!(first, DispatchBatch::BudgetExhausted);
        assert_eq!(handled, vec![1, 2]);
        assert_eq!(queued, VecDeque::from([3, 4]));

        let second = dispatch_batch(
            4,
            || queued.pop_front(),
            |event| {
                handled.push(event);
                Ok(true)
            },
        )
        .expect("second batch");
        assert_eq!(second, DispatchBatch::Drained);
        assert_eq!(handled, vec![1, 2, 3, 4]);
        assert!(queued.is_empty());
    }

    #[test]
    fn routed_dispatch_stop_does_not_consume_later_work() {
        let mut queued = VecDeque::from([1_u8, 2, 3]);
        let mut handled = Vec::new();

        let result = dispatch_batch(
            8,
            || queued.pop_front(),
            |event| {
                handled.push(event);
                Ok(event != 2)
            },
        )
        .expect("stopped batch");

        assert_eq!(result, DispatchBatch::Stop);
        assert_eq!(handled, vec![1, 2]);
        assert_eq!(queued, VecDeque::from([3]));
    }
}
