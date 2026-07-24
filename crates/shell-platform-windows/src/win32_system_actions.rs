use std::mem::size_of;

use windows::Win32::Devices::Display::{
    SDC_APPLY, SDC_TOPOLOGY_CLONE, SDC_TOPOLOGY_EXTEND, SDC_TOPOLOGY_EXTERNAL,
    SDC_TOPOLOGY_INTERNAL, SET_DISPLAY_CONFIG_FLAGS, SetDisplayConfig,
};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY, VK_LWIN, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HRESULT, PCWSTR, Result, w};

use crate::{PopoverAction, ProjectionMode, SessionAction, SystemRoute};

pub(super) fn apply(action: &PopoverAction) -> Result<()> {
    match action {
        PopoverAction::OpenTaskManager => launch("taskmgr.exe", None),
        PopoverAction::OpenSystemRoute(route) => launch(route.uri(), None),
        PopoverAction::OpenQuickSettings => send_chord(VK_LWIN, VIRTUAL_KEY(u16::from(b'A'))),
        PopoverAction::SetProjectionMode(mode) => set_projection_mode(*mode),
        PopoverAction::VolumeDown => adjust_volume(-5),
        PopoverAction::ToggleMute => toggle_mute(),
        PopoverAction::VolumeUp => adjust_volume(5),
        PopoverAction::MediaPrevious => send_key(VK_MEDIA_PREV_TRACK),
        PopoverAction::MediaPlayPause => send_key(VK_MEDIA_PLAY_PAUSE),
        PopoverAction::MediaNext => send_key(VK_MEDIA_NEXT_TRACK),
        PopoverAction::ConfirmSession(action) => apply_session(*action),
        PopoverAction::OpenSettings
        | PopoverAction::CalendarPrevious
        | PopoverAction::CalendarToday
        | PopoverAction::CalendarNext
        | PopoverAction::OpenBackgroundApp(_)
        | PopoverAction::OpenBackgroundAppContextMenu(_) => Ok(()),
    }
}

pub(super) fn set_projection_mode(mode: ProjectionMode) -> Result<()> {
    // SAFETY: Category 8 (FFI boundary). Topology flags request a documented
    // persisted Windows display configuration; both optional arrays are absent
    // as required for SDC_TOPOLOGY_* calls.
    let result = unsafe { SetDisplayConfig(None, None, projection_flags(mode)) };
    if result == 0 {
        Ok(())
    } else {
        Err(windows::core::Error::new(
            HRESULT::from_win32(result as u32),
            format!("Windows refused projection mode {mode:?} with code {result}"),
        ))
    }
}

fn projection_flags(mode: ProjectionMode) -> SET_DISPLAY_CONFIG_FLAGS {
    SDC_APPLY
        | match mode {
            ProjectionMode::Internal => SDC_TOPOLOGY_INTERNAL,
            ProjectionMode::Duplicate => SDC_TOPOLOGY_CLONE,
            ProjectionMode::Extend => SDC_TOPOLOGY_EXTEND,
            ProjectionMode::External => SDC_TOPOLOGY_EXTERNAL,
        }
}

pub(super) fn open_search() -> Result<()> {
    launch("search:", None)
}

impl SystemRoute {
    const fn uri(self) -> &'static str {
        match self {
            Self::Network => "ms-settings:network-status",
            Self::Wifi => "ms-settings:network-wifi",
            Self::Bluetooth => "ms-settings:bluetooth",
            Self::Sound => "ms-settings:sound",
            Self::Display => "ms-settings:display",
            Self::Focus => "ms-settings:quiethours",
            Self::Power => "ms-settings:powersleep",
            Self::DateTime => "ms-settings:dateandtime",
        }
    }
}

