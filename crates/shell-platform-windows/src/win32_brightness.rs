use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR, SetMonitorBrightness,
};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};
use windows::core::{Error, HRESULT, Result as WinResult};

use crate::brightness_coordinator::{BrightnessApplyError, BrightnessApplyResult};

unsafe extern "C" {
    fn minhaui_wmi_brightness_read(value: *mut u8) -> i32;
    fn minhaui_wmi_brightness_set(value: u8) -> i32;
}

pub(super) fn read() -> WinResult<u8> {
    read_wmi().or_else(|_| read_ddc())
}

pub(super) fn apply(
    value: u8,
) -> core::result::Result<BrightnessApplyResult, BrightnessApplyError> {
    let value = value.min(100);
    set_wmi(value)
        .or_else(|_| set_ddc(value))
        .map(|()| BrightnessApplyResult { value })
        .map_err(|error| BrightnessApplyError::new(error.to_string()))
}

fn read_wmi() -> WinResult<u8> {
    let mut value = 0_u8;
    // SAFETY: the bridge writes one byte to the valid out parameter and owns all COM objects.
    bridge_status(unsafe { minhaui_wmi_brightness_read(&raw mut value) })?;
    Ok(value.min(100))
}

fn set_wmi(value: u8) -> WinResult<()> {
    // SAFETY: the bridge receives a bounded value and owns all COM objects.
    bridge_status(unsafe { minhaui_wmi_brightness_set(value.min(100)) })
}

fn read_ddc() -> WinResult<u8> {
    let monitors = primary_physical_monitors()?;
    for monitor in monitors.as_slice() {
        let handle = monitor.hPhysicalMonitor;
        let mut minimum = 0_u32;
        let mut current = 0_u32;
        let mut maximum = 0_u32;
        // SAFETY: the physical-monitor handle is valid for the lifetime of `monitors`.
        if unsafe {
            GetMonitorBrightness(handle, &raw mut minimum, &raw mut current, &raw mut maximum)
        } != 0
        {
            return Ok(percent_from_hardware_range(minimum, current, maximum));
        }
    }
    Err(not_supported("No brightness-capable primary monitor"))
}

fn set_ddc(percent: u8) -> WinResult<()> {
    let monitors = primary_physical_monitors()?;
    for monitor in monitors.as_slice() {
        let handle = monitor.hPhysicalMonitor;
        let mut minimum = 0_u32;
        let mut current = 0_u32;
        let mut maximum = 0_u32;
        // SAFETY: the physical-monitor handle is valid for the lifetime of `monitors`.
        if unsafe {
            GetMonitorBrightness(handle, &raw mut minimum, &raw mut current, &raw mut maximum)
        } == 0
        {
            continue;
        }
        let value = hardware_value_from_percent(minimum, maximum, percent);
        // SAFETY: `value` is clamped to the range reported by this physical monitor.
        if unsafe { SetMonitorBrightness(handle, value) } != 0 {
            return Ok(());
        }
    }
    Err(Error::from_thread())
}

fn primary_physical_monitors() -> WinResult<PhysicalMonitors> {
    // SAFETY: no pointer parameters are involved; the constant requests the primary monitor.
    let monitor = unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
    let mut count = 0_u32;
    // SAFETY: `count` is a valid writable counter.
    unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &raw mut count) }?;
    if count == 0 {
        return Err(not_supported("No physical monitor handles"));
    }
    let mut monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
    // SAFETY: the binding receives the correctly sized initialized slice.
    unsafe { GetPhysicalMonitorsFromHMONITOR(monitor, &mut monitors) }?;
    Ok(PhysicalMonitors(monitors))
}

struct PhysicalMonitors(Vec<PHYSICAL_MONITOR>);

impl PhysicalMonitors {
    fn as_slice(&self) -> &[PHYSICAL_MONITOR] {
        &self.0
    }
}
impl Drop for PhysicalMonitors {
    fn drop(&mut self) {
        // SAFETY: each handle came from one successful physical-monitor enumeration.
        let _ = unsafe { DestroyPhysicalMonitors(&self.0) };
    }
}

fn bridge_status(status: i32) -> WinResult<()> {
    HRESULT(status).ok()
}

fn not_supported(message: &str) -> Error {
    Error::new(HRESULT(0x8007_0032_u32 as i32), message)
}

fn percent_from_hardware_range(minimum: u32, current: u32, maximum: u32) -> u8 {
    let span = maximum.saturating_sub(minimum);
    if span == 0 {
        return 0;
    }
    let relative = current.clamp(minimum, maximum).saturating_sub(minimum);
    u8::try_from((relative.saturating_mul(100) + span / 2) / span).unwrap_or(100)
}

fn hardware_value_from_percent(minimum: u32, maximum: u32, percent: u8) -> u32 {
    let span = maximum.saturating_sub(minimum);
    minimum.saturating_add((span.saturating_mul(u32::from(percent.min(100))) + 50) / 100)
}

#[cfg(test)]
mod tests {
    use super::{hardware_value_from_percent, percent_from_hardware_range};

    #[test]
    fn hardware_brightness_ranges_map_to_panel_percentages() {
        assert_eq!(percent_from_hardware_range(20, 60, 100), 50);
        assert_eq!(percent_from_hardware_range(25, 25, 25), 0);
        assert_eq!(hardware_value_from_percent(20, 100, 50), 60);
        assert_eq!(hardware_value_from_percent(20, 100, 100), 100);
    }
}
