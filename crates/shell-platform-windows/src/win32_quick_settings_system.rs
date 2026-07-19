use std::ffi::c_void;
use std::mem::size_of;
use std::time::{SystemTime, UNIX_EPOCH};

use shell_core::QuickControlKind;
use win_nightlight_lib::nightlight_state::NightlightState;
use win_nightlight_lib::{NightlightBackend, NightlightError, NightlightManager, RegistryBackend};
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
use windows_registry::{CURRENT_USER, Value};
use windows_result::{Error as RegistryError, HRESULT as RegistryHresult};

use crate::night_light_coordinator::{NightLightApplyError, NightLightApplyResult};

const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const NEARBY_SHARING_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CDP";
const QUIET_HOURS_CACHE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CloudStore\Store\Cache\DefaultAccount\$$windows.data.notifications.quiethourssettings\Current";
const QUIET_HOURS_PROFILE_PREFIX: &str = "Microsoft.QuietHoursProfile.";
const QUIET_HOURS_UNRESTRICTED: &str = "Microsoft.QuietHoursProfile.Unrestricted";
const QUIET_HOURS_PRIORITY_ONLY: &str = "Microsoft.QuietHoursProfile.PriorityOnly";
const QUIET_HOURS_SUFFIX: [u8; 4] = [0xCA, 0x28, 0x00, 0x00];
const CLOUDSTORE_CURRENT_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CloudStore\Store\DefaultAccount\Current";
const PER_DEVICE_STATE_MARKER: &str =
    "$windows.data.bluelightreduction.bluelightreductionstateperdevice";
const PER_DEVICE_STATE_CHILD: &str =
    "windows.data.bluelightreduction.bluelightreductionstateperdevice";
const PER_DEVICE_SETTINGS_CHILD: &str = "windows.data.bluelightreduction.settingsperdevice";

