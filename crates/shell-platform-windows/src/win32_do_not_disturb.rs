use std::ffi::c_void;
use std::mem::size_of;
use std::time::{Duration, Instant};

use windows::core::{Error, HRESULT, Result};

use crate::DoNotDisturbMode;

const CONFIRMATION_TIMEOUT: Duration = Duration::from_millis(300);
const CONFIRMATION_POLL: Duration = Duration::from_millis(15);

#[repr(C)]
struct WnfStateName {
    data: [u32; 2],
}

static DO_NOT_DISTURB_STATE: WnfStateName = WnfStateName {
    data: [0xA3BF_1C75, 0x0D83_063E],
};

unsafe extern "C" {
    fn minhaui_quiet_hours_set_mode(mode: i32) -> i32;
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryWnfStateData(
        state_name: *const WnfStateName,
        type_id: *const c_void,
        explicit_scope: *const c_void,
        change_stamp: *mut u32,
        buffer: *mut c_void,
        buffer_size: *mut u32,
    ) -> i32;
}

pub(super) fn read_mode() -> Result<DoNotDisturbMode> {
    let mut change_stamp = 0_u32;
    let mut value = 0_u32;
    let mut buffer_size = size_of::<u32>() as u32;

    // SAFETY: all pointers reference initialized local storage for the duration of
    // this read-only call. No type or explicit scope is required for this state.
    let status = unsafe {
        NtQueryWnfStateData(
            &raw const DO_NOT_DISTURB_STATE,
            std::ptr::null(),
            std::ptr::null(),
            &raw mut change_stamp,
            (&raw mut value).cast(),
            &raw mut buffer_size,
        )
    };

    if status < 0 {
        return Err(query_error(status));
    }
    if buffer_size != size_of::<u32>() as u32 {
        return Err(Error::new(
            failure(),
            format!("Windows returned an invalid Do Not Disturb payload size {buffer_size}"),
        ));
    }

    decode_wnf_state(value)
}

pub(super) fn set_mode(mode: DoNotDisturbMode) -> Result<DoNotDisturbMode> {
    // Quick Settings dispatches actions on the application's long-lived STA UI
    // thread. Keeping this call in that apartment avoids transient WinRT teardown.
    let raw = raw_mode(mode);
    // SAFETY: the bridge accepts only the three version-probed enum values.
    let status = unsafe { minhaui_quiet_hours_set_mode(raw) };
    bridge_status(status, "Windows rejected the Do Not Disturb mode")?;

    let deadline = Instant::now() + CONFIRMATION_TIMEOUT;
    let mut last_observed = read_mode();
    loop {
        if last_observed
            .as_ref()
            .is_ok_and(|observed| *observed == mode)
        {
            return Ok(mode);
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(CONFIRMATION_POLL);
        last_observed = read_mode();
    }

    Err(Error::new(
        failure(),
        format!(
            "Windows did not confirm the requested Do Not Disturb mode; last observation: {last_observed:?}"
        ),
    ))
}

fn bridge_status(status: i32, context: &str) -> Result<()> {
    let status = HRESULT(status);
    status
        .ok()
        .map_err(|error| Error::new(error.code(), format!("{context}: {error}")))
}

const fn raw_mode(mode: DoNotDisturbMode) -> i32 {
    match mode {
        DoNotDisturbMode::Off => 1,
        DoNotDisturbMode::PriorityOnly => 2,
        DoNotDisturbMode::AlarmsOnly => 3,
    }
}
fn decode_wnf_state(value: u32) -> Result<DoNotDisturbMode> {
    match value {
        0 => Ok(DoNotDisturbMode::Off),
        1 => Ok(DoNotDisturbMode::PriorityOnly),
        2 => Ok(DoNotDisturbMode::AlarmsOnly),
        unknown => Err(invalid_state(unknown)),
    }
}

fn invalid_state(value: impl std::fmt::Display) -> Error {
    Error::new(
        failure(),
        format!("Windows returned unknown Do Not Disturb state {value}"),
    )
}

fn query_error(status: i32) -> Error {
    Error::new(
        failure(),
        format!(
            "Windows could not report Do Not Disturb state (NTSTATUS 0x{:08X})",
            status as u32
        ),
    )
}

const fn failure() -> HRESULT {
    HRESULT(0x8000_4005_u32 as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wnf_state_maps_off_priority_and_alarms_without_guessing() {
        assert_eq!(decode_wnf_state(0).unwrap(), DoNotDisturbMode::Off);
        assert_eq!(decode_wnf_state(1).unwrap(), DoNotDisturbMode::PriorityOnly);
        assert_eq!(decode_wnf_state(2).unwrap(), DoNotDisturbMode::AlarmsOnly);
        assert!(decode_wnf_state(3).is_err());
    }

    #[test]
    fn internal_winrt_values_are_explicit_and_do_not_reuse_wnf_values() {
        assert_eq!(raw_mode(DoNotDisturbMode::Off), 1);
        assert_eq!(raw_mode(DoNotDisturbMode::PriorityOnly), 2);
        assert_eq!(raw_mode(DoNotDisturbMode::AlarmsOnly), 3);
    }
}
