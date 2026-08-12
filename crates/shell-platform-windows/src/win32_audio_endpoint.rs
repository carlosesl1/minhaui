use std::ptr::null;

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eMultimedia, eRender};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::Result;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AudioEndpointState {
    pub(super) volume: u8,
    pub(super) muted: bool,
}

pub(super) fn read() -> Result<AudioEndpointState> {
    let endpoint = default_endpoint()?;
    // SAFETY: The COM interface is valid for this call and returns value-only state.
    let scalar = unsafe { endpoint.GetMasterVolumeLevelScalar()? };
    // SAFETY: The COM interface is valid for this call and returns value-only state.
    let muted = unsafe { endpoint.GetMute()? }.as_bool();
    Ok(AudioEndpointState {
        volume: (scalar.clamp(0.0, 1.0) * 100.0).round() as u8,
        muted,
    })
}

pub(super) fn set_volume(value: u8) -> Result<AudioEndpointState> {
    let endpoint = default_endpoint()?;
    let value = value.min(100);
    // SAFETY: The level is a documented scalar in the inclusive 0.0..=1.0 range.
    unsafe {
        endpoint.SetMasterVolumeLevelScalar(f32::from(value) / 100.0, null())?;
        if value > 0 {
            endpoint.SetMute(false, null())?;
        }
    }
    Ok(AudioEndpointState {
        volume: value,
        muted: false,
    })
}

pub(super) fn toggle_mute() -> Result<AudioEndpointState> {
    let endpoint = default_endpoint()?;
    // SAFETY: The COM interface is valid and both methods use value-only state.
    let muted = unsafe { endpoint.GetMute()? }.as_bool();
    // SAFETY: The event context is optional; null uses no application-specific context.
    unsafe { endpoint.SetMute(!muted, null())? };
    // SAFETY: The COM interface is valid for this value-only query.
    let scalar = unsafe { endpoint.GetMasterVolumeLevelScalar()? };
    Ok(AudioEndpointState {
        volume: (scalar.clamp(0.0, 1.0) * 100.0).round() as u8,
        muted: !muted,
    })
}

fn default_endpoint() -> Result<IAudioEndpointVolume> {
    // SAFETY: COM is initialized by the owning UI thread before quick settings are created.
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
    // SAFETY: The enumerator owns the returned default rendering endpoint interface.
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }?;
    // SAFETY: IAudioEndpointVolume is a documented activation interface for audio endpoints.
    unsafe { device.Activate(CLSCTX_ALL, None) }
}
