use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::ptr::null;

use windows::Win32::Foundation::{CloseHandle, HANDLE, PROPERTYKEY};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, AudioSessionStateExpired, DEVICE_STATE_ACTIVE, IAudioSessionControl2,
    IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
    eMultimedia, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToString;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{GUID, Interface, PWSTR, Result};

use crate::{
    AudioOutputId, AudioOutputSnapshot, AudioPanelSnapshot, AudioSessionId, AudioSessionSnapshot,
};

const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

pub(super) fn read() -> Result<AudioPanelSnapshot> {
    let enumerator = device_enumerator()?;
    // SAFETY: The enumerator owns the returned active rendering endpoint.
    let default = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }?;
    let default_id = device_id(&default).unwrap_or_default();
    let outputs = read_outputs(&enumerator, &default_id)?;
    let sessions = read_sessions(&default)?;
    Ok(AudioPanelSnapshot::new(sessions, outputs, false))
}

pub(super) fn set_session_volume(id: AudioSessionId, value: u8) -> Result<()> {
    let enumerator = device_enumerator()?;
    // SAFETY: The enumerator owns the returned active rendering endpoint.
    let default = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }?;
    // SAFETY: IAudioSessionManager2 is the documented activation interface for an endpoint.
    let manager: IAudioSessionManager2 = unsafe { default.Activate(CLSCTX_ALL, None) }?;
    // SAFETY: The manager returns a snapshot enumerator owned by this scope.
    let sessions = unsafe { manager.GetSessionEnumerator() }?;
    // SAFETY: The enumerator is valid for this synchronous query.
    let count = unsafe { sessions.GetCount() }?;
    for index in 0..count {
        // SAFETY: `index` is bounded by the count returned above.
        let control = unsafe { sessions.GetSession(index) }?;
        let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
            continue;
        };
        if session_id(&control2) != Some(id) {
            continue;
        }
        let Ok(volume) = control.cast::<ISimpleAudioVolume>() else {
            continue;
        };
        let scalar = f32::from(value.min(100)) / 100.0;
        // SAFETY: The scalar is in 0.0..=1.0 and the event context is optional.
        unsafe {
            volume.SetMasterVolume(scalar, null())?;
            if value > 0 {
                volume.SetMute(false, null())?;
            }
        }
        return Ok(());
    }
    Err(windows::core::Error::new(
        windows::core::HRESULT(0x8007_0002_u32 as i32),
        "The audio session is no longer available",
    ))
}

fn device_enumerator() -> Result<IMMDeviceEnumerator> {
    // SAFETY: COM is initialized by the owning shell thread.
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn read_outputs(
    enumerator: &IMMDeviceEnumerator,
    default_id: &str,
) -> Result<Vec<AudioOutputSnapshot>> {
    // SAFETY: The state mask requests active render endpoints only.
    let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }?;
    // SAFETY: The collection remains live for the duration of enumeration.
    let count = unsafe { collection.GetCount() }?;
    let mut result = Vec::new();
    for index in 0..count {
        // SAFETY: `index` is bounded by the count returned above.
        let device = unsafe { collection.Item(index) }?;
        let raw_id = device_id(&device).unwrap_or_else(|| format!("output-{index}"));
        let label = friendly_name(&device).unwrap_or_else(|| "Audio output".to_owned());
        let selected = raw_id == default_id;
        result.push(AudioOutputSnapshot::new(
            AudioOutputId::new(stable_id(&raw_id)),
            &label,
            if selected {
                "Current output"
            } else {
                "Manage in Windows Settings"
            },
            selected,
        ));
    }
    result.sort_by_key(|output| !output.selected());
    Ok(result)
}

