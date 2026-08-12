use std::ffi::c_void;
use std::mem::size_of;

use shell_core::QuickControlKind;
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, LPARAM, WIN32_ERROR, WPARAM,
};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SPI_GETWINARRANGING, SPI_SETWINARRANGING, SPIF_SENDCHANGE,
    SPIF_UPDATEINIFILE, SendMessageTimeoutW, SystemParametersInfoW, WM_SETTINGCHANGE,
};
use windows::core::{Error, HRESULT, PCWSTR, Result};

use crate::night_light_coordinator::{NightLightApplyError, NightLightApplyResult};

const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const NEARBY_SHARING_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CDP";

pub(super) fn read_active(kind: QuickControlKind) -> Result<bool> {
    match kind {
        QuickControlKind::Wifi => read_radio(RadioKind::WiFi),
        QuickControlKind::Bluetooth => read_radio(RadioKind::Bluetooth),
        QuickControlKind::NearbySharing => Ok(read_dword(
            NEARBY_SHARING_KEY,
            "NearShareChannelUserAuthzPolicy",
        )?
        .is_some_and(|value| value != 0)),
        QuickControlKind::Focus => Err(unsupported(kind)),
        QuickControlKind::Multitasking => multitasking_active(),
        QuickControlKind::DarkMode => {
            Ok(read_dword(PERSONALIZE_KEY, "AppsUseLightTheme")?.is_some_and(|value| value == 0))
        }
        QuickControlKind::NightLight => Err(unsupported(kind)),
        _ => Err(unsupported(kind)),
    }
}

pub(super) fn set_active(kind: QuickControlKind, active: bool) -> Result<()> {
    match kind {
        QuickControlKind::Wifi => set_radio(RadioKind::WiFi, active),
        QuickControlKind::Bluetooth => set_radio(RadioKind::Bluetooth, active),
        QuickControlKind::NearbySharing => {
            write_dword(
                NEARBY_SHARING_KEY,
                "NearShareChannelUserAuthzPolicy",
                u32::from(active),
            )?;
            notify_settings(NEARBY_SHARING_KEY);
            Ok(())
        }
        QuickControlKind::Focus => Err(unsupported(kind)),
        QuickControlKind::Multitasking => set_multitasking(active),
        QuickControlKind::DarkMode => {
            let light = u32::from(!active);
            write_dword(PERSONALIZE_KEY, "AppsUseLightTheme", light)?;
            write_dword(PERSONALIZE_KEY, "SystemUsesLightTheme", light)?;
            notify_settings("ImmersiveColorSet");
            Ok(())
        }
        QuickControlKind::NightLight => Err(unsupported(kind)),
        _ => Err(unsupported(kind)),
    }
}

fn read_radio(kind: RadioKind) -> Result<bool> {
    let radios = Radio::GetRadiosAsync()?.join()?;
    for radio in &radios {
        if radio.Kind()? == kind {
            return Ok(radio.State()? == RadioState::On);
        }
    }
    Err(Error::new(not_found(), "requested radio is not present"))
}

fn set_radio(kind: RadioKind, active: bool) -> Result<()> {
    if Radio::RequestAccessAsync()?.join()? != RadioAccessStatus::Allowed {
        return Err(Error::new(
            access_denied(),
            "Windows denied radio control access",
        ));
    }
    let radios = Radio::GetRadiosAsync()?.join()?;
    let target = if active {
        RadioState::On
    } else {
        RadioState::Off
    };
    let mut matched = false;
    for radio in &radios {
        if radio.Kind()? == kind {
            matched = true;
            if radio.SetStateAsync(target)?.join()? != RadioAccessStatus::Allowed {
                return Err(Error::new(
                    access_denied(),
                    "Windows refused the requested radio state",
                ));
            }
        }
    }
    if matched {
        Ok(())
    } else {
        Err(Error::new(not_found(), "requested radio is not present"))
    }
}

fn multitasking_active() -> Result<bool> {
    let mut enabled = 0_i32;
    // SAFETY: `enabled` is writable storage for this synchronous value-only query.
    unsafe {
        SystemParametersInfoW(
            SPI_GETWINARRANGING,
            0,
            Some((&raw mut enabled).cast::<c_void>()),
            Default::default(),
        )?;
    }
    Ok(enabled != 0)
}

fn set_multitasking(active: bool) -> Result<()> {
    // SPI_SETWINARRANGING documents pvParam as a BOOL encoded in the pointer value.
    let value = usize::from(active) as *mut c_void;
    // SAFETY: The action and value representation follow the documented SPI contract.
    unsafe {
        SystemParametersInfoW(
            SPI_SETWINARRANGING,
            0,
            Some(value),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    }
}

pub(crate) fn apply_night_light(
    _active: bool,
    _minimum_timestamp: u64,
) -> std::result::Result<NightLightApplyResult, NightLightApplyError> {
    Err(NightLightApplyError::new(
        "Direct Night light control is unavailable; open Windows Settings instead",
    ))
}

fn read_dword(path: &str, name: &str) -> Result<Option<u32>> {
    let path = wide(path);
    let name = wide(name);
    let mut value = 0_u32;
    let mut size = u32::try_from(size_of::<u32>()).unwrap_or(4);
    // SAFETY: All UTF-16 strings are terminated and output storage is correctly sized.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast::<c_void>()),
            Some(&raw mut size),
        )
    };
    match result {
        ERROR_SUCCESS => Ok(Some(value)),
        ERROR_FILE_NOT_FOUND => Ok(None),
        error => Err(win32_error(error, "registry read failed")),
    }
}

fn write_dword(path: &str, name: &str, value: u32) -> Result<()> {
    let path = wide(path);
    let name = wide(name);
    // SAFETY: The key paths are fixed per-user settings and the DWORD pointer is valid
    // for the duration of this synchronous call.
    let result = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            REG_DWORD.0,
            Some((&raw const value).cast::<c_void>()),
            u32::try_from(size_of::<u32>()).unwrap_or(4),
        )
    };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(win32_error(result, "registry write failed"))
    }
}

fn notify_settings(section: &str) {
    let section = wide(section);
    // SAFETY: HWND_BROADCAST is documented for WM_SETTINGCHANGE; the UTF-16 buffer
    // remains alive until the bounded synchronous broadcast returns.
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(section.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            None,
        );
    }
}

fn win32_error(code: WIN32_ERROR, context: &str) -> Error {
    Error::new(HRESULT::from_win32(code.0), context)
}

fn unsupported(kind: QuickControlKind) -> Error {
    Error::new(
        invalid_arg(),
        format!("{kind:?} does not expose a direct quick-settings action"),
    )
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

const fn invalid_arg() -> HRESULT {
    HRESULT(0x8007_0057_u32 as i32)
}
const fn access_denied() -> HRESULT {
    HRESULT(0x8007_0005_u32 as i32)
}
const fn not_found() -> HRESULT {
    HRESULT(0x8007_0002_u32 as i32)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_adapter_rejects_restricted_do_not_disturb_state() {
        assert!(read_active(QuickControlKind::Focus).is_err());
    }

    #[test]
    fn private_night_light_mutation_is_not_available_in_stable_builds() {
        assert!(apply_night_light(true, 0).is_err());
    }
}
