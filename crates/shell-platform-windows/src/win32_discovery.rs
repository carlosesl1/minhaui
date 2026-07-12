use std::path::Path;

use shell_core::{AppId, WindowId};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR, Result};

use crate::ObservedWindow;

pub(super) fn discover_running_windows(excluded: &[HWND]) -> Result<Vec<ObservedWindow>> {
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
    Some(ObservedWindow::new(
        WindowId::new(hwnd.0 as usize as u64),
        app,
        hwnd.0 as isize == state.foreground,
        {
            // SAFETY: Category 8 (FFI boundary). The HWND is from EnumWindows and
            // still valid for this synchronous minimized-state query.
            unsafe { IsIconic(hwnd) }.as_bool()
        },
    ))
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