fn read_sessions(device: &IMMDevice) -> Result<Vec<AudioSessionSnapshot>> {
    // SAFETY: IAudioSessionManager2 is the documented activation interface for an endpoint.
    let manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None) }?;
    // SAFETY: The manager returns a snapshot enumerator owned by this scope.
    let enumerator = unsafe { manager.GetSessionEnumerator() }?;
    // SAFETY: The enumerator is valid for this synchronous query.
    let count = unsafe { enumerator.GetCount() }?;
    let mut result = Vec::new();
    for index in 0..count {
        // SAFETY: `index` is bounded by the count returned above.
        let control = unsafe { enumerator.GetSession(index) }?;
        // SAFETY: The session remains valid while the control reference is held.
        if unsafe { control.GetState() }? == AudioSessionStateExpired {
            continue;
        }
        let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
            continue;
        };
        let Ok(volume) = control.cast::<ISimpleAudioVolume>() else {
            continue;
        };
        let Some(id) = session_id(&control2) else {
            continue;
        };
        // SAFETY: The session interfaces remain valid for this synchronous value query.
        let scalar = unsafe { volume.GetMasterVolume() }?;
        // SAFETY: The session interfaces remain valid for this synchronous value query.
        let muted = unsafe { volume.GetMute() }?.as_bool();
        // SAFETY: The session control remains valid and the call only identifies its category.
        let system = unsafe { control2.IsSystemSoundsSession() } == windows::core::HRESULT(0);
        // SAFETY: The session control remains valid and the call returns a value-only process ID.
        let pid = unsafe { control2.GetProcessId() }.unwrap_or(0);
        let identity = (!system).then(|| process_identity(pid)).flatten();
        let label = if system {
            "System sounds".to_owned()
        } else {
            identity
                .as_ref()
                .map(|(label, _)| label.clone())
                .unwrap_or_else(|| format!("Application {pid}"))
        };
        // SAFETY: The session control remains valid for this synchronous state query.
        let active = unsafe { control.GetState() }? == AudioSessionStateActive;
        let mut snapshot = AudioSessionSnapshot::new(
            id,
            &label,
            if active {
                "Playing audio"
            } else {
                "Recent audio"
            },
            (scalar.clamp(0.0, 1.0) * 100.0).round() as u8,
            muted,
        );
        if let Some((_, path)) = identity {
            snapshot = snapshot.with_icon_source(&path);
        }
        result.push(snapshot);
    }
    result.sort_by_key(|session| session.detail() != "Playing audio");
    result.truncate(3);
    Ok(result)
}

fn session_id(control: &IAudioSessionControl2) -> Option<AudioSessionId> {
    // SAFETY: The returned string is COM-allocated and freed immediately after conversion.
    let raw = unsafe { control.GetSessionInstanceIdentifier() }.ok()?;
    let value = take_com_string(raw)?;
    Some(AudioSessionId::new(stable_id(&value)))
}

fn device_id(device: &IMMDevice) -> Option<String> {
    // SAFETY: The returned string is COM-allocated and freed immediately after conversion.
    let raw = unsafe { device.GetId() }.ok()?;
    take_com_string(raw)
}

fn friendly_name(device: &IMMDevice) -> Option<String> {
    // SAFETY: The device owns the returned property store interface.
    let store = unsafe { device.OpenPropertyStore(STGM_READ) }.ok()?;
    // SAFETY: The static property key is valid and the variant owns its value in this scope.
    let value = unsafe { store.GetValue(&PKEY_DEVICE_FRIENDLY_NAME) }.ok()?;
    let mut buffer = [0_u16; 512];
    // SAFETY: The PROPVARIANT and writable UTF-16 buffer are valid for the conversion.
    unsafe { PropVariantToString(&value, &mut buffer) }.ok()?;
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    (length > 0).then(|| String::from_utf16_lossy(&buffer[..length]))
}

fn take_com_string(value: PWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // SAFETY: The API guarantees a NUL-terminated COM allocation.
    let string = unsafe { value.to_string() }.ok();
    // SAFETY: The matching Core Audio APIs allocate these strings with CoTaskMemAlloc.
    unsafe { CoTaskMemFree(Some(value.0.cast())) };
    string.filter(|value| !value.is_empty())
}

fn process_identity(pid: u32) -> Option<(String, String)> {
    if pid == 0 {
        return None;
    }
    // SAFETY: This opens a read-only query handle to an existing process id.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let identity = query_process_identity(handle);
    // SAFETY: `handle` is owned by this function.
    let _ = unsafe { CloseHandle(handle) };
    identity
}

fn query_process_identity(handle: HANDLE) -> Option<(String, String)> {
    let mut buffer = [0_u16; 1024];
    let mut length = buffer.len() as u32;
    // SAFETY: The process handle is valid and the size describes the writable buffer.
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &raw mut length,
        )
    }
    .ok()?;
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    let label = Path::new(&path)
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())?;
    Some((label, path))
}

fn stable_id(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
