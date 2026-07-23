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
    build_background_apps,
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

pub(super) fn capture_background_apps() -> Result<Vec<BackgroundAppEntry>, BackgroundAppsError> {
    let registrations = capture_notification_registrations()?;
    let processes = capture_running_processes()?;
    Ok(build_background_apps(&registrations, &processes))
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
    use super::BackgroundAppsError;

    #[test]
    fn adapter_error_is_typed_and_path_free() {
        let error = BackgroundAppsError::Registry;

        assert_eq!(error.code(), "registry_unavailable");
        assert!(!format!("{error:?}").contains("Carlos"));
    }
}
