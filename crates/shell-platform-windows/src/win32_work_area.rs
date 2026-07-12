use shell_renderer::PhysicalRect;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
};
use windows::core::Result;

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

const fn rect_from_win32(rect: RECT) -> PhysicalRect {
    PhysicalRect::new(
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )
}
