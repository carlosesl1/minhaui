use std::path::Path;

use shell_core::AppId;
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW,
    RegOpenKeyExW,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, PWSTR, w};

use crate::background_apps::{
    BackgroundAppEntry, MAX_NOTIFICATION_REGISTRATIONS, NotificationRegistration, RunningProcess,
    build_background_apps, merge_background_apps,
};
use crate::win32_tray_source::{
    NativeTrayCapture, NativeTrayCaptureOutcome, capture_native_tray_apps,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum BackgroundAppsError {
    #[error("notification registry unavailable")]
    Registry,
    #[error("process snapshot unavailable")]
    ProcessSnapshot,
    #[error("application activation unavailable")]
    Activation,
}

impl BackgroundAppsError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Registry => "registry_unavailable",
            Self::ProcessSnapshot => "process_snapshot_unavailable",
            Self::Activation => "activation_unavailable",
        }
    }
}

pub(super) fn activate_background_app(
    entry: &BackgroundAppEntry,
    windows: &[crate::ObservedWindow],
) -> Result<(), BackgroundAppsError> {
    let executable_name = Path::new(entry.executable())
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(BackgroundAppsError::Activation)?;
    let app = AppId::parse(executable_name).map_err(|_| BackgroundAppsError::Activation)?;
    if let Some(window) = windows.iter().find(|window| window.matches_app(&app)) {
        crate::win32_actions::focus_window(window.window());
        return Ok(());
    }

    let current_path = process_image_path(entry.process_id())
        .filter(|path| path.eq_ignore_ascii_case(entry.executable()))
        .ok_or(BackgroundAppsError::Activation)?;
    let wide = current_path.encode_utf16().chain([0]).collect::<Vec<_>>();
    // SAFETY: Category 8 (FFI boundary). The target is a null-terminated path
    // re-resolved from the same live process at click time and remains valid for
    // the synchronous shell request.
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
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(BackgroundAppsError::Activation)
    }
}

pub(super) fn open_background_app_location(
    entry: &BackgroundAppEntry,
) -> Result<(), BackgroundAppsError> {
    let current_path = process_image_path(entry.process_id())
        .filter(|path| path.eq_ignore_ascii_case(entry.executable()))
        .ok_or(BackgroundAppsError::Activation)?;
    let mut parameters = String::from("/select,\"");
    parameters.push_str(&current_path);
    parameters.push('"');
    let parameters = parameters.encode_utf16().chain([0]).collect::<Vec<_>>();
    // SAFETY: Category 8 (FFI boundary). Both strings are null-terminated values
    // derived from the revalidated live process path and remain valid for this
    // synchronous shell request.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("explorer.exe"),
            PCWSTR(parameters.as_ptr()),
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(BackgroundAppsError::Activation)
    }
}

pub(super) fn capture_background_apps(
    generation: u64,
) -> Result<Vec<BackgroundAppEntry>, BackgroundAppsError> {
    capture_background_apps_with(generation, capture_native_tray_apps, || {
        let registrations = capture_notification_registrations()?;
        let processes = capture_running_processes()?;
        Ok(build_background_apps(&registrations, &processes))
    })
}

