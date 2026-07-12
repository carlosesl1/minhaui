use std::ffi::c_void;
use std::path::Path;

use shell_core::{AppId, Effect, WindowId};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowThreadProcessId, PostMessageW, SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL,
    SetForegroundWindow, ShowWindow, WM_CLOSE,
};
use windows::core::{PCWSTR, PWSTR, Result, w};

use crate::{PreviewQueuedAction, QueuedDockAction};

pub(super) fn apply_dock_actions(actions: &[QueuedDockAction]) -> Result<bool> {
    for action in actions {
        match action {
            QueuedDockAction::Launch(target) => launch(target)?,
            QueuedDockAction::Effect(effect) => apply_effect(effect)?,
            QueuedDockAction::Preview(action) => apply_preview_action(action.clone())?,
            QueuedDockAction::Quit => return Ok(false),
        }
    }
    Ok(true)
}

fn apply_preview_action(action: PreviewQueuedAction) -> Result<()> {
    match action {
        PreviewQueuedAction::Focus { window, app } => {
            if window_matches_app(window, &app) {
                focus(window);
            }
            Ok(())
        }
        PreviewQueuedAction::Close { window, app } => close_if_current(window, &app),
    }
}

fn apply_effect(effect: &Effect) -> Result<()> {
    match effect {
        Effect::Launch(app) => launch(app.as_str()),
        Effect::FocusWindow(window) => {
            focus(*window);
            Ok(())
        }
        Effect::MinimizeWindow(window) => {
            minimize(*window);
            Ok(())
        }
        Effect::PersistConfiguration | Effect::RebuildSurfaces | Effect::ApplyTaskbarPolicy(_) => {
            Ok(())
        }
    }
}

fn launch(target: &str) -> Result<()> {
    let target = shell_target(target);
    let wide = target.encode_utf16().chain([0]).collect::<Vec<_>>();
    // SAFETY: Category 8 (FFI boundary). Verb and show command are documented,
    // and the target buffer is null-terminated and live for the synchronous call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        Err(windows::core::Error::new(
            invalid_arg(),
            format!("ShellExecuteW failed with code {}", result.0 as isize),
        ))
    } else {
        Ok(())
    }
}

fn focus(window: WindowId) {
    let hwnd = hwnd_from_id(window);
    // SAFETY: Category 8 (FFI boundary). HWND identity comes from documented
    // platform discovery; invalid handles are ignored by Windows.
    let _ = unsafe { ShowWindow(hwnd, SW_RESTORE) };
    // SAFETY: Category 8 (FFI boundary). Foreground arbitration remains owned by
    // Windows; failure is non-fatal and observable by unchanged focus.
    let _ = unsafe { SetForegroundWindow(hwnd) };
}

fn minimize(window: WindowId) {
    let hwnd = hwnd_from_id(window);
    // SAFETY: Category 8 (FFI boundary). HWND identity comes from documented
    // platform discovery; invalid handles are ignored by Windows.
    let _ = unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
}

fn close_if_current(window: WindowId, app: &AppId) -> Result<()> {
    if !window_matches_app(window, app) {
        return Ok(());
    }
    let hwnd = hwnd_from_id(window);
    // SAFETY: Category 8 (FFI boundary). The HWND was discovered from Win32 and
    // posting WM_CLOSE is the documented non-blocking close request.
    unsafe { PostMessageW(Some(hwnd), WM_CLOSE, Default::default(), Default::default()) }
}

fn window_matches_app(window: WindowId, app: &AppId) -> bool {
    let hwnd = hwnd_from_id(window);
    window_app(hwnd).as_ref() == Some(app)
}

fn window_app(hwnd: HWND) -> Option<AppId> {
    let mut process = 0;
    // SAFETY: Category 8 (FFI boundary). The process-id out pointer is valid for
    // the duration of the call; invalid or recycled HWNDs yield no trusted app.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process)) };
    process_app(process)
}

fn process_app(process: u32) -> Option<AppId> {
    if process == 0 {
        return None;
    }
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

fn shell_target(target: &str) -> String {
    match target {
        "app.calculator" | "calculator" => "calc.exe".to_owned(),
        "app.notepad" | "notepad" => "notepad.exe".to_owned(),
        value => value.to_owned(),
    }
}

fn hwnd_from_id(window: WindowId) -> HWND {
    HWND(window.value() as usize as *mut c_void)
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}
