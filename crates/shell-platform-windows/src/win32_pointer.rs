use shell_renderer::{DipPoint, Dpi, PhysicalRect};
use windows::Win32::Foundation::{HWND, LPARAM, POINT};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DockCursorLocation {
    PhysicalBottom,
    Dock,
    ApproachCorridor,
    NearPhysicalBottom,
    Away,
}

pub(super) fn dock_cursor_location(
    hwnd: HWND,
    dock_rect: PhysicalRect,
) -> Option<DockCursorLocation> {
    let mut cursor = POINT::default();
    // SAFETY: Category 8 (FFI boundary). `cursor` is writable storage for the
    // synchronous global cursor-position query.
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return None;
    }
    let monitor = crate::win32_work_area::window_monitor_bounds(hwnd).ok()?;
    if crate::dock_edge_detection::cursor_hits_physical_bottom(monitor, cursor.x, cursor.y) {
        Some(DockCursorLocation::PhysicalBottom)
    } else if crate::dock_edge_detection::cursor_inside_rect(dock_rect, cursor.x, cursor.y) {
        Some(DockCursorLocation::Dock)
    } else if crate::dock_edge_detection::cursor_inside_approach_corridor(
        monitor, dock_rect, cursor.x, cursor.y,
    ) {
        Some(DockCursorLocation::ApproachCorridor)
    } else {
        // SAFETY: Category 8 (FFI boundary). The callback supplies a live HWND.
        let dpi = Dpi::from_raw(unsafe { GetDpiForWindow(hwnd) }.max(96));
        let threshold = (48.0 * dpi.scale()).round().max(1.0) as i32;
        if crate::dock_edge_detection::cursor_near_physical_bottom(
            monitor, cursor.x, cursor.y, threshold,
        ) {
            Some(DockCursorLocation::NearPhysicalBottom)
        } else {
            Some(DockCursorLocation::Away)
        }
    }
}

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
