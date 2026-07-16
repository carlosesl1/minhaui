use std::path::Path;

use shell_core::{AppId, WindowId};
use windows::Win32::Foundation::HWND;
use windows::Win32::Storage::FileSystem::SearchPathW;
use windows::Win32::System::SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW};
use windows::Win32::UI::Shell::{
    ASSOCF_OPEN_BYEXENAME, ASSOCSTR_EXECUTABLE, AssocQueryStringW, IShellItem,
    SHCreateItemFromParsingName,
};
use windows::core::{PCWSTR, PWSTR};

use crate::dock_window_sync::canonical_app_id;

trait AppDiscoveryAdapter {
    fn package_icon_source(&self, identifier: &str) -> Option<String>;
    fn appsfolder_source(&self, aumid: &str) -> Option<String>;
    fn executable_path(&self, target: &str) -> Option<String>;
    fn registered_executable(&self, target: &str) -> Option<String>;
    fn package_icon_source_for_executable(&self, target: &str) -> Option<String>;
}

struct WindowsAppDiscovery;

impl AppDiscoveryAdapter for WindowsAppDiscovery {
    fn package_icon_source(&self, identifier: &str) -> Option<String> {
        crate::win32_package_icon::package_icon_source(identifier)
    }

    fn appsfolder_source(&self, aumid: &str) -> Option<String> {
        appsfolder_source(aumid)
    }

    fn executable_path(&self, target: &str) -> Option<String> {
        resolve_executable_path(target)
    }

    fn registered_executable(&self, target: &str) -> Option<String> {
        registered_executable(target)
    }

    fn package_icon_source_for_executable(&self, target: &str) -> Option<String> {
        crate::win32_package_icon::package_icon_source_for_executable(target)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedAppIdentity {
    app: AppId,
    aliases: Vec<AppId>,
}

impl ResolvedAppIdentity {
    pub(crate) const fn app(&self) -> &AppId {
        &self.app
    }

    pub(crate) fn aliases(&self) -> &[AppId] {
        &self.aliases
    }
}

pub(crate) fn app_for_window(hwnd: HWND) -> Option<ResolvedAppIdentity> {
    let identity = crate::win32_window_identity::effective_window_identity(hwnd);
    let prefer_aumid = identity
        .application_user_model_id()
        .is_some_and(|aumid| appsfolder_source(aumid).is_some());
    resolved_app_identity(
        identity.application_user_model_id(),
        identity.process_image_path(),
        prefer_aumid,
    )
}

fn resolved_app_identity(
    application_user_model_id: Option<&str>,
    process_image_path: Option<&str>,
    prefer_aumid: bool,
) -> Option<ResolvedAppIdentity> {
    let executable = process_image_path
        .and_then(|path| Path::new(path).file_name())
        .and_then(|file| file.to_str())
        .map(canonical_app_id)
        .and_then(|value| AppId::parse(&value).ok());
    let aumid = application_user_model_id
        .map(canonical_app_id)
        .and_then(|value| AppId::parse(&value).ok());
    let app = if prefer_aumid {
        aumid.clone().or_else(|| executable.clone())
    } else {
        executable.clone().or_else(|| aumid.clone())
    }?;
    let aliases = [aumid, executable]
        .into_iter()
        .flatten()
        .filter(|alias| alias != &app)
        .collect();
    Some(ResolvedAppIdentity { app, aliases })
}

pub(crate) fn window_icon_source(window: WindowId) -> Option<String> {
    let hwnd = HWND(window.value() as usize as *mut std::ffi::c_void);
    let identity = crate::win32_window_identity::effective_window_identity(hwnd);
    identity
        .application_user_model_id()
        .and_then(|aumid| {
            crate::win32_package_icon::package_icon_source(aumid)
                .or_else(|| appsfolder_source(aumid))
        })
        .or_else(|| identity.process_image_path().map(str::to_owned))
}

pub(crate) fn resolve_icon_source(target: &str) -> Option<String> {
    resolve_icon_source_with(&WindowsAppDiscovery, target)
}

fn resolve_icon_source_with(discovery: &impl AppDiscoveryAdapter, target: &str) -> Option<String> {
    match target.to_ascii_lowercase().as_str() {
        "app.calculator" | "calculator" | "calc.exe" | "calculatorapp.exe" => {
            let aumid = "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
            discovery
                .package_icon_source(aumid)
                .or_else(|| discovery.appsfolder_source(aumid))
        }
        value if value.starts_with("shell:appsfolder\\") => Some(target.to_owned()),
        _ => discovery
            .package_icon_source(target)
            .or_else(|| discovery.appsfolder_source(target))
            .or_else(|| discovery.executable_path(target))
            .or_else(|| discovery.registered_executable(target))
            .or_else(|| discovery.package_icon_source_for_executable(target)),
    }
}

pub(crate) fn resolve_executable_path(target: &str) -> Option<String> {
    let target = shell_target(target);
    let direct = Path::new(&target);
    if direct.is_file() {
        return Some(direct.to_string_lossy().into_owned());
    }
    let wide = target.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut buffer = vec![0_u16; 32_768];
    // SAFETY: Category 8 (FFI boundary). The target is null-terminated and the
    // writable output buffer remains live for the synchronous search.
    let length =
        unsafe { SearchPathW(None, PCWSTR(wide.as_ptr()), None, Some(&mut buffer), None) } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..length]))
}

