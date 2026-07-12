use std::path::Path;

use shell_core::{AppId, MonitorId, WindowId};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
    GetWindowRect, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR, Result};

use crate::{FullscreenObservation, ObservedWindow, PreviewCapture};

pub(super) fn discover_running_windows(
    excluded: &[HWND],
    preview_host: Option<HWND>,
) -> Result<Vec<ObservedWindow>> {
    let mut state = EnumState {
        excluded: excluded.iter().map(|hwnd| hwnd.0 as isize).collect(),
        foreground: {
            // SAFETY: Category 8 (FFI boundary). Foreground HWND lookup has no
            // parameters and returns null when no window owns foreground.
            unsafe { GetForegroundWindow().0 as isize }
        },
        current_process: {
            // SAFETY: Category 8 (FFI boundary). The call has no parameters and
            // returns the current process identifier.
            unsafe { GetCurrentProcessId() }
        },
        preview_host,
        windows: Vec::new(),
    };
    // SAFETY: Category 8 (FFI boundary). `state` lives for the whole synchronous
    // enumeration call and the callback only casts the lparam back to this type.
    unsafe { EnumWindows(Some(enum_window), LPARAM(&mut state as *mut _ as isize)) }?;
    Ok(state.windows)
}

struct EnumState {
    excluded: Vec<isize>,
    foreground: isize,
    current_process: u32,
    preview_host: Option<HWND>,
    windows: Vec<ObservedWindow>,
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: Category 8 (FFI boundary). `lparam` was created from `&mut
    // EnumState` in `discover_running_windows` and EnumWindows is synchronous.
    let state = unsafe { &mut *(lparam.0 as *mut EnumState) };
    if let Some(window) = observed_window(hwnd, state) {
        state.windows.push(window);
    }
    true.into()
}

fn observed_window(hwnd: HWND, state: &EnumState) -> Option<ObservedWindow> {
    if state.excluded.contains(&(hwnd.0 as isize)) || !eligible_window(hwnd) {
        return None;
    }
    let process = process_id(hwnd)?;
    if process == state.current_process {
        return None;
    }
    let app = process_app(process)?;
    let window_id = WindowId::new(hwnd.0 as usize as u64);
    let observed = ObservedWindow::new(window_id, app, hwnd.0 as isize == state.foreground, {
        // SAFETY: Category 8 (FFI boundary). The HWND is from EnumWindows and
        // still valid for this synchronous minimized-state query.
        unsafe { IsIconic(hwnd) }.as_bool()
    });
    Some(match state.preview_host {
        Some(host) => observed.with_preview(probe_preview(host, hwnd)),
        None => observed,
    })
    .map(|window| match fullscreen_observation(hwnd, window_id) {
        Some(fullscreen) => window.with_fullscreen(fullscreen),
        None => window,
    })
}

fn probe_preview(host: HWND, source: HWND) -> PreviewCapture {
    crate::win32_preview::probe_dwm_thumbnail(host, source)
}

fn eligible_window(hwnd: HWND) -> bool {
    // SAFETY: Category 8 (FFI boundary). The HWND is supplied by EnumWindows and
    // is valid during this callback.
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return false;
    }
    // SAFETY: Category 8 (FFI boundary). Owner lookup is read-only for this live
    // HWND; an owner means this is not a top-level app window.
    if unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok() {
        return false;
    }
    // SAFETY: Category 8 (FFI boundary). Reads the extended style from the live
    // HWND so tool windows can be filtered.
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
        return false;
    }
    // SAFETY: Category 8 (FFI boundary). Length query is read-only and returns 0
    // for untitled surfaces, which are not useful dock app windows.
    (unsafe { GetWindowTextLengthW(hwnd) }) > 0
}

fn process_id(hwnd: HWND) -> Option<u32> {
    let mut process = 0;
    // SAFETY: Category 8 (FFI boundary). The process-id out pointer is valid for
    // the duration of the call and the HWND came from EnumWindows.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process)) };
    (process != 0).then_some(process)
}

fn process_app(process: u32) -> Option<AppId> {
    // SAFETY: Category 8 (FFI boundary). PROCESS_QUERY_LIMITED_INFORMATION is a
    // documented read-only access right for process image-name queries.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) }.ok()?;
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    // SAFETY: Category 8 (FFI boundary). The buffer is writable and `length`
    // points to its capacity on input and receives the UTF-16 length on output.
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    // SAFETY: Category 8 (FFI boundary). The handle was returned by OpenProcess
    // in this function and is closed exactly once.
    let _ = unsafe { CloseHandle(handle) };
    result.ok()?;
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    let file = Path::new(&path).file_name()?.to_str()?.to_ascii_lowercase();
    AppId::parse(&file).ok()
}

fn fullscreen_observation(hwnd: HWND, window: WindowId) -> Option<FullscreenObservation> {
    let mut window_rect = RECT::default();
    // SAFETY: Category 8 (FFI boundary). The HWND is supplied by EnumWindows and
    // `window_rect` is valid writable storage for the synchronous query.
    unsafe { GetWindowRect(hwnd, &mut window_rect) }.ok()?;
    // SAFETY: Category 8 (FFI boundary). The default-nearest flag gives the
    // monitor containing the enumerated HWND.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). `info` has the documented size field
    // and valid writable storage for this monitor query.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    (window_rect.left == info.rcMonitor.left
        && window_rect.top == info.rcMonitor.top
        && window_rect.right == info.rcMonitor.right
        && window_rect.bottom == info.rcMonitor.bottom)
        .then_some(FullscreenObservation::new(
            window,
            MonitorId::new(monitor.0 as usize as u64),
            rect_from_win32(info.rcMonitor),
        ))
}

const fn rect_from_win32(rect: RECT) -> shell_renderer::PhysicalRect {
    shell_renderer::PhysicalRect::new(
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )
}
