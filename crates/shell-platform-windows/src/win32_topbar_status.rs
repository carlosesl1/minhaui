use std::ffi::c_void;
use std::ptr::null_mut;

use windows::Win32::Media::Audio::waveOutGetVolume;
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, MIB_IF_TABLE2, MIB_IF_TYPE_LOOPBACK,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Shell::{QUNS_ACCEPTS_NOTIFICATIONS, SHQueryUserNotificationState};

use crate::{NetworkSnapshot, PowerSnapshot, TopbarSnapshot};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct NetworkTotals {
    received: u64,
    sent: u64,
    online: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NetworkSample {
    totals: NetworkTotals,
    sampled_ms: u64,
}

#[derive(Debug, Default)]
pub(super) struct TopbarStatusReader {
    network: Option<NetworkSample>,
}

impl TopbarStatusReader {
    pub(super) fn snapshot(&mut self, now_ms: u64) -> TopbarSnapshot {
        TopbarSnapshot::new(
            clock_label(),
            self.network_snapshot(now_ms),
            volume_percent(),
            power_snapshot(),
            notification_indicator(),
        )
    }

    fn network_snapshot(&mut self, now_ms: u64) -> NetworkSnapshot {
        let current = network_totals();
        let previous = self.network.replace(NetworkSample {
            totals: current,
            sampled_ms: now_ms,
        });
        let Some(previous) = previous else {
            return network_from_rates(current.online, 0, 0);
        };
        let elapsed_ms = now_ms.saturating_sub(previous.sampled_ms);
        if elapsed_ms == 0 {
            return network_from_rates(current.online, 0, 0);
        }
        let received = kib_per_second(
            current.received.saturating_sub(previous.totals.received),
            elapsed_ms,
        );
        let sent = kib_per_second(
            current.sent.saturating_sub(previous.totals.sent),
            elapsed_ms,
        );
        network_from_rates(current.online, received, sent)
    }
}

fn clock_label() -> String {
    // SAFETY: Category 8 (FFI boundary). GetLocalTime writes and returns a value
    // struct with no borrowed pointers.
    let local = unsafe { GetLocalTime() };
    format!(
        "{:02}:{:02} {} {:02}",
        local.wHour,
        local.wMinute,
        weekday(local.wDayOfWeek),
        local.wDay
    )
}

fn network_totals() -> NetworkTotals {
    let mut table: *mut MIB_IF_TABLE2 = null_mut();
    // SAFETY: Category 8 (FFI boundary). The API initializes `table` on success;
    // successful allocations are released with FreeMibTable before returning.
    let result = unsafe { GetIfTable2(&mut table) };
    if result.0 != 0 || table.is_null() {
        return NetworkTotals::default();
    }
    // SAFETY: Category 8 (FFI boundary). A successful GetIfTable2 returns a table
    // with NumEntries contiguous rows starting at Table[0].
    let totals = unsafe { totals_from_table(&*table) };
    // SAFETY: Category 8 (FFI boundary). The pointer came from GetIfTable2 above.
    unsafe { FreeMibTable(table.cast::<c_void>()) };
    totals
}

unsafe fn totals_from_table(table: &MIB_IF_TABLE2) -> NetworkTotals {
    let Ok(count) = usize::try_from(table.NumEntries) else {
        return NetworkTotals::default();
    };
    // SAFETY: Category 8 (FFI boundary). Caller guarantees the MIB table contains
    // `count` rows as documented by GetIfTable2.
    let rows = unsafe { std::slice::from_raw_parts(table.Table.as_ptr(), count) };
    rows.iter()
        .filter(|row| row.Type != MIB_IF_TYPE_LOOPBACK)
        .fold(NetworkTotals::default(), |mut totals, row| {
            totals.received = totals.received.saturating_add(row.InOctets);
            totals.sent = totals.sent.saturating_add(row.OutOctets);
            totals.online |= row.OperStatus.0 == 1;
            totals
        })
}

fn volume_percent() -> u8 {
    let mut raw = 0_u32;
    // SAFETY: Category 8 (FFI boundary). Passing no HWAVEOUT queries the preferred
    // wave output device and writes one u32 volume value.
    if unsafe { waveOutGetVolume(None, &mut raw) } != 0 {
        return 0;
    }
    let left = raw & 0xffff;
    let right = (raw >> 16) & 0xffff;
    let percent = ((left + right) / 2).saturating_mul(100) / 0xffff;
    u8::try_from(percent).unwrap_or(100)
}

fn power_snapshot() -> PowerSnapshot {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: Category 8 (FFI boundary). The pointer is writable storage for the
    // synchronous system power query.
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return PowerSnapshot::new(None, false);
    }
    let percent = match status.BatteryLifePercent {
        0..=100 => Some(status.BatteryLifePercent),
        _ => None,
    };
    PowerSnapshot::new(percent, status.ACLineStatus == 1)
}

fn notification_indicator() -> u16 {
    // SAFETY: Category 8 (FFI boundary). The shell API returns an enum value.
    match unsafe { SHQueryUserNotificationState() } {
        Ok(state) if state == QUNS_ACCEPTS_NOTIFICATIONS => 0,
        Ok(_) | Err(_) => 1,
    }
}

fn network_from_rates(online: bool, received: u32, sent: u32) -> NetworkSnapshot {
    NetworkSnapshot::new(if online { "Online" } else { "Offline" }, received, sent)
}

fn kib_per_second(bytes: u64, elapsed_ms: u64) -> u32 {
    let kib = bytes.saturating_mul(1_000) / elapsed_ms / 1_024;
    u32::try_from(kib).unwrap_or(u32::MAX)
}

const fn weekday(value: u16) -> &'static str {
    match value {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "Day",
    }
}