fn appsfolder_source(aumid: &str) -> Option<String> {
    if aumid.is_empty()
        || aumid.contains(['\\', '/', ':', '\0'])
        || Path::new(aumid).components().count() != 1
    {
        return None;
    }
    let source = format!("shell:AppsFolder\\{aumid}");
    let wide = source.encode_utf16().chain([0]).collect::<Vec<_>>();
    // SAFETY: Category 8 (FFI boundary). The parsing name is a live,
    // null-terminated UTF-16 buffer; Windows owns the returned COM interface.
    let _: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()? };
    Some(source)
}

pub(crate) fn shell_target(target: &str) -> String {
    match target {
        "app.calculator" | "calculator" | "calc.exe" | "calculatorapp.exe" => {
            "shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".to_owned()
        }
        "app.notepad" | "notepad" => "notepad.exe".to_owned(),
        value => value.to_owned(),
    }
}

pub(crate) fn resolve_launch_target(target: &str) -> Option<String> {
    if target.contains('\0') {
        return None;
    }
    match target.to_ascii_lowercase().as_str() {
        "app.calculator" | "calculator" | "calc.exe" | "calculatorapp.exe" => {
            Some("shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".to_owned())
        }
        "app.notepad" | "notepad" | "notepad.exe" => {
            trusted_directory_executable(GetSystemDirectoryW, "notepad.exe")
        }
        "taskmgr.exe" => trusted_directory_executable(GetSystemDirectoryW, "taskmgr.exe"),
        "explorer" | "explorer.exe" => {
            trusted_directory_executable(GetWindowsDirectoryW, "explorer.exe")
        }
        value if value.starts_with("shell:appsfolder\\") => Some(target.to_owned()),
        _ => {
            if let Some(source) = appsfolder_source(target) {
                return Some(source);
            }
            let direct = Path::new(target);
            if direct.is_absolute() && direct.is_file() {
                Some(direct.to_string_lossy().into_owned())
            } else if direct.components().count() == 1 {
                registered_executable(target)
            } else {
                None
            }
        }
    }
}

