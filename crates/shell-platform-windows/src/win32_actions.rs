use std::ffi::c_void;

use shell_core::{Effect, WindowId};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow,
};
use windows::core::{PCWSTR, Result, w};

use crate::QueuedDockAction;

pub(super) fn apply_dock_actions(actions: &[QueuedDockAction]) -> Result<bool> {
    for action in actions {
        match action {
            QueuedDockAction::Launch(target) => launch(target)?,
            QueuedDockAction::Effect(effect) => apply_effect(effect)?,
            QueuedDockAction::Quit => return Ok(false),
        }
    }
    Ok(true)
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
