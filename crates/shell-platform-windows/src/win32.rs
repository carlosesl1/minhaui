use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, Ordering};

use shell_core::{AppId, DockItem, DockItemId, ShellState};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;
use windows::core::{Result, w};

use crate::win32_owner::RuntimeSurfaces;
use crate::win32_timer::TimerGuard;
use crate::win32_window::{OwnedWindow, WindowClass, print_window};
use crate::win32_windowing::{message_loop, primary_work_area};
use crate::{DockController, DockRuntimeConfig, PlatformEvent};

pub(super) const TIMER_ID: usize = 0x4D55;
pub(super) const SYNC_TIMER_ID: usize = 0x4D56;
pub(super) static LIVE_WINDOWS: AtomicI32 = AtomicI32::new(0);
pub(super) static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
pub(super) static DOCK_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub(super) static DOCK_DRAGGING: AtomicBool = AtomicBool::new(false);

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

    let work_area = primary_work_area()?;
    let mut topbar = OwnedWindow::create(
        &class,
        shell_renderer::native::ShowcaseRole::Topbar,
        work_area,
    )?;
    let mut dock = OwnedWindow::create(
        &class,
        shell_renderer::native::ShowcaseRole::Dock,
        work_area,
    )?;
    let timer = qa_exit_ms
        .map(|milliseconds| TimerGuard::start(topbar.hwnd, milliseconds))
        .transpose()?;
    let sync_timer = TimerGuard::start_sync(dock.hwnd)?;
    let dock_controller = sample_dock_controller()?;
    let mut runtime = RuntimeSurfaces::new(force_warp, &topbar, &dock, dock_controller)?;
    runtime.handle_event(PlatformEvent::SyncWindows, &mut topbar, &mut dock)?;
    print_window(&topbar, runtime.device_kind());
    print_window(&dock, runtime.device_kind());
    if simulate_device_loss_once {
        runtime.handle_event(PlatformEvent::DeviceLost, &mut topbar, &mut dock)?;
    }
    if simulate_lifecycle_events {
        for event in [
            PlatformEvent::DisplayChanged,
            PlatformEvent::PowerResumed,
            PlatformEvent::TaskbarCreated,
        ] {
            runtime.handle_event(event, &mut topbar, &mut dock)?;
        }
    }
    let result = message_loop(|event| runtime.handle_event(event, &mut topbar, &mut dock));
    drop(runtime);
    drop(sync_timer);
    drop(timer);
    drop(dock);
    drop(topbar);
    drop(class);
    result
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

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}
