use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};
use windows::core::Result;

use crate::external_menu_coordinator::EXTERNAL_MENU_OBSERVATION_INTERVAL_MS;
use crate::win32::{
    DOCK_ANIMATION_TIMER_ID, DOCK_EDGE_PROBE_TIMER_ID, EXTERNAL_MENU_TIMER_ID, PREVIEW_TIMER_ID,
    SYNC_TIMER_ID, TIMER_ID,
};

const DOCK_ANIMATION_INTERVAL_MS: u32 = 16;
pub(super) struct TimerGuard {
    hwnd: HWND,
    id: usize,
    interval_ms: u32,
}

impl TimerGuard {
    pub(super) fn start(hwnd: HWND, milliseconds: u32) -> Result<Self> {
        Self::start_id(hwnd, TIMER_ID, milliseconds.clamp(100, 60_000))
    }

    pub(super) fn start_sync(hwnd: HWND) -> Result<Self> {
        Self::start_id(hwnd, SYNC_TIMER_ID, 1_000)
    }

    pub(super) fn start_dock_animation(hwnd: HWND) -> Result<Self> {
        Self::start_id(hwnd, DOCK_ANIMATION_TIMER_ID, DOCK_ANIMATION_INTERVAL_MS)
    }

    pub(super) fn start_dock_edge_probe(hwnd: HWND, interval_ms: u32) -> Result<Self> {
        Self::start_id(hwnd, DOCK_EDGE_PROBE_TIMER_ID, interval_ms)
    }

    pub(super) fn start_preview(hwnd: HWND) -> Result<Self> {
        Self::start_id(hwnd, PREVIEW_TIMER_ID, 16)
    }

    pub(super) fn start_external_menu(hwnd: HWND) -> Result<Self> {
        Self::start_id(
            hwnd,
            EXTERNAL_MENU_TIMER_ID,
            EXTERNAL_MENU_OBSERVATION_INTERVAL_MS,
        )
    }

    fn start_id(hwnd: HWND, id: usize, milliseconds: u32) -> Result<Self> {
        // SAFETY: Category 8 (FFI boundary). The live HWND owns the numeric timer;
        // messages are delivered to its window procedure without a callback pointer.
        let timer = unsafe { SetTimer(Some(hwnd), id, milliseconds, None) };
        if timer == 0 {
            return Err(windows::core::Error::from_thread());
        }
        Ok(Self {
            hwnd,
            id,
            interval_ms: milliseconds,
        })
    }

    pub(super) fn rearm(&mut self, milliseconds: u32) -> Result<()> {
        if self.interval_ms == milliseconds {
            return Ok(());
        }
        // SAFETY: Category 8 (FFI boundary). Reusing the owned HWND/id pair
        // updates the existing timer cadence without changing its owner.
        let timer = unsafe { SetTimer(Some(self.hwnd), self.id, milliseconds, None) };
        if timer == 0 {
            return Err(windows::core::Error::from_thread());
        }
        self.interval_ms = milliseconds;
        Ok(())
    }
}

impl Drop for TimerGuard {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). This guard uniquely owns its timer id and
        // cancellation occurs before its HWND owner is dropped.
        let _ = unsafe { KillTimer(Some(self.hwnd), self.id) };
    }
}

#[cfg(test)]
mod tests {
    use super::DOCK_ANIMATION_INTERVAL_MS;

    #[test]
    fn dock_animation_fallback_timer_matches_the_sixty_hz_frame_budget() {
        assert_eq!(DOCK_ANIMATION_INTERVAL_MS, 16);
    }
}
