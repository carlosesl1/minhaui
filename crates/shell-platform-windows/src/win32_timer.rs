use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};
use windows::core::Result;

use crate::win32::{SYNC_TIMER_ID, TIMER_ID};

pub(super) struct TimerGuard {
    hwnd: HWND,
    id: usize,
}

impl TimerGuard {
    pub(super) fn start(hwnd: HWND, milliseconds: u32) -> Result<Self> {
        Self::start_id(hwnd, TIMER_ID, milliseconds)
    }

    pub(super) fn start_sync(hwnd: HWND) -> Result<Self> {
        Self::start_id(hwnd, SYNC_TIMER_ID, 1_000)
    }

    fn start_id(hwnd: HWND, id: usize, milliseconds: u32) -> Result<Self> {
        let bounded = milliseconds.clamp(100, 60_000);
        // SAFETY: Category 8 (FFI boundary). The live HWND owns the numeric timer;
        // messages are delivered to its window procedure without a callback pointer.
        let timer = unsafe { SetTimer(Some(hwnd), id, bounded, None) };
        if timer == 0 {
            return Err(windows::core::Error::from_thread());
        }
        Ok(Self { hwnd, id })
    }
}

impl Drop for TimerGuard {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). This guard uniquely owns its timer id and
        // cancellation occurs before its HWND owner is dropped.
        let _ = unsafe { KillTimer(Some(self.hwnd), self.id) };
    }
}