fn apply_session(action: SessionAction) -> Result<()> {
    match action {
        SessionAction::Lock => {
            // SAFETY: Category 8 (FFI boundary). This parameterless documented
            // system command requests workstation locking and retains no pointer.
            unsafe { LockWorkStation() }
        }
        SessionAction::Sleep => {
            // SAFETY: Category 8 (FFI boundary). The synchronous call receives
            // value-only flags and requests normal sleep without forced closure.
            if unsafe { SetSuspendState(false, false, false) } {
                Ok(())
            } else {
                Err(windows::core::Error::new(
                    invalid_arg(),
                    "Windows refused the sleep request",
                ))
            }
        }
        SessionAction::SignOut => launch("shutdown.exe", Some("/l")),
        SessionAction::Restart => launch("shutdown.exe", Some("/r /t 0")),
        SessionAction::ShutDown => launch("shutdown.exe", Some("/s /t 0")),
    }
}

fn adjust_volume(delta: i16) -> Result<()> {
    let current = crate::win32_audio_endpoint::read()?.volume;
    let next = (i16::from(current) + delta).clamp(0, 100) as u8;
    crate::win32_audio_endpoint::set_volume(next).map(|_| ())
}

fn toggle_mute() -> Result<()> {
    crate::win32_audio_endpoint::toggle_mute().map(|_| ())
}

fn send_key(key: VIRTUAL_KEY) -> Result<()> {
    send_inputs(&[
        keyboard_input(key, KEYBD_EVENT_FLAGS(0)),
        keyboard_input(key, KEYEVENTF_KEYUP),
    ])
}

fn send_chord(modifier: VIRTUAL_KEY, key: VIRTUAL_KEY) -> Result<()> {
    send_inputs(&[
        keyboard_input(modifier, KEYBD_EVENT_FLAGS(0)),
        keyboard_input(key, KEYBD_EVENT_FLAGS(0)),
        keyboard_input(key, KEYEVENTF_KEYUP),
        keyboard_input(modifier, KEYEVENTF_KEYUP),
    ])
}

fn keyboard_input(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn send_inputs(inputs: &[INPUT]) -> Result<()> {
    // SAFETY: Category 8 (FFI boundary). The slice is contiguous and remains
    // alive for the synchronous input injection call.
    let sent = unsafe {
        SendInput(
            inputs,
            i32::try_from(size_of::<INPUT>()).unwrap_or_default(),
        )
    };
    if usize::try_from(sent).ok() == Some(inputs.len()) {
        Ok(())
    } else {
        Err(windows::core::Error::new(
            invalid_arg(),
            "Windows did not accept every synthetic input event",
        ))
    }
}

fn launch(target: &str, parameters: Option<&str>) -> Result<()> {
    let target = wide(target);
    let parameters = parameters.map(wide);
    // SAFETY: Category 8 (FFI boundary). All UTF-16 buffers are null-terminated,
    // fixed by typed action mapping, and live for the synchronous shell call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            parameters
                .as_ref()
                .map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr())),
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(windows::core::Error::new(
            invalid_arg(),
            format!(
                "system action launch failed with code {}",
                result.0 as isize
            ),
        ))
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}

#[cfg(test)]
mod tests {
    use crate::{ProjectionMode, SystemRoute};

    use super::projection_flags;

    #[test]
    fn system_routes_are_closed_and_documented() {
        assert_eq!(SystemRoute::Network.uri(), "ms-settings:network-status");
        assert_eq!(SystemRoute::Wifi.uri(), "ms-settings:network-wifi");
        assert_eq!(SystemRoute::Bluetooth.uri(), "ms-settings:bluetooth");
        assert_eq!(SystemRoute::Focus.uri(), "ms-settings:quiethours");
        assert_eq!(SystemRoute::DateTime.uri(), "ms-settings:dateandtime");
    }

    #[test]
    fn projection_modes_map_to_the_four_documented_windows_topologies() {
        assert_eq!(projection_flags(ProjectionMode::Internal).0, 0x81);
        assert_eq!(projection_flags(ProjectionMode::Duplicate).0, 0x82);
        assert_eq!(projection_flags(ProjectionMode::Extend).0, 0x84);
        assert_eq!(projection_flags(ProjectionMode::External).0, 0x88);
    }
}
