use std::sync::atomic::{AtomicI32, AtomicIsize, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;
use windows::core::{Result, w};

use crate::PlatformEvent;
use crate::win32_slots::{
    create_slots, dispatch_event, handle_broadcast_event, print_monitor_placements,
};
use crate::win32_timer::TimerGuard;
use crate::win32_window::WindowClass;
use crate::win32_windowing::{message_loop, monitor_placement_inputs};

pub(super) const TIMER_ID: usize = 0x4D55;
pub(super) const SYNC_TIMER_ID: usize = 0x4D56;
pub(super) static LIVE_WINDOWS: AtomicI32 = AtomicI32::new(0);
pub(super) static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
pub(super) static DOCK_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub(super) static TOPBAR_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub(super) static POPOVER_WINDOW: AtomicIsize = AtomicIsize::new(0);
static DOCK_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();
static TOPBAR_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();
static POPOVER_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();

pub fn run_showcase(
    force_warp: bool,
    qa_exit_ms: Option<u32>,
    simulate_device_loss_once: bool,
    simulate_lifecycle_events: bool,
) -> Result<()> {
    // SAFETY: Category 8 (FFI boundary). DPI awareness is set before any HWND is
    // created and uses the documented process-wide per-monitor-v2 constant.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }?;
    let class = WindowClass::register()?;
    // SAFETY: Category 8 (FFI boundary). The registered message name is a static,
    // null-terminated UTF-16 string and the returned identifier is process-global.
    let taskbar_message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    TASKBAR_CREATED.store(taskbar_message, Ordering::Release);

    let monitors = monitor_placement_inputs()?;
    if monitors.is_empty() {
        return Err(windows::core::Error::new(
            invalid_arg(),
            "no display monitors were enumerated",
        ));
    }
    print_monitor_placements(&monitors);
    let mut slots = create_slots(&class, &monitors, force_warp)?;
    let timer = qa_exit_ms
        .map(|milliseconds| TimerGuard::start(slots[0].topbar_hwnd(), milliseconds))
        .transpose()?;
    for slot in &mut slots {
        slot.handle_event(PlatformEvent::SyncWindows)?;
        slot.print_windows();
    }
    if simulate_device_loss_once {
        for slot in &mut slots {
            slot.handle_event(PlatformEvent::DeviceLost)?;
        }
    }
    if simulate_lifecycle_events {
        for event in [
            PlatformEvent::DisplayChanged,
            PlatformEvent::PowerResumed,
            PlatformEvent::TaskbarCreated,
        ] {
            handle_broadcast_event(&class, force_warp, &mut slots, event)?;
        }
    }
    let result = message_loop(|event| dispatch_event(&class, force_warp, &mut slots, event));
    drop(slots);
    drop(timer);
    drop(class);
    result
}

pub(super) fn register_dock_window(hwnd: HWND) {
    register_window(hwnd, &DOCK_WINDOWS, &DOCK_WINDOW);
}

pub(super) fn unregister_dock_window(hwnd: HWND) {
    unregister_window(hwnd, &DOCK_WINDOWS, &DOCK_WINDOW);
}

pub(super) fn register_topbar_window(hwnd: HWND) {
    register_window(hwnd, &TOPBAR_WINDOWS, &TOPBAR_WINDOW);
}

pub(super) fn unregister_topbar_window(hwnd: HWND) {
    unregister_window(hwnd, &TOPBAR_WINDOWS, &TOPBAR_WINDOW);
}

pub(super) fn register_popover_window(hwnd: HWND) {
    register_window(hwnd, &POPOVER_WINDOWS, &POPOVER_WINDOW);
}

pub(super) fn unregister_popover_window(hwnd: HWND) {
    unregister_window(hwnd, &POPOVER_WINDOWS, &POPOVER_WINDOW);
}

pub(super) fn is_dock_window(hwnd: HWND) -> bool {
    contains_window(hwnd, &DOCK_WINDOWS, &DOCK_WINDOW)
}

pub(super) fn is_topbar_window(hwnd: HWND) -> bool {
    contains_window(hwnd, &TOPBAR_WINDOWS, &TOPBAR_WINDOW)
}

pub(super) fn is_popover_window(hwnd: HWND) -> bool {
    contains_window(hwnd, &POPOVER_WINDOWS, &POPOVER_WINDOW)
}

fn register_window(
    hwnd: HWND,
    registry: &'static OnceLock<Mutex<Vec<isize>>>,
    latest: &'static AtomicIsize,
) {
    let raw = hwnd.0 as isize;
    let windows = registry.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut values) = windows.lock()
        && !values.contains(&raw)
    {
        values.push(raw);
    }
    latest.store(raw, Ordering::Release);
}

fn unregister_window(
    hwnd: HWND,
    registry: &'static OnceLock<Mutex<Vec<isize>>>,
    latest: &'static AtomicIsize,
) {
    let raw = hwnd.0 as isize;
    if let Some(windows) = registry.get()
        && let Ok(mut values) = windows.lock()
    {
        values.retain(|value| *value != raw);
        if latest.load(Ordering::Acquire) == raw {
            latest.store(
                values.last().copied().unwrap_or_default(),
                Ordering::Release,
            );
        }
        return;
    }
    if latest.load(Ordering::Acquire) == raw {
        latest.store(0, Ordering::Release);
    }
}

fn contains_window(
    hwnd: HWND,
    registry: &'static OnceLock<Mutex<Vec<isize>>>,
    latest: &'static AtomicIsize,
) -> bool {
    let raw = hwnd.0 as isize;
    latest.load(Ordering::Acquire) == raw
        || registry
            .get()
            .and_then(|windows| windows.lock().ok().map(|values| values.contains(&raw)))
            .unwrap_or(false)
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}
