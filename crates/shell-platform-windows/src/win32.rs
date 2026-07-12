use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use shell_core::{AppId, DockItem, DockItemId, ShellState};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;
use windows::core::{Result, w};

use crate::win32_owner::RuntimeSurfaces;
use crate::win32_timer::TimerGuard;
use crate::win32_window::{OwnedWindow, WindowClass, print_window};
use crate::win32_windowing::{message_loop, monitor_placement_inputs};
use crate::{DockController, DockRuntimeConfig, PlatformEvent};

pub(super) const TIMER_ID: usize = 0x4D55;
pub(super) const SYNC_TIMER_ID: usize = 0x4D56;
pub(super) static LIVE_WINDOWS: AtomicI32 = AtomicI32::new(0);
pub(super) static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
pub(super) static DOCK_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub(super) static DOCK_DRAGGING: AtomicBool = AtomicBool::new(false);
static DOCK_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();

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
        .map(|milliseconds| TimerGuard::start(slots[0].topbar.hwnd, milliseconds))
        .transpose()?;
    let sync_timers = slots
        .iter()
        .map(|slot| TimerGuard::start_sync(slot.dock.hwnd))
        .collect::<Result<Vec<_>>>()?;
    for slot in &mut slots {
        slot.handle_event(PlatformEvent::SyncWindows)?;
        print_window(&slot.topbar, slot.runtime.device_kind());
        print_window(&slot.dock, slot.runtime.device_kind());
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
            for slot in &mut slots {
                slot.handle_event(event.clone())?;
            }
        }
    }
    let result = message_loop(|event| {
        for slot in &mut slots {
            if !slot.handle_event(event.clone())? {
                return Ok(false);
            }
        }
        Ok(true)
    });
    drop(slots);
    drop(sync_timers);
    drop(timer);
    drop(class);
    result
}

struct ShellSlot {
    topbar: OwnedWindow,
    dock: OwnedWindow,
    runtime: RuntimeSurfaces,
}

impl ShellSlot {
    fn handle_event(&mut self, event: PlatformEvent) -> Result<bool> {
        self.runtime
            .handle_event(event, &mut self.topbar, &mut self.dock)
    }
}

fn create_slots(
    class: &WindowClass,
    monitors: &[crate::MonitorPlacementInput],
    force_warp: bool,
) -> Result<Vec<ShellSlot>> {
    let mut slots = Vec::new();
    for monitor in monitors {
        let topbar = OwnedWindow::create(
            class,
            shell_renderer::native::ShowcaseRole::Topbar,
            monitor.work_area(),
        )?;
        let dock = OwnedWindow::create(
            class,
            shell_renderer::native::ShowcaseRole::Dock,
            monitor.work_area(),
        )?;
        let dock_controller = sample_dock_controller()?;
        let runtime = RuntimeSurfaces::new(force_warp, &topbar, &dock, dock_controller)?;
        slots.push(ShellSlot {
            topbar,
            dock,
            runtime,
        });
    }
    Ok(slots)
}

fn print_monitor_placements(monitors: &[crate::MonitorPlacementInput]) {
    let fullscreen = Vec::new();
    for placement in crate::plan_monitor_placements(
        monitors,
        DockRuntimeConfig::default(),
        fullscreen.as_slice(),
    ) {
        let dock = placement.dock().rect();
        println!(
            "MONITOR_PLACEMENT id={} edge={:?} suppressed={} dock={},{},{},{}",
            placement.monitor().value(),
            placement.taskbar_edge(),
            placement.suppressed(),
            dock.x,
            dock.y,
            dock.width,
            dock.height
        );
    }
}

fn sample_dock_controller() -> Result<DockController> {
    let items = vec![
        DockItem::pinned(DockItemId::new(1), parse_app("notepad.exe")?),
        DockItem::pinned(DockItemId::new(2), parse_app("calc.exe")?),
        DockItem::pinned(DockItemId::new(3), parse_app("explorer.exe")?),
    ];
    DockController::new(
        ShellState::default().with_dock_items(items),
        DockRuntimeConfig::default().with_autohide(true),
    )
    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))
}

fn parse_app(value: &str) -> Result<AppId> {
    AppId::parse(value).map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))
}

pub(super) fn register_dock_window(hwnd: HWND) {
    let raw = hwnd.0 as isize;
    let windows = DOCK_WINDOWS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut values) = windows.lock()
        && !values.contains(&raw)
    {
        values.push(raw);
    }
    DOCK_WINDOW.store(raw, Ordering::Release);
}

pub(super) fn unregister_dock_window(hwnd: HWND) {
    let raw = hwnd.0 as isize;
    if let Some(windows) = DOCK_WINDOWS.get()
        && let Ok(mut values) = windows.lock()
    {
        values.retain(|value| *value != raw);
        if DOCK_WINDOW.load(Ordering::Acquire) == raw {
            DOCK_WINDOW.store(
                values.last().copied().unwrap_or_default(),
                Ordering::Release,
            );
        }
        return;
    }
    if DOCK_WINDOW.load(Ordering::Acquire) == raw {
        DOCK_WINDOW.store(0, Ordering::Release);
    }
}

pub(super) fn is_dock_window(hwnd: HWND) -> bool {
    let raw = hwnd.0 as isize;
    DOCK_WINDOW.load(Ordering::Acquire) == raw
        || DOCK_WINDOWS
            .get()
            .and_then(|windows| windows.lock().ok().map(|values| values.contains(&raw)))
            .unwrap_or(false)
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}
