use shell_core::{AppId, Effect, ShellState, WindowId};
use std::ffi::c_void;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow, WM_CLOSE,
};
use windows::core::{PCWSTR, Result, w};

use crate::{PreviewQueuedAction, QueuedDockAction};

pub(super) fn apply_dock_actions(actions: &[QueuedDockAction], state: &ShellState) -> Result<bool> {
    if actions.iter().any(|action| {
        matches!(
            action,
            QueuedDockAction::Effect(Effect::PersistConfiguration)
        )
    }) {
        crate::win32_config::persist_dock_state(state)?;
    }
    for action in actions {
        match action {
            QueuedDockAction::Launch(target) => launch_or_log(target),
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
                focus_window(window);
            }
            Ok(())
        }
        PreviewQueuedAction::Close { window, app } => close_if_current(window, &app),
    }
}

fn apply_effect(effect: &Effect) -> Result<()> {
    match effect {
        Effect::Launch(app) => {
            launch_or_log(app.as_str());
            Ok(())
        }
        Effect::FocusWindow(window) => {
            focus_window(*window);
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
    let target = crate::win32_app_identity::resolve_launch_target(target).ok_or_else(|| {
        windows::core::Error::new(invalid_arg(), "launch target is not trusted or registered")
    })?;
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

fn launch_or_log(target: &str) {
    let diagnostic_target = diagnostic_launch_target(target);
    match launch(target) {
        Ok(()) => crate::diagnostics::record(
            crate::diagnostics::DiagnosticModule::DockLaunch,
            crate::diagnostics::LogLevel::Info,
            "dock.launch.succeeded",
            &[("target", diagnostic_target)],
        ),
        Err(error) => {
            let message = error.to_string();
            crate::diagnostics::record(
                crate::diagnostics::DiagnosticModule::DockLaunch,
                crate::diagnostics::LogLevel::Error,
                "dock.launch.failed",
                &[("target", diagnostic_target), ("error", &message)],
            );
        }
    }
}

fn diagnostic_launch_target(target: &str) -> &'static str {
    if target.eq_ignore_ascii_case("taskmgr.exe") {
        "task-manager"
    } else if matches!(target, "app.calculator" | "calculator" | "calc.exe") {
        "calculator"
    } else if matches!(target, "app.notepad" | "notepad" | "notepad.exe") {
        "notepad"
    } else {
        "custom"
    }
}

pub(super) fn focus_window(window: WindowId) {
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
    window_app(hwnd)
        .as_ref()
        .is_some_and(|window_app| crate::dock_window_sync::app_ids_match(window_app, app))
}

fn window_app(hwnd: HWND) -> Option<AppId> {
    crate::win32_app_identity::app_for_window(hwnd).map(|identity| identity.app().clone())
}

fn hwnd_from_id(window: WindowId) -> HWND {
    HWND(window.value() as usize as *mut c_void)
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}

#[cfg(test)]
mod tests {
    use shell_core::{AppId, ShellState, WindowId};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        IsWindow, MSG, PM_REMOVE, PeekMessageW, RegisterClassW, TranslateMessage, UnregisterClassW,
        WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };
    use windows::core::{PCWSTR, Result, w};

    use crate::{PreviewQueuedAction, QueuedDockAction};

    use super::{
        apply_dock_actions, apply_preview_action, diagnostic_launch_target, window_matches_app,
    };

    const CLASS_NAME: PCWSTR = w!("MinhaUi.NativeShell.ActionTestWindow.v1");

    #[test]
    fn failed_application_launch_keeps_the_dock_running() {
        // Given: an application target that the trusted resolver rejects.
        let actions = [QueuedDockAction::Launch(
            "missing-relative-app.exe".to_owned(),
        )];

        // When: the native action boundary attempts the launch.
        let outcome = apply_dock_actions(&actions, &ShellState::default());

        // Then: the failure is handled without terminating the shell loop.
        assert!(outcome.is_ok_and(|keep_running| keep_running));
    }

    #[test]
    fn launch_diagnostics_never_persist_custom_paths() {
        assert_eq!(diagnostic_launch_target("taskmgr.exe"), "task-manager");
        assert_eq!(
            diagnostic_launch_target(r"C:\Users\Carlos\Private\secret.exe"),
            "custom"
        );
    }

    #[test]
    fn preview_focus_and_close_actions_use_native_window_input_path() -> Result<()> {
        // Given: a safe Win32 test window owned by the current test process.
        let window = TestWindow::create()?;
        let app = current_test_app()?;
        let id = WindowId::new(window.hwnd.0 as usize as u64);
        assert!(window_matches_app(id, &app));

        // When: preview focus and close actions are applied through native dispatch.
        apply_preview_action(PreviewQueuedAction::Focus {
            window: id,
            app: app.clone(),
        })?;
        apply_preview_action(PreviewQueuedAction::Close { window: id, app })?;
        pump_until_closed(window.hwnd);

        // Then: WM_CLOSE reached the safe window and destroyed it.
        // SAFETY: Category 8 (FFI boundary). The HWND value is the test window
        // handle; IsWindow only queries handle liveness.
        assert!(!unsafe { IsWindow(Some(window.hwnd)) }.as_bool());
        Ok(())
    }

    struct TestWindow {
        instance: HINSTANCE,
        hwnd: HWND,
    }

    impl TestWindow {
        fn create() -> Result<Self> {
            // SAFETY: Category 8 (FFI boundary). Passing no module name returns the
            // loaded test executable module for class registration.
            let module = unsafe { GetModuleHandleW(None) }?;
            let instance = HINSTANCE(module.0);
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(test_window_proc),
                hInstance: instance,
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            // SAFETY: Category 8 (FFI boundary). The class structure is fully
            // initialized and static strings outlive the registration.
            unsafe { RegisterClassW(&class) };
            // SAFETY: Category 8 (FFI boundary). The registered class and static
            // title are valid; no application pointer crosses the API.
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    CLASS_NAME,
                    w!("Task6 Safe Action Window"),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    320,
                    200,
                    None,
                    None,
                    Some(instance),
                    None,
                )
            }?;
            Ok(Self { instance, hwnd })
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: Category 8 (FFI boundary). This call targets a window owned
            // by this test guard and tolerates an already-destroyed HWND.
            let _ = unsafe { DestroyWindow(self.hwnd) };
            // SAFETY: Category 8 (FFI boundary). The class was registered by this
            // test guard and unregister tolerates an already-removed class.
            let _ = unsafe { UnregisterClassW(CLASS_NAME, Some(self.instance)) };
        }
    }

    fn current_test_app() -> Result<AppId> {
        let exe = std::env::current_exe()
            .map_err(|error| windows::core::Error::new(super::invalid_arg(), error.to_string()))?;
        let file = exe
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| windows::core::Error::new(super::invalid_arg(), "missing test exe"))?
            .to_ascii_lowercase();
        AppId::parse(&file)
            .map_err(|error| windows::core::Error::new(super::invalid_arg(), error.to_string()))
    }

    fn pump_until_closed(hwnd: HWND) {
        let mut message = MSG::default();
        for _ in 0..16 {
            // SAFETY: Category 8 (FFI boundary). The message storage is writable
            // and the filter is limited to the test HWND.
            while unsafe { PeekMessageW(&mut message, Some(hwnd), 0, 0, PM_REMOVE) }.as_bool() {
                // SAFETY: Category 8 (FFI boundary). The message was initialized by
                // PeekMessageW and is translated synchronously.
                let _ = unsafe { TranslateMessage(&message) };
                // SAFETY: Category 8 (FFI boundary). The message was initialized by
                // PeekMessageW and is dispatched synchronously.
                unsafe { DispatchMessageW(&message) };
            }
            // SAFETY: Category 8 (FFI boundary). The HWND is only queried for liveness.
            if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
                return;
            }
            std::thread::yield_now();
        }
    }

    unsafe extern "system" fn test_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // SAFETY: Category 8 (FFI boundary). The test callback forwards every
        // message unchanged to the documented default window procedure.
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }
}
