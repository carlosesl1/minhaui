use shell_core::MonitorId;
use shell_renderer::{Dpi, PhysicalRect};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromWindow,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::core::{BOOL, Result};

use crate::MonitorPlacementInput;

pub(super) fn window_work_area(hwnd: windows::Win32::Foundation::HWND) -> Result<PhysicalRect> {
    // SAFETY: Category 8 (FFI boundary). The default-nearest flag gives a monitor
    // for owned shell HWNDs even during display topology changes.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    monitor_work_area(monitor)
}

pub(super) fn window_monitor_id(hwnd: windows::Win32::Foundation::HWND) -> MonitorId {
    // SAFETY: Category 8 (FFI boundary). The default-nearest flag gives a stable
    // monitor handle for a live or recently moved shell HWND.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    MonitorId::new(monitor.0 as usize as u64)
}

pub(super) fn monitor_placement_inputs() -> Result<Vec<MonitorPlacementInput>> {
    let mut monitors = Vec::new();
    // SAFETY: Category 8 (FFI boundary). The mutable vector pointer remains valid
    // for the synchronous EnumDisplayMonitors traversal.
    if !unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor),
            LPARAM(&mut monitors as *mut _ as isize),
        )
    }
    .as_bool()
    {
        return Err(windows::core::Error::from_thread());
    }
    Ok(monitors)
}

unsafe extern "system" fn enum_monitor(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: Category 8 (FFI boundary). `data` was created from `&mut
    // Vec<MonitorPlacementInput>` and EnumDisplayMonitors is synchronous.
    let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorPlacementInput>) };
    if let Ok(input) = monitor_input(monitor) {
        monitors.push(input);
    }
    true.into()
}

fn monitor_input(monitor: HMONITOR) -> Result<MonitorPlacementInput> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). `info` is valid writable storage with
    // the documented size field set for this monitor query.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(windows::core::Error::from_thread());
    }
    let mut dpi_x = 96;
    let mut dpi_y = 96;
    // SAFETY: Category 8 (FFI boundary). The monitor handle comes from GDI and
    // the DPI out pointers are valid for the synchronous call.
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    Ok(MonitorPlacementInput::new(
        MonitorId::new(monitor.0 as usize as u64),
        rect_from_win32(info.rcMonitor),
        rect_from_win32(info.rcWork),
        Dpi::from_raw(dpi_x.max(dpi_y).max(96)),
    ))
}

fn monitor_work_area(monitor: HMONITOR) -> Result<PhysicalRect> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). `info` is valid writable storage with
    // the documented size field set for this monitor query.
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
