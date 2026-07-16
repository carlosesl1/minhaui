use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, PROPERTYKEY};
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::StructuredStorage::PropVariantToString;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetWindowThreadProcessId};
use windows::core::{BOOL, GUID, PWSTR};

const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    pid: 5,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WindowIdentity {
    application_user_model_id: Option<String>,
    process_image_path: Option<String>,
}

impl WindowIdentity {
    #[cfg(test)]
    fn new(application_user_model_id: Option<&str>, process_image_path: Option<&str>) -> Self {
        Self {
            application_user_model_id: application_user_model_id.map(str::to_owned),
            process_image_path: process_image_path.map(str::to_owned),
        }
    }

    pub(super) fn application_user_model_id(&self) -> Option<&str> {
        self.application_user_model_id.as_deref()
    }

    pub(super) fn process_image_path(&self) -> Option<&str> {
        self.process_image_path.as_deref()
    }
}

pub(super) fn effective_window_identity(hwnd: HWND) -> WindowIdentity {
    let owner = window_identity(hwnd);
    if owner.application_user_model_id().is_some() {
        return owner;
    }

    let mut search = ChildIdentitySearch::default();
    // SAFETY: Category 8 (FFI boundary). `search` remains live for the complete
    // synchronous enumeration and the callback restores this exact pointer type.
    let completed = unsafe {
        EnumChildWindows(
            Some(hwnd),
            Some(enum_child_identity),
            LPARAM(&mut search as *mut _ as isize),
        )
    }
    .as_bool();
    if !completed && search.identity.is_none() {
        return owner;
    }
    search.identity.unwrap_or(owner)
}

#[derive(Default)]
struct ChildIdentitySearch {
    identity: Option<WindowIdentity>,
}

unsafe extern "system" fn enum_child_identity(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: Category 8 (FFI boundary). `lparam` was created from a live mutable
    // ChildIdentitySearch and EnumChildWindows invokes this callback synchronously.
    let search = unsafe { &mut *(lparam.0 as *mut ChildIdentitySearch) };
    let identity = window_identity(hwnd);
    if identity.application_user_model_id().is_some() {
        search.identity = Some(identity);
        return false.into();
    }
    true.into()
}

#[cfg(test)]
fn preferred_identity(
    owner: WindowIdentity,
    children: impl IntoIterator<Item = WindowIdentity>,
) -> WindowIdentity {
    children
        .into_iter()
        .find(|identity| identity.application_user_model_id().is_some())
        .unwrap_or(owner)
}

fn window_identity(hwnd: HWND) -> WindowIdentity {
    let process = process_id(hwnd);
    WindowIdentity {
        application_user_model_id: window_application_user_model_id(hwnd)
            .or_else(|| application_user_model_id(process)),
        process_image_path: process_image_path(process),
    }
}

fn window_application_user_model_id(hwnd: HWND) -> Option<String> {
    // SAFETY: Category 8 (FFI boundary). The HWND is live for the synchronous
    // property-store query and the interface guard owns the returned reference.
    let store: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(hwnd) }.ok()?;
    // SAFETY: Category 8 (FFI boundary). The static property key is valid and
    // the returned PROPVARIANT owns its value until this function returns.
    let value = unsafe { store.GetValue(&PKEY_APP_USER_MODEL_ID) }.ok()?;
    let mut buffer = [0_u16; 512];
    // SAFETY: Category 8 (FFI boundary). The PROPVARIANT is live and the bounded
    // UTF-16 buffer is writable for the synchronous conversion.
    unsafe { PropVariantToString(&value, &mut buffer) }.ok()?;
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    (length > 0).then(|| String::from_utf16_lossy(&buffer[..length]))
}

fn process_id(hwnd: HWND) -> u32 {
    let mut process = 0;
    // SAFETY: Category 8 (FFI boundary). The process-id out pointer is valid for
    // this read-only query against a live HWND.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process)) };
    process
}

fn process_image_path(process: u32) -> Option<String> {
    if process == 0 {
        return None;
    }
    // SAFETY: Category 8 (FFI boundary). Limited query access is sufficient for
    // reading the process image path and does not mutate the target process.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) }.ok()?;
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    // SAFETY: Category 8 (FFI boundary). The buffer and its capacity pointer are
    // valid for the synchronous process image-name query.
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    // SAFETY: Category 8 (FFI boundary). This function owns the process handle
    // returned by OpenProcess and closes it exactly once.
    let _ = unsafe { CloseHandle(handle) };
    result.ok()?;
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}

fn application_user_model_id(process: u32) -> Option<String> {
    if process == 0 {
        return None;
    }
    // SAFETY: Category 8 (FFI boundary). Limited query access reads packaged
    // process metadata without mutating the process.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) }.ok()?;
    let mut length = 0_u32;
    // SAFETY: Category 8 (FFI boundary). A null output buffer is the documented
    // size-query form and `length` is valid writable storage.
    let size_result = unsafe { GetApplicationUserModelId(handle, &mut length, None) };
    if size_result.0 != 122 || length == 0 {
        // SAFETY: Category 8 (FFI boundary). This function owns the process handle.
        let _ = unsafe { CloseHandle(handle) };
        return None;
    }
    let mut buffer = vec![0_u16; length as usize];
    // SAFETY: Category 8 (FFI boundary). The writable buffer exactly matches the
    // capacity requested by the preceding size query.
    let result =
        unsafe { GetApplicationUserModelId(handle, &mut length, Some(PWSTR(buffer.as_mut_ptr()))) };
    // SAFETY: Category 8 (FFI boundary). This function owns the process handle.
    let _ = unsafe { CloseHandle(handle) };
    if result.0 != 0 {
        return None;
    }
    let content_length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..content_length]))
}

#[cfg(test)]
mod tests {
    use super::{WindowIdentity, preferred_identity};

    #[test]
    fn hosted_packaged_window_prefers_the_child_application_identity() {
        let owner = WindowIdentity::new(None, Some("ApplicationFrameHost.exe"));
        let calculator = WindowIdentity::new(
            Some("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"),
            Some("CalculatorApp.exe"),
        );

        assert_eq!(preferred_identity(owner, [calculator.clone()]), calculator);
    }

    #[test]
    fn native_window_keeps_its_owner_identity() {
        let owner = WindowIdentity::new(None, Some("notepad.exe"));
        let child = WindowIdentity::new(None, Some("render-host.exe"));

        assert_eq!(preferred_identity(owner.clone(), [child]), owner);
    }
}
