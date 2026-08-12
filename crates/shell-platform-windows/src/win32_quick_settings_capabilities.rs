use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;

use shell_core::QuickControlKind;
use windows::Win32::Devices::Bluetooth::{
    BLUETOOTH_FIND_RADIO_PARAMS, BluetoothFindFirstRadio, BluetoothFindRadioClose,
};
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO,
    GetDisplayConfigBufferSizes, QDC_ALL_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HANDLE};
use windows::Win32::Graphics::Gdi::DISPLAYCONFIG_PATH_ACTIVE;
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, IF_TYPE_IEEE80211, MIB_IF_TABLE2,
};
use windows::Win32::System::Power::{
    GetPwrCapabilities, GetSystemPowerStatus, SYSTEM_POWER_CAPABILITIES, SYSTEM_POWER_STATUS,
};

use crate::{
    ProjectionMode, QuickControlAvailability, QuickControlCapability, QuickSettingsCapabilities,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NetworkInterfaceFacts {
    interface_type: u32,
    hardware: bool,
    admin_up: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SystemBatteryFacts {
    present: bool,
    short_term: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DisplayPathFacts {
    source: (i32, u32, u32),
    target: (i32, u32, u32),
    available: bool,
    active: bool,
    embedded: bool,
}

pub(super) fn read_capabilities() -> QuickSettingsCapabilities {
    let mut controls = direct_controls();
    if wifi_availability().is_some() {
        let availability = native_availability(QuickControlKind::Wifi);
        controls.push(QuickControlCapability::new(
            QuickControlKind::Wifi,
            availability,
            "Wi-Fi",
            state_detail(availability),
            None,
        ));
    }
    if bluetooth_radio_present() {
        let availability = native_availability(QuickControlKind::Bluetooth);
        controls.push(QuickControlCapability::new(
            QuickControlKind::Bluetooth,
            availability,
            "Bluetooth",
            state_detail(availability),
            None,
        ));
    }
    if let Some((percent, charging)) = battery_status() {
        controls.push(QuickControlCapability::new(
            QuickControlKind::Battery,
            QuickControlAvailability::Available { active: charging },
            "Battery",
            if charging { "Charging" } else { "On battery" },
            Some(percent),
        ));
        controls.push(QuickControlCapability::new(
            QuickControlKind::EnergySaver,
            QuickControlAvailability::TemporarilyUnavailable,
            "Energy saver",
            "Managed by Windows",
            None,
        ));
    }
    if let Ok(audio) = crate::win32_audio_endpoint::read() {
        controls.push(QuickControlCapability::new(
            QuickControlKind::Volume,
            QuickControlAvailability::Available {
                active: audio.muted,
            },
            "Sound",
            "Default audio output",
            Some(audio.volume),
        ));
    }
    if let Ok(brightness) = crate::win32_brightness::read() {
        controls.push(QuickControlCapability::new(
            QuickControlKind::Brightness,
            QuickControlAvailability::Available { active: false },
            "Brightness",
            "Display brightness",
            Some(brightness),
        ));
    }
    if let Some(mode) = projection_mode_for_connected_displays() {
        controls.push(QuickControlCapability::new(
            QuickControlKind::Projection,
            QuickControlAvailability::Available {
                active: mode != ProjectionMode::Internal,
            },
            "Projection",
            projection_mode_label(mode),
            Some(projection_mode_value(mode)),
        ));
    }
    QuickSettingsCapabilities::new(controls)
}

fn projection_mode_for_connected_displays() -> Option<ProjectionMode> {
    let facts = display_paths()?
        .iter()
        .map(|path| DisplayPathFacts {
            source: (
                path.sourceInfo.adapterId.HighPart,
                path.sourceInfo.adapterId.LowPart,
                path.sourceInfo.id,
            ),
            target: (
                path.targetInfo.adapterId.HighPart,
                path.targetInfo.adapterId.LowPart,
                path.targetInfo.id,
            ),
            available: path.targetInfo.targetAvailable.as_bool(),
            active: path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0,
            embedded: matches!(
                path.targetInfo.outputTechnology,
                DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                    | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS
                    | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
                    | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
            ),
        })
        .collect::<Vec<_>>();
    let mut connected = Vec::new();
    for path in facts.iter().filter(|path| path.available) {
        if !connected.contains(&path.target) {
            connected.push(path.target);
        }
    }
    (connected.len() > 1).then(|| classify_projection_mode(&facts))
}

fn classify_projection_mode(paths: &[DisplayPathFacts]) -> ProjectionMode {
    let active = paths.iter().filter(|path| path.active).collect::<Vec<_>>();
    let Some(first) = active.first() else {
        return ProjectionMode::Internal;
    };
    if active.len() == 1 {
        return if first.embedded {
            ProjectionMode::Internal
        } else {
            ProjectionMode::External
        };
    }
    if active.iter().all(|path| path.source == first.source) {
        ProjectionMode::Duplicate
    } else {
        ProjectionMode::Extend
    }
}
fn display_paths() -> Option<Vec<DISPLAYCONFIG_PATH_INFO>> {
    for _ in 0..3 {
        let mut path_count = 0_u32;
        let mut mode_count = 0_u32;
        // SAFETY: Both pointers are valid writable counters for the synchronous query.
        if unsafe {
            GetDisplayConfigBufferSizes(QDC_ALL_PATHS, &raw mut path_count, &raw mut mode_count)
        } != ERROR_SUCCESS
        {
            return None;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // SAFETY: Both buffers were allocated from the exact sizes returned above.
        let result = unsafe {
            QueryDisplayConfig(
                QDC_ALL_PATHS,
                &raw mut path_count,
                paths.as_mut_ptr(),
                &raw mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if result != ERROR_SUCCESS {
            return None;
        }
        paths.truncate(path_count as usize);
        return Some(paths);
    }
    None
}

const fn projection_mode_value(mode: ProjectionMode) -> u8 {
    match mode {
        ProjectionMode::Internal => 0,
        ProjectionMode::Duplicate => 1,
        ProjectionMode::Extend => 2,
        ProjectionMode::External => 3,
    }
}

const fn projection_mode_label(mode: ProjectionMode) -> &'static str {
    match mode {
        ProjectionMode::Internal => "PC screen only",
        ProjectionMode::Duplicate => "Duplicate",
        ProjectionMode::Extend => "Extend",
        ProjectionMode::External => "Second screen only",
    }
}

fn direct_controls() -> Vec<QuickControlCapability> {
    [
        (QuickControlKind::NearbySharing, "Nearby sharing", "Off"),
        (QuickControlKind::Focus, "Do not disturb", "Off"),
        (QuickControlKind::Multitasking, "Snap windows", "Off"),
        (QuickControlKind::DarkMode, "Dark mode", "Off"),
        (QuickControlKind::NightLight, "Night light", "Off"),
    ]
    .into_iter()
    .map(|(kind, label, detail)| {
        let availability = if kind == QuickControlKind::Focus {
            let mode = crate::win32_do_not_disturb::read_mode().ok();
            let availability = mode.map_or(
                QuickControlAvailability::RouteOnly { active: None },
                |mode| QuickControlAvailability::Available {
                    active: mode.active(),
                },
            );
            return QuickControlCapability::new(
                kind,
                availability,
                label,
                mode.map_or("Open settings", crate::DoNotDisturbMode::label),
                mode.map(crate::DoNotDisturbMode::value),
            );
        } else if kind == QuickControlKind::NightLight {
            QuickControlAvailability::RouteOnly { active: None }
        } else {
            native_availability(kind)
        };
        QuickControlCapability::new(
            kind,
            availability,
            label,
            match availability {
                QuickControlAvailability::Available { .. } => {
                    direct_control_detail(kind, availability)
                }
                QuickControlAvailability::RouteOnly { .. } => state_detail(availability),
                _ => detail,
            },
            None,
        )
    })
    .collect()
}

const fn direct_control_detail(
    _kind: QuickControlKind,
    availability: QuickControlAvailability,
) -> &'static str {
    state_detail(availability)
}

fn native_availability(kind: QuickControlKind) -> QuickControlAvailability {
    crate::win32_quick_settings_system::read_active(kind)
        .map_or(QuickControlAvailability::TemporarilyUnavailable, |active| {
            QuickControlAvailability::Available { active }
        })
}

fn wifi_availability() -> Option<QuickControlAvailability> {
    let mut table: *mut MIB_IF_TABLE2 = null_mut();
    // SAFETY: The API initializes `table`; successful allocations are released below.
    let result = unsafe { GetIfTable2(&mut table) };
    if result.0 != 0 || table.is_null() {
        return None;
    }
    // SAFETY: A successful table contains NumEntries contiguous rows.
    let facts = unsafe {
        let count = usize::try_from((*table).NumEntries).unwrap_or_default();
        std::slice::from_raw_parts((*table).Table.as_ptr(), count)
            .iter()
            .map(|row| NetworkInterfaceFacts {
                interface_type: row.Type,
                hardware: row.InterfaceAndOperStatusFlags._bitfield & 1 != 0,
                admin_up: row.AdminStatus.0 == 1,
            })
            .collect::<Vec<_>>()
    };
    // SAFETY: The pointer came from GetIfTable2 above.
    unsafe { FreeMibTable(table.cast::<c_void>()) };
    classify_wifi(&facts)
}

fn classify_wifi(rows: &[NetworkInterfaceFacts]) -> Option<QuickControlAvailability> {
    let wireless = rows
        .iter()
        .filter(|row| row.interface_type == IF_TYPE_IEEE80211 && row.hardware)
        .collect::<Vec<_>>();
    (!wireless.is_empty()).then(|| QuickControlAvailability::Available {
        active: wireless.iter().any(|row| row.admin_up),
    })
}

fn bluetooth_radio_present() -> bool {
    let params = BLUETOOTH_FIND_RADIO_PARAMS {
        dwSize: u32::try_from(size_of::<BLUETOOTH_FIND_RADIO_PARAMS>()).unwrap_or_default(),
    };
    let mut radio = HANDLE::default();
    // SAFETY: Both pointers reference writable/value-only storage for the synchronous call.
    let Ok(find) = (unsafe { BluetoothFindFirstRadio(&params, &mut radio) }) else {
        return false;
    };
    // SAFETY: Handles were returned by the corresponding Bluetooth/Win32 APIs.
    let _ = unsafe { CloseHandle(radio) };
    // SAFETY: The find handle came from BluetoothFindFirstRadio above.
    let _ = unsafe { BluetoothFindRadioClose(find) };
    true
}

fn battery_status() -> Option<(u8, bool)> {
    let mut capabilities = SYSTEM_POWER_CAPABILITIES::default();
    // SAFETY: The pointer is writable storage for a synchronous system query.
    if !unsafe { GetPwrCapabilities(&mut capabilities) }
        || !has_real_system_battery(SystemBatteryFacts {
            present: capabilities.SystemBatteriesPresent,
            short_term: capabilities.BatteriesAreShortTerm,
        })
    {
        return None;
    }
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: The pointer is writable storage for a synchronous system query.
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return None;
    }
    (status.BatteryLifePercent <= 100)
        .then_some((status.BatteryLifePercent, status.ACLineStatus == 1))
}

const fn has_real_system_battery(facts: SystemBatteryFacts) -> bool {
    facts.present && !facts.short_term
}

const fn state_detail(availability: QuickControlAvailability) -> &'static str {
    match availability {
        QuickControlAvailability::Available { active: true } => "On",
        QuickControlAvailability::Available { active: false } => "Off",
        QuickControlAvailability::RouteOnly { active: Some(true) } => "On",
        QuickControlAvailability::RouteOnly {
            active: Some(false),
        } => "Off",
        QuickControlAvailability::RouteOnly { active: None } => "Open settings",
        QuickControlAvailability::TemporarilyUnavailable => "Unavailable",
        QuickControlAvailability::Unsupported => "Unsupported",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn do_not_disturb_reports_the_real_mode_and_only_claims_direct_control_when_probed() {
        let focus = direct_controls()
            .into_iter()
            .find(|control| control.kind() == QuickControlKind::Focus)
            .expect("focus control");

        assert_eq!(focus.label(), "Do not disturb");
        assert!(matches!(
            focus.detail(),
            "Off" | "Priority only" | "Alarms only" | "Open settings"
        ));
    }

    #[test]
    fn do_not_disturb_route_uses_the_observed_state_with_a_safe_fallback() {
        assert_eq!(
            state_detail(QuickControlAvailability::RouteOnly { active: Some(true) }),
            "On"
        );
        assert_eq!(
            state_detail(QuickControlAvailability::RouteOnly {
                active: Some(false),
            }),
            "Off"
        );
        assert_eq!(
            state_detail(QuickControlAvailability::RouteOnly { active: None }),
            "Open settings"
        );
    }

    #[test]
    fn wifi_hardware_can_be_present_while_interface_is_disabled() {
        let rows = [NetworkInterfaceFacts {
            interface_type: IF_TYPE_IEEE80211,
            hardware: true,
            admin_up: false,
        }];
        assert_eq!(
            classify_wifi(&rows),
            Some(QuickControlAvailability::Available { active: false })
        );
    }

    #[test]
    fn virtual_wifi_rows_do_not_create_a_hardware_control() {
        let rows = [NetworkInterfaceFacts {
            interface_type: IF_TYPE_IEEE80211,
            hardware: false,
            admin_up: true,
        }];
        assert_eq!(classify_wifi(&rows), None);
    }

    #[test]
    fn ups_only_power_does_not_create_battery_section() {
        assert!(!has_real_system_battery(SystemBatteryFacts {
            present: true,
            short_term: true
        }));
    }

    #[test]
    fn inactive_secondary_display_keeps_projection_available() {
        let paths = [
            DisplayPathFacts {
                source: (0, 1, 0),
                target: (0, 1, 0),
                available: true,
                active: true,
                embedded: true,
            },
            DisplayPathFacts {
                source: (0, 1, 1),
                target: (0, 1, 1),
                available: true,
                active: false,
                embedded: false,
            },
        ];
        assert_eq!(classify_projection_mode(&paths), ProjectionMode::Internal);
        assert_eq!(paths.iter().filter(|path| path.available).count(), 2);
    }
}