fn capture_background_apps_with(
    generation: u64,
    capture_native: impl FnOnce(u64) -> NativeTrayCapture,
    capture_fallback: impl FnOnce() -> Result<Vec<BackgroundAppEntry>, BackgroundAppsError>,
) -> Result<Vec<BackgroundAppEntry>, BackgroundAppsError> {
    let native = capture_native(generation);
    if let Some(diagnostic) = native_capture_diagnostic(native.outcome) {
        crate::diagnostics::record(
            crate::diagnostics::DiagnosticModule::AppLifecycle,
            crate::diagnostics::LogLevel::Info,
            diagnostic.event,
            &[("code", diagnostic.code)],
        );
    }
    match capture_fallback() {
        Ok(fallback) => Ok(merge_background_apps(native.entries, fallback)),
        Err(error) if native.entries.is_empty() => Err(error),
        Err(error) => {
            crate::diagnostics::record(
                crate::diagnostics::DiagnosticModule::AppLifecycle,
                crate::diagnostics::LogLevel::Info,
                "background_apps.fallback_capture.unavailable",
                &[("code", error.code())],
            );
            Ok(merge_background_apps(native.entries, []))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeCaptureDiagnostic {
    event: &'static str,
    code: &'static str,
}

const fn native_capture_diagnostic(
    outcome: NativeTrayCaptureOutcome,
) -> Option<NativeCaptureDiagnostic> {
    match outcome {
        NativeTrayCaptureOutcome::Complete => None,
        NativeTrayCaptureOutcome::Partial(_) => Some(NativeCaptureDiagnostic {
            event: "background_apps.native_capture.partial",
            code: outcome.code(),
        }),
        NativeTrayCaptureOutcome::Unavailable(_) => Some(NativeCaptureDiagnostic {
            event: "background_apps.native_capture.unavailable",
            code: outcome.code(),
        }),
    }
}

fn capture_notification_registrations() -> Result<Vec<NotificationRegistration>, BackgroundAppsError>
{
    let mut raw_key = HKEY::default();
    // SAFETY: Category 8 (FFI boundary). The output points to initialized storage;
    // access is read-only and the returned key is owned by `RegistryKey`.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Control Panel\\NotifyIconSettings"),
            None,
            KEY_READ,
            &mut raw_key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(BackgroundAppsError::Registry);
    }
    let key = RegistryKey(raw_key);
    let mut registrations = Vec::new();

    for index in 0..MAX_NOTIFICATION_REGISTRATIONS as u32 {
        let mut name = [0_u16; 256];
        let mut name_len = (name.len() - 1) as u32;
        // SAFETY: Category 8 (FFI boundary). `name` and `name_len` are valid for
        // the synchronous read and the key guard remains alive for the call.
        let status = unsafe {
            RegEnumKeyExW(
                key.0,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };
        if status == ERROR_NO_MORE_ITEMS {
            break;
        }
        if status != ERROR_SUCCESS || name_len == 0 {
            continue;
        }
        name[name_len as usize] = 0;
        let subkey = PCWSTR(name.as_ptr());
        let Some(executable) = read_registry_string(key.0, subkey, w!("ExecutablePath")) else {
            continue;
        };
        let tooltip = read_registry_string(key.0, subkey, w!("InitialTooltip")).unwrap_or_default();
        registrations.push(NotificationRegistration::new(&executable, &tooltip));
    }

    Ok(registrations)
}

fn read_registry_string(key: HKEY, subkey: PCWSTR, value: PCWSTR) -> Option<String> {
    let mut byte_len = 0_u32;
    // SAFETY: Category 8 (FFI boundary). This first call only requests the size;
    // the registry key and string pointers remain valid for the call.
    let status = unsafe {
        RegGetValueW(
            key,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut byte_len),
        )
    };
    if status != ERROR_SUCCESS || byte_len < 2 {
        return None;
    }

    let mut buffer = vec![0_u16; (byte_len as usize).div_ceil(2)];
    // SAFETY: Category 8 (FFI boundary). The byte capacity reported by the first
    // call fits the allocated UTF-16 buffer and `byte_len` is writable.
    let status = unsafe {
        RegGetValueW(
            key,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut byte_len),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let units = (byte_len as usize / 2).min(buffer.len());
    let content_len = buffer[..units]
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(units);
    (content_len > 0).then(|| String::from_utf16_lossy(&buffer[..content_len]))
}

fn capture_running_processes() -> Result<Vec<RunningProcess>, BackgroundAppsError> {
    // SAFETY: Category 8 (FFI boundary). The snapshot is read-only and ownership
    // transfers immediately to `OwnedHandle` for exactly-once closure.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map(OwnedHandle)
        .map_err(|_| BackgroundAppsError::ProcessSnapshot)?;
    // SAFETY: Category 8 (FFI boundary). This parameterless query returns the
    // caller's process identifier and does not borrow any native memory.
    let current_process = unsafe { GetCurrentProcessId() };
    let current_session =
        session_id(current_process).ok_or(BackgroundAppsError::ProcessSnapshot)?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). `entry` has the documented size and is
    // valid writable storage while the snapshot guard is alive.
    if unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_err() {
        return Ok(Vec::new());
    }

    let mut processes = Vec::new();
    loop {
        let process_id = entry.th32ProcessID;
        if process_id != 0
            && session_id(process_id) == Some(current_session)
            && let Some(executable) = process_image_path(process_id)
        {
            processes.push(RunningProcess::new(process_id, &executable, ""));
        }

        // SAFETY: Category 8 (FFI boundary). The same initialized record and live
        // snapshot are reused until Windows reports the end of enumeration.
        if unsafe { Process32NextW(snapshot.0, &mut entry) }.is_err() {
            break;
        }
    }
    Ok(processes)
}

fn session_id(process_id: u32) -> Option<u32> {
    let mut session = 0_u32;
    // SAFETY: Category 8 (FFI boundary). `session` is valid writable storage and
    // the query does not open or mutate the target process.
    unsafe { ProcessIdToSessionId(process_id, &mut session) }
        .ok()
        .map(|()| session)
}

fn process_image_path(process_id: u32) -> Option<String> {
    // SAFETY: Category 8 (FFI boundary). Limited query access reads only the image
    // path and the returned handle transfers to `OwnedHandle`.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .ok()
        .map(OwnedHandle)?;
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    // SAFETY: Category 8 (FFI boundary). The writable buffer and capacity pointer
    // remain valid for the synchronous query while the process handle is alive.
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .ok()?;
    (length > 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        // SAFETY: Category 2 (resource cleanup). This guard exclusively owns the
        // key returned by RegOpenKeyExW and closes it exactly once.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: Category 2 (resource cleanup). This guard exclusively owns a
        // successful snapshot/process handle and closes it exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use crate::NativeWindowId;
    use crate::background_apps::{BackgroundAppEntry, BackgroundAppOrigin, MAX_BACKGROUND_APPS};
    use crate::native_tray::NativeTrayIdentity;
    use crate::win32_tray_source::{
        NativeTrayCapture, NativeTrayCaptureError, NativeTrayCaptureOutcome,
    };

    use super::{
        BackgroundAppsError, NativeCaptureDiagnostic, capture_background_apps_with,
        native_capture_diagnostic,
    };

    #[test]
    fn adapter_error_is_typed_and_path_free() {
        let error = BackgroundAppsError::Registry;

        assert_eq!(error.code(), "registry_unavailable");
        assert!(!format!("{error:?}").contains("Carlos"));
    }

    fn fallback_entry() -> BackgroundAppEntry {
        BackgroundAppEntry::registry_fallback(
            42,
            "Discord fallback",
            r"C:\Apps\Discord.exe",
            r"C:\Apps\Discord.exe",
        )
    }

    fn native_entry(generation: u64) -> BackgroundAppEntry {
        let identity =
            NativeTrayIdentity::new(NativeWindowId::new(7), 42, 9, 0x8001, 0, None, generation)
                .expect("synthetic native identity");
        BackgroundAppEntry::native(
            identity,
            "Discord native",
            r"c:/apps/discord.exe",
            r"c:/apps/discord.exe",
        )
    }

    #[test]
    fn unavailable_native_capture_preserves_registry_fallback() {
        for error in [
            NativeTrayCaptureError::AccessDenied,
            NativeTrayCaptureError::Unsupported,
        ] {
            let result = capture_background_apps_with(
                91,
                |_| NativeTrayCapture {
                    entries: Vec::new(),
                    outcome: NativeTrayCaptureOutcome::Unavailable(error),
                },
                || Ok(vec![fallback_entry()]),
            )
            .expect("fallback must remain available");

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].label(), "Discord fallback");
            assert!(matches!(
                result[0].origin(),
                BackgroundAppOrigin::RegistryFallback
            ));
        }
    }

    #[test]
    fn capture_passes_generation_and_native_entry_wins_registry_duplicate() {
        let captured_generation = Cell::new(0_u64);

        let result = capture_background_apps_with(
            92,
            |generation| {
                captured_generation.set(generation);
                NativeTrayCapture {
                    entries: vec![native_entry(generation)],
                    outcome: NativeTrayCaptureOutcome::Complete,
                }
            },
            || Ok(vec![fallback_entry()]),
        )
        .expect("injected capture");

        assert_eq!(captured_generation.get(), 92);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label(), "Discord native");
        assert!(matches!(result[0].origin(), BackgroundAppOrigin::Native(_)));
    }

    #[test]
    fn fallback_error_returns_bounded_native_entries() {
        let generation = 93;
        let native_entries = (0..MAX_BACKGROUND_APPS + 7)
            .map(|index| {
                let identity = NativeTrayIdentity::new(
                    NativeWindowId::new(index as isize + 1),
                    42,
                    index as u32 + 1,
                    0x8001,
                    0,
                    None,
                    generation,
                )
                .expect("synthetic native identity");
                let executable = format!(r"C:\Apps\Native{index:02}.exe");
                BackgroundAppEntry::native(
                    identity,
                    format!("Native {index:02}"),
                    executable.clone(),
                    executable,
                )
            })
            .collect::<Vec<_>>();

        let result = capture_background_apps_with(
            generation,
            |_| NativeTrayCapture {
                entries: native_entries,
                outcome: NativeTrayCaptureOutcome::Partial(NativeTrayCaptureError::HostUnavailable),
            },
            || Err(BackgroundAppsError::Registry),
        )
        .expect("native entries must survive a fallback failure");

        assert_eq!(result.len(), MAX_BACKGROUND_APPS);
        assert!(
            result
                .iter()
                .all(|entry| matches!(entry.origin(), BackgroundAppOrigin::Native(_)))
        );
    }

    #[test]
    fn empty_unavailable_native_capture_preserves_fallback_error() {
        let result = capture_background_apps_with(
            94,
            |_| NativeTrayCapture {
                entries: Vec::new(),
                outcome: NativeTrayCaptureOutcome::Unavailable(NativeTrayCaptureError::Unsupported),
            },
            || Err(BackgroundAppsError::ProcessSnapshot),
        );

        assert_eq!(result, Err(BackgroundAppsError::ProcessSnapshot));
    }

    #[test]
    fn native_capture_diagnostic_is_closed_redacted_and_skips_complete_outcomes() {
        assert_eq!(
            native_capture_diagnostic(NativeTrayCaptureOutcome::Complete),
            None
        );
        assert_eq!(
            native_capture_diagnostic(NativeTrayCaptureOutcome::Partial(
                NativeTrayCaptureError::AccessDenied,
            )),
            Some(NativeCaptureDiagnostic {
                event: "background_apps.native_capture.partial",
                code: "native_tray_access_denied",
            })
        );
        assert_eq!(
            native_capture_diagnostic(NativeTrayCaptureOutcome::Unavailable(
                NativeTrayCaptureError::Unsupported,
            )),
            Some(NativeCaptureDiagnostic {
                event: "background_apps.native_capture.unavailable",
                code: "native_tray_unsupported",
            })
        );
    }
}