pub(super) fn read_active(kind: QuickControlKind) -> Result<bool> {
    match kind {
        QuickControlKind::Wifi => read_radio(RadioKind::WiFi),
        QuickControlKind::Bluetooth => read_radio(RadioKind::Bluetooth),
        QuickControlKind::NearbySharing => Ok(read_dword(
            NEARBY_SHARING_KEY,
            "NearShareChannelUserAuthzPolicy",
        )?
        .is_some_and(|value| value != 0)),
        QuickControlKind::Focus => quiet_hours_active(&read_quiet_hours_blob()?),
        QuickControlKind::Multitasking => multitasking_active(),
        QuickControlKind::DarkMode => {
            Ok(read_dword(PERSONALIZE_KEY, "AppsUseLightTheme")?.is_some_and(|value| value == 0))
        }
        QuickControlKind::NightLight => night_light_active(),
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
        QuickControlKind::Focus => set_quiet_hours_active(active),
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

fn set_quiet_hours_active(active: bool) -> Result<()> {
    let original = read_quiet_hours_blob()?;
    let observed = original
        .get(4..12)
        .and_then(|bytes| bytes.try_into().ok())
        .map_or(0, u64::from_le_bytes);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(observed, |duration| {
            const WINDOWS_TO_UNIX_SECONDS: u64 = 11_644_473_600;
            duration
                .as_secs()
                .saturating_add(WINDOWS_TO_UNIX_SECONDS)
                .saturating_mul(10_000_000)
                .saturating_add(u64::from(duration.subsec_nanos()) / 100)
        });
    let timestamp = now.max(observed.saturating_add(1));
    let changed = rewrite_quiet_hours(&original, active, timestamp)?;
    write_quiet_hours_blob(&changed)?;
    notify_settings("Notifications");

    if quiet_hours_active(&read_quiet_hours_blob()?) == Ok(active) {
        Ok(())
    } else {
        Err(Error::new(
            failure(),
            "Windows did not confirm the requested Focus state",
        ))
    }
}

fn read_quiet_hours_blob() -> Result<Vec<u8>> {
    let key = CURRENT_USER
        .options()
        .read()
        .open(QUIET_HOURS_CACHE_KEY)
        .map_err(quiet_hours_registry_error)?;
    key.get_value("Data")
        .map(|value| value.to_vec())
        .map_err(quiet_hours_registry_error)
}

fn write_quiet_hours_blob(data: &[u8]) -> Result<()> {
    let key = CURRENT_USER
        .options()
        .write()
        .open(QUIET_HOURS_CACHE_KEY)
        .map_err(quiet_hours_registry_error)?;
    key.set_value("Data", &Value::from(data))
        .map_err(quiet_hours_registry_error)
}

fn quiet_hours_active(data: &[u8]) -> Result<bool> {
    Ok(quiet_hours_profile(data)? != QUIET_HOURS_UNRESTRICTED)
}

fn quiet_hours_profile(data: &[u8]) -> Result<String> {
    let (start, end) = quiet_hours_profile_range(data)?;
    let units = data[start..end]
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| invalid_quiet_hours_blob("invalid profile name"))
}

fn rewrite_quiet_hours(data: &[u8], active: bool, timestamp: u64) -> Result<Vec<u8>> {
    if data.len() < 12 {
        return Err(invalid_quiet_hours_blob("missing CloudStore timestamp"));
    }
    let (start, end) = quiet_hours_profile_range(data)?;
    let profile = if active {
        QUIET_HOURS_PRIORITY_ONLY
    } else {
        QUIET_HOURS_UNRESTRICTED
    };
    let mut changed = Vec::with_capacity(data.len() + profile.len().saturating_mul(2));
    changed.extend_from_slice(&data[..start]);
    for unit in profile.encode_utf16() {
        changed.extend_from_slice(&unit.to_le_bytes());
    }
    changed.extend_from_slice(&data[end..]);
    changed[4..12].copy_from_slice(&timestamp.to_le_bytes());
    Ok(changed)
}

fn quiet_hours_profile_range(data: &[u8]) -> Result<(usize, usize)> {
    let prefix = QUIET_HOURS_PROFILE_PREFIX
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let start = data
        .windows(prefix.len())
        .position(|window| window == prefix)
        .ok_or_else(|| invalid_quiet_hours_blob("missing profile name"))?;
    let end = data[start..]
        .windows(QUIET_HOURS_SUFFIX.len())
        .position(|window| window == QUIET_HOURS_SUFFIX)
        .map(|offset| start + offset)
        .filter(|end| (end - start) % 2 == 0)
        .ok_or_else(|| invalid_quiet_hours_blob("missing profile terminator"))?;
    Ok((start, end))
}

fn invalid_quiet_hours_blob(message: &str) -> Error {
    Error::new(failure(), message)
}

fn quiet_hours_registry_error(error: RegistryError) -> Error {
    Error::new(failure(), error.to_string())
}

fn night_light_active() -> Result<bool> {
    let global = NightlightManager::new(RegistryBackend)
        .get_state()
        .map_err(night_light_error)?;
    let per_device = per_device_night_light_backends()?;
    if per_device.is_empty() {
        return Ok(global.is_enabled);
    }
    let expected = global.is_enabled;
    for backend in per_device {
        let state = NightlightManager::new(backend)
            .get_state()
            .map_err(night_light_error)?;
        if state.is_enabled != expected {
            return Err(Error::new(failure(), "Night light records do not agree"));
        }
    }
    Ok(expected)
}

pub(crate) fn apply_night_light(
    active: bool,
    minimum_timestamp: u64,
) -> std::result::Result<NightLightApplyResult, NightLightApplyError> {
    apply_night_light_inner(active, minimum_timestamp)
        .map_err(|error| NightLightApplyError::new(error.to_string()))
}

fn apply_night_light_inner(active: bool, minimum_timestamp: u64) -> Result<NightLightApplyResult> {
    let global_manager = NightlightManager::new(RegistryBackend);
    let global_state = global_manager.get_state().map_err(night_light_error)?;
    let mut per_device = Vec::new();
    for backend in per_device_night_light_backends()? {
        let manager = NightlightManager::new(backend);
        let state = manager.get_state().map_err(night_light_error)?;
        per_device.push((manager, state));
    }
    let observed = per_device
        .iter()
        .map(|(_, state)| state.timestamp)
        .fold(global_state.timestamp, u64::max);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(observed, |duration| duration.as_secs());
    let timestamp = next_night_light_timestamp(now, observed, minimum_timestamp);

    write_night_light_state(&global_manager, global_state, active, timestamp)?;
    for (manager, state) in &per_device {
        write_night_light_state(manager, state.clone(), active, timestamp)?;
    }

    verify_night_light_state(&global_manager, active, timestamp)?;
    for (manager, _) in &per_device {
        verify_night_light_state(manager, active, timestamp)?;
    }
    Ok(NightLightApplyResult { active, timestamp })
}

fn write_night_light_state<B: NightlightBackend>(
    manager: &NightlightManager<B>,
    mut state: NightlightState,
    active: bool,
    timestamp: u64,
) -> Result<()> {
    if active {
        state.enable()
    } else {
        state.disable()
    };
    state.timestamp = timestamp;
    manager.set_state(&state).map_err(night_light_error)
}

fn verify_night_light_state<B: NightlightBackend>(
    manager: &NightlightManager<B>,
    active: bool,
    minimum_timestamp: u64,
) -> Result<()> {
    let state = manager.get_state().map_err(night_light_error)?;
    if state.is_enabled == active && state.timestamp >= minimum_timestamp {
        Ok(())
    } else {
        Err(Error::new(
            failure(),
            "Windows did not confirm the requested Night light state",
        ))
    }
}

fn next_night_light_timestamp(now: u64, observed: u64, last_written: u64) -> u64 {
    now.max(observed.saturating_add(1))
        .max(last_written.saturating_add(1))
}

struct PerDeviceNightLightBackend {
    state_path: String,
    settings_path: String,
}

impl NightlightBackend for PerDeviceNightLightBackend {
    fn read_settings_bytes(&self) -> std::result::Result<Vec<u8>, NightlightError> {
        read_night_light_blob(&self.settings_path)
    }

    fn write_settings_bytes(&self, _data: &[u8]) -> std::result::Result<(), NightlightError> {
        Err(NightlightError::WriteRegistryValue(
            RegistryError::from_hresult(RegistryHresult::from_win32(50)),
        ))
    }

    fn read_state_bytes(&self) -> std::result::Result<Vec<u8>, NightlightError> {
        read_night_light_blob(&self.state_path)
    }

    fn write_state_bytes(&self, data: &[u8]) -> std::result::Result<(), NightlightError> {
        let converted = to_per_device_state_blob(data).map_err(|error| {
            let _ = error;
            NightlightError::WriteRegistryValue(RegistryError::from_hresult(
                RegistryHresult::from_win32(13),
            ))
        })?;
        write_night_light_blob(&self.state_path, &converted)
    }
}

fn per_device_night_light_backends() -> Result<Vec<PerDeviceNightLightBackend>> {
    let current = CURRENT_USER
        .options()
        .read()
        .open(CLOUDSTORE_CURRENT_KEY)
        .map_err(night_light_registry_error)?;
    let keys = current.keys().map_err(night_light_registry_error)?;
    let mut backends = keys
        .filter(|name| name.ends_with(PER_DEVICE_STATE_MARKER))
        .map(|state_key| {
            let settings_key =
                state_key.replace("bluelightreductionstateperdevice", "settingsperdevice");
            PerDeviceNightLightBackend {
                state_path: format!(
                    "{CLOUDSTORE_CURRENT_KEY}\\{state_key}\\{PER_DEVICE_STATE_CHILD}"
                ),
                settings_path: format!(
                    "{CLOUDSTORE_CURRENT_KEY}\\{settings_key}\\{PER_DEVICE_SETTINGS_CHILD}"
                ),
            }
        })
        .collect::<Vec<_>>();
    backends.sort_by(|left, right| left.state_path.cmp(&right.state_path));
    Ok(backends)
}

fn read_night_light_blob(path: &str) -> std::result::Result<Vec<u8>, NightlightError> {
    let key = CURRENT_USER
        .options()
        .read()
        .open(path)
        .map_err(NightlightError::OpenRegistryKey)?;
    key.get_value("Data")
        .map(|value| value.to_vec())
        .map_err(NightlightError::ReadRegistryValue)
}

fn write_night_light_blob(path: &str, data: &[u8]) -> std::result::Result<(), NightlightError> {
    let key = CURRENT_USER
        .options()
        .write()
        .open(path)
        .map_err(NightlightError::OpenRegistryKey)?;
    key.set_value("Data", &Value::from(data))
        .map_err(NightlightError::WriteRegistryValue)
}

fn to_per_device_state_blob(data: &[u8]) -> Result<Vec<u8>> {
    const BOND_HEADER: [u8; 4] = [0x43, 0x42, 0x01, 0x00];
    let inner_start = data
        .windows(BOND_HEADER.len())
        .enumerate()
        .skip(1)
        .find_map(|(index, bytes)| (bytes == BOND_HEADER).then_some(index))
        .ok_or_else(|| invalid_night_light_blob("missing inner Bond payload"))?;
    let length_index = inner_start
        .checked_sub(1)
        .ok_or_else(|| invalid_night_light_blob("missing payload length"))?;
    let inner_length = usize::from(data[length_index]);
    if data[length_index] >= 0x80 {
        return Err(invalid_night_light_blob("unsupported payload length"));
    }
    let inner_end = inner_start
        .checked_add(inner_length)
        .filter(|end| *end <= data.len())
        .ok_or_else(|| invalid_night_light_blob("truncated inner payload"))?;
    let inner = &data[inner_start..inner_end];
    if !inner.starts_with(&BOND_HEADER) {
        return Err(invalid_night_light_blob("invalid inner Bond header"));
    }

    let mut cursor = BOND_HEADER.len();
    if inner.get(cursor) == Some(&0x10) {
        cursor = varint_end(inner, cursor + 1)?;
    }
    let initialized_start = cursor;
    if inner.get(cursor..cursor + 2) != Some(&[0xD0, 0x0A]) {
        return Err(invalid_night_light_blob("missing global initialized field"));
    }
    cursor = varint_end(inner, cursor + 2)?;
    let initialized_end = cursor;
    if inner.get(cursor..cursor + 2) != Some(&[0xC6, 0x14]) {
        return Err(invalid_night_light_blob("missing transition field"));
    }
    cursor = varint_end(inner, cursor + 2)?;
    if inner.get(cursor..) != Some(&[0x00]) {
        return Err(invalid_night_light_blob("unexpected state payload tail"));
    }

    let converted_length = inner_length - (initialized_end - initialized_start) + 1;
    let converted_length = u8::try_from(converted_length)
        .map_err(|_| invalid_night_light_blob("per-device payload is too large"))?;
    let mut converted = Vec::with_capacity(data.len() - 2);
    converted.extend_from_slice(&data[..length_index]);
    converted.push(converted_length);
    converted.extend_from_slice(&inner[..initialized_start]);
    converted.extend_from_slice(&inner[initialized_end..cursor]);
    converted.extend_from_slice(&[0x01, 0x00]);
    converted.extend_from_slice(&data[inner_end..]);
    Ok(converted)
}

fn varint_end(data: &[u8], mut cursor: usize) -> Result<usize> {
    while let Some(byte) = data.get(cursor) {
        cursor += 1;
        if byte & 0x80 == 0 {
            return Ok(cursor);
        }
    }
    Err(invalid_night_light_blob("truncated varint"))
}

fn invalid_night_light_blob(message: &str) -> Error {
    Error::new(failure(), message)
}

fn night_light_error(error: NightlightError) -> Error {
    Error::new(failure(), error.to_string())
}

fn night_light_registry_error(error: RegistryError) -> Error {
    Error::new(failure(), error.to_string())
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
const fn failure() -> HRESULT {
    HRESULT(0x8000_4005_u32 as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use win_nightlight_lib::nightlight_state::NightlightState;

    fn quiet_hours_blob(profile: &str, timestamp: u64) -> Vec<u8> {
        let mut blob = vec![
            0x02, 0x00, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x00, 0x00, 0x00, 0x43, 0x42,
            0x01, 0x00, 0xC2, 0x0A, 0x01, 0xD2, 0x14,
        ];
        blob[4..12].copy_from_slice(&timestamp.to_le_bytes());
        for unit in profile.encode_utf16() {
            blob.extend_from_slice(&unit.to_le_bytes());
        }
        blob.extend_from_slice(&[0xCA, 0x28, 0x00, 0x00]);
        blob
    }

    #[test]
    fn quiet_hours_profile_reads_the_cloud_store_state() {
        let off = quiet_hours_blob("Microsoft.QuietHoursProfile.Unrestricted", 42);
        let on = quiet_hours_blob("Microsoft.QuietHoursProfile.PriorityOnly", 43);

        assert!(!quiet_hours_active(&off).unwrap());
        assert!(quiet_hours_active(&on).unwrap());
    }

    #[test]
    fn quiet_hours_profile_rewrite_advances_timestamp_and_preserves_framing() {
        let original = quiet_hours_blob("Microsoft.QuietHoursProfile.Unrestricted", 42);
        let changed = rewrite_quiet_hours(&original, true, 99).unwrap();

        assert!(quiet_hours_active(&changed).unwrap());
        assert_eq!(u64::from_le_bytes(changed[4..12].try_into().unwrap()), 99);
        assert!(changed.starts_with(&[0x02, 0x00, 0x00, 0x00]));
        assert!(changed.ends_with(&[0xCA, 0x28, 0x00, 0x00]));
    }

    #[test]
    fn per_device_night_light_blob_keeps_the_state_and_derived_schema() {
        let expected = NightlightState {
            timestamp: 1_784_426_935,
            is_enabled: true,
            initialized: 0,
            last_transition_filetime: 134_130_317_350_000_000,
        };
        let converted = to_per_device_state_blob(&expected.serialize_to_bytes()).unwrap();
        let decoded = NightlightState::deserialize_from_bytes(&converted).unwrap();

        assert_eq!(decoded.is_enabled, expected.is_enabled);
        assert_eq!(decoded.timestamp, expected.timestamp);
        assert_eq!(
            decoded.last_transition_filetime,
            expected.last_transition_filetime
        );
        assert!(converted.windows(2).any(|bytes| bytes == [0x01, 0x00]));
    }

    #[test]
    fn night_light_timestamp_advances_even_inside_the_same_second() {
        assert_eq!(next_night_light_timestamp(50, 50, 50), 51);
        assert_eq!(next_night_light_timestamp(20, 50, 49), 51);
        assert_eq!(next_night_light_timestamp(100, 50, 90), 100);
        assert_eq!(
            next_night_light_timestamp(u64::MAX, u64::MAX, u64::MAX),
            u64::MAX
        );
    }
}
