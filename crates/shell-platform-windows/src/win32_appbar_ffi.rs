use shell_renderer::PhysicalRect;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::Shell::{
    ABE_TOP, ABM_ACTIVATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, ABM_WINDOWPOSCHANGED,
    APPBARDATA, SHAppBarMessage,
};
use windows::core::Result;

pub(super) fn register_with_shell(hwnd: HWND) -> Result<()> {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: crate::win32_appbar::APPBAR_CALLBACK_MESSAGE,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). `data` has the documented size,
    // references the live topbar HWND, and remains writable for this call.
    if unsafe { SHAppBarMessage(ABM_NEW, &mut data) } == 0 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(-2_147_467_259),
            "Windows rejected the topbar AppBar registration",
        ));
    }
    Ok(())
}

pub(super) fn remove_from_shell(hwnd: HWND) {
    send_simple_message(hwnd, ABM_REMOVE, 0);
}

pub(super) fn reserve_top_edge(hwnd: HWND, requested: PhysicalRect) -> PhysicalRect {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uEdge: ABE_TOP,
        rc: rect_to_win32(requested),
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). The AppBar is registered, `data`
    // is initialized, and the Shell mutates it only during this call.
    unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut data) };
    data.rc.bottom = data.rc.top.saturating_add(requested.height.max(1));
    // SAFETY: Category 8 (FFI boundary). The queried rectangle and requested
    // thickness are valid screen coordinates for the registered top edge.
    unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) };
    rect_from_win32(data.rc)
}

pub(super) fn notify_window_position(hwnd: HWND) {
    send_simple_message(hwnd, ABM_WINDOWPOSCHANGED, 0);
}

pub(super) fn notify_activation(hwnd: HWND, active: bool) {
    send_simple_message(hwnd, ABM_ACTIVATE, if active { 1 } else { 0 });
}

fn send_simple_message(hwnd: HWND, message: u32, parameter: isize) {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        lParam: LPARAM(parameter),
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). The initialized data and live topbar
    // HWND satisfy the selected AppBar message contract for this call.
    unsafe { SHAppBarMessage(message, &mut data) };
}

const fn rect_to_win32(rect: PhysicalRect) -> RECT {
    let width = positive(rect.width);
    let height = positive(rect.height);
    RECT {
        left: rect.x,
        top: rect.y,
        right: rect.x.saturating_add(width),
        bottom: rect.y.saturating_add(height),
    }
}

const fn rect_from_win32(rect: RECT) -> PhysicalRect {
    let width = positive(rect.right.saturating_sub(rect.left));
    let height = positive(rect.bottom.saturating_sub(rect.top));
    PhysicalRect::new(rect.left, rect.top, width, height)
}

const fn positive(value: i32) -> i32 {
    if value > 0 { value } else { 1 }
}