fn trusted_directory_executable(
    directory: unsafe fn(Option<&mut [u16]>) -> u32,
    file_name: &str,
) -> Option<String> {
    let mut buffer = vec![0_u16; 32_768];
    // SAFETY: Category 8 (FFI boundary). The selected Windows directory API
    // writes at most the provided UTF-16 buffer and returns its used length.
    let length = unsafe { directory(Some(&mut buffer)) } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    let path = Path::new(&String::from_utf16_lossy(&buffer[..length])).join(file_name);
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

fn registered_executable(file_name: &str) -> Option<String> {
    let association = file_name.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut output = vec![0_u16; 32_768];
    let mut length = output.len() as u32;
    // SAFETY: Category 8 (FFI boundary). Both UTF-16 buffers are bounded and
    // null-terminated where required; App Paths returns into owned storage.
    unsafe {
        AssocQueryStringW(
            ASSOCF_OPEN_BYEXENAME,
            ASSOCSTR_EXECUTABLE,
            PCWSTR(association.as_ptr()),
            PCWSTR::null(),
            Some(PWSTR(output.as_mut_ptr())),
            &mut length,
        )
    }
    .ok()
    .ok()?;
    let used = output
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(length as usize)
        .min(output.len());
    let path = Path::new(&String::from_utf16_lossy(&output[..used])).to_path_buf();
    (path.is_absolute() && path.is_file()).then(|| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "native-validation")]
    use std::path::Path;

    #[cfg(feature = "native-validation")]
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

    use super::{
        AppDiscoveryAdapter, canonical_app_id, resolve_icon_source_with, resolved_app_identity,
    };
    #[cfg(feature = "native-validation")]
    use super::{resolve_executable_path, resolve_icon_source, resolve_launch_target};

    #[cfg(feature = "native-validation")]
    struct ComApartment;

    #[cfg(feature = "native-validation")]
    impl ComApartment {
        fn initialize() -> Self {
            // SAFETY: Category 8 (FFI boundary). The apartment lifetime is
            // balanced by this guard's CoUninitialize call on the same thread.
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .expect("COM apartment initialization failed");
            Self
        }
    }

    #[cfg(feature = "native-validation")]
    impl Drop for ComApartment {
        fn drop(&mut self) {
            // SAFETY: Balances the successful initialization on this thread.
            unsafe { CoUninitialize() };
        }
    }

    struct FixedAppDiscovery;

    impl AppDiscoveryAdapter for FixedAppDiscovery {
        fn package_icon_source(&self, identifier: &str) -> Option<String> {
            (identifier == "Vendor.App!Main").then(|| "image:package-logo.png".to_owned())
        }

        fn appsfolder_source(&self, aumid: &str) -> Option<String> {
            (aumid == "Vendor.Fallback!Main")
                .then(|| "shell:AppsFolder\\Vendor.Fallback!Main".to_owned())
        }

        fn executable_path(&self, target: &str) -> Option<String> {
            (target == "native.exe").then(|| r"C:\Windows\native.exe".to_owned())
        }

        fn registered_executable(&self, _target: &str) -> Option<String> {
            None
        }

        fn package_icon_source_for_executable(&self, _target: &str) -> Option<String> {
            None
        }
    }

    #[test]
    fn icon_source_resolution_uses_injected_discovery_in_fallback_order() {
        let discovery = FixedAppDiscovery;

        assert_eq!(
            resolve_icon_source_with(&discovery, "Vendor.App!Main").as_deref(),
            Some("image:package-logo.png")
        );
        assert_eq!(
            resolve_icon_source_with(&discovery, "Vendor.Fallback!Main").as_deref(),
            Some("shell:AppsFolder\\Vendor.Fallback!Main")
        );
        assert_eq!(
            resolve_icon_source_with(&discovery, "native.exe").as_deref(),
            Some(r"C:\Windows\native.exe")
        );
    }

    #[cfg(feature = "native-validation")]
    #[test]
    fn notepad_alias_resolves_to_a_real_windows_executable() {
        let resolved = resolve_executable_path("app.notepad")
            .unwrap_or_else(|| panic!("Windows did not resolve Notepad"));
        assert!(Path::new(&resolved).is_file(), "missing {resolved}");
    }

    #[cfg(feature = "native-validation")]
    #[test]
    fn calculator_alias_uses_the_packaged_windows_shell_item() {
        let source = resolve_icon_source("app.calculator")
            .unwrap_or_else(|| panic!("Windows did not resolve the packaged Calculator icon"));
        let path = source
            .strip_prefix("image:")
            .unwrap_or_else(|| panic!("Calculator did not resolve to its package image: {source}"));
        assert!(Path::new(path).is_file(), "missing {path}");
    }

    #[test]
    fn calculator_process_aliases_share_the_pinned_app_identity() {
        let pinned_identity = canonical_app_id("calc.exe");
        for alias in [
            "CalculatorApp.exe",
            "calc.exe",
            "app.calculator",
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App",
        ] {
            assert_eq!(canonical_app_id(alias), pinned_identity);
        }
    }

    #[cfg(feature = "native-validation")]
    #[test]
    fn registered_aumid_resolves_to_an_appsfolder_shell_item() {
        let _com = ComApartment::initialize();
        let aumid = "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";

        let source = super::appsfolder_source(aumid)
            .unwrap_or_else(|| panic!("registered Calculator AUMID did not resolve"));

        assert_eq!(source, format!("shell:AppsFolder\\{aumid}"));
    }

    #[test]
    fn explicit_win32_aumid_keeps_the_executable_as_a_matching_alias() {
        let identity = resolved_app_identity(
            Some("Vivaldi.WMOW6CEHCLPDQ4UVKQKEJN7FEI"),
            Some(r"C:\Users\Carlos\AppData\Local\Vivaldi\Application\vivaldi.exe"),
            true,
        )
        .unwrap_or_else(|| panic!("Vivaldi identity did not resolve"));

        assert_eq!(
            identity.app().as_str(),
            "vivaldi.wmow6cehclpdq4uvkqkejn7fei"
        );
        assert_eq!(identity.aliases().len(), 1);
        assert_eq!(identity.aliases()[0].as_str(), "vivaldi.exe");
    }
    #[test]
    fn unregistered_aumid_preserves_the_executable_as_the_stable_identity() {
        let identity = resolved_app_identity(
            Some("Vendor.Unregistered.Profile"),
            Some(r"C:\Program Files\Vendor\browser.exe"),
            false,
        )
        .unwrap_or_else(|| panic!("unregistered Win32 identity did not resolve"));

        assert_eq!(identity.app().as_str(), "browser.exe");
        assert_eq!(
            identity.aliases(),
            &[shell_core::AppId::parse("vendor.unregistered.profile")
                .unwrap_or_else(|error| panic!("invalid test AUMID: {error}"))]
        );
    }

    #[cfg(feature = "native-validation")]
    #[test]
    fn launch_targets_never_return_untrusted_relative_executables() {
        for target in ["notepad.exe", "explorer.exe", "taskmgr.exe"] {
            let resolved = resolve_launch_target(target)
                .unwrap_or_else(|| panic!("trusted target did not resolve: {target}"));
            assert!(
                Path::new(&resolved).is_absolute(),
                "relative target: {resolved}"
            );
        }
        assert!(
            resolve_launch_target("app.calculator")
                .is_some_and(|target| target.starts_with("shell:AppsFolder\\"))
        );
        assert!(resolve_launch_target("missing-relative-app.exe").is_none());
        assert!(resolve_launch_target("notepad.exe\0malicious.exe").is_none());
    }
}
