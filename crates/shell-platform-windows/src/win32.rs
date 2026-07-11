use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use shell_renderer::native::{CompositionRenderer, ShowcaseRole, WindowSurface};
use shell_renderer::{Dpi, PhysicalRect, ShellMetrics, dock_showcase_rect, topbar_rect};
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DestroyWindow, IsWindowVisible, KillTimer,
    RegisterClassExW, RegisterWindowMessageW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER,
    SWP_SHOWWINDOW, SetTimer, SetWindowPos, ShowWindow, UnregisterClassW, WINDOW_EX_STYLE,
    WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use crate::win32_windowing::{message_loop, primary_work_area, window_proc};

const CLASS_NAME: PCWSTR = w!("MinhaUi.NativeShell.Window.v1");
pub(super) const TIMER_ID: usize = 0x4D55;
pub(super) static LIVE_WINDOWS: AtomicI32 = AtomicI32::new(0);
pub(super) static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub fn run_showcase(force_warp: bool, qa_exit_ms: Option<u32>) -> Result<()> {
    // SAFETY: Category 8 (FFI boundary). DPI awareness is set before any HWND is
    // created and uses the documented process-wide per-monitor-v2 constant.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }?;
    let class = WindowClass::register()?;
    // SAFETY: Category 8 (FFI boundary). The registered message name is a static,
    // null-terminated UTF-16 string and the returned identifier is process-global.
    let taskbar_message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    TASKBAR_CREATED.store(taskbar_message, Ordering::Release);

    let work_area = primary_work_area()?;
    let topbar = OwnedWindow::create(&class, ShowcaseRole::Topbar, work_area)?;
    let dock = OwnedWindow::create(&class, ShowcaseRole::Dock, work_area)?;
    let renderer = CompositionRenderer::new(force_warp)?;
    let topbar_surface = renderer.create_surface(
        topbar.hwnd,
        ShowcaseRole::Topbar,
        topbar.rect.width.max(1) as u32,
        topbar.rect.height.max(1) as u32,
    )?;
    let dock_surface = renderer.create_surface(
        dock.hwnd,
        ShowcaseRole::Dock,
        dock.rect.width.max(1) as u32,
        dock.rect.height.max(1) as u32,
    )?;
    let timer = match qa_exit_ms {
        Some(milliseconds) => Some(TimerGuard::start(topbar.hwnd, milliseconds)?),
        None => None,
    };
    print_window(&topbar, renderer.device_kind());
    print_window(&dock, renderer.device_kind());

    let _owner = NativeOwner {
        _topbar_surface: topbar_surface,
        _dock_surface: dock_surface,
        _renderer: renderer,
        _timer: timer,
        _topbar: topbar,
        _dock: dock,
        _class: class,
    };
    message_loop()
}

struct NativeOwner {
    _topbar_surface: WindowSurface,
    _dock_surface: WindowSurface,
    _renderer: CompositionRenderer,
    _timer: Option<TimerGuard>,
    _topbar: OwnedWindow,
    _dock: OwnedWindow,
    _class: WindowClass,
}

struct WindowClass {
    instance: HINSTANCE,
}

impl WindowClass {
    fn register() -> Result<Self> {
        // SAFETY: Category 8 (FFI boundary). Passing no module name returns the
        // current executable module, which stays loaded through process shutdown.
        let module = unsafe { GetModuleHandleW(None) }?;
        let instance = HINSTANCE(module.0);
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        // SAFETY: Category 8 (FFI boundary). `class` is fully initialized, its
        // callback uses the system ABI, and all borrowed strings have static life.
        let atom = unsafe { RegisterClassExW(&class) };
        if atom == 0 {
            return Err(windows::core::Error::from_thread());
        }
        Ok(Self { instance })
    }
}

impl Drop for WindowClass {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). Both windows are earlier fields in the
        // owner and are destroyed before this class guard is dropped.
        let _ = unsafe { UnregisterClassW(CLASS_NAME, Some(self.instance)) };
    }
}

struct OwnedWindow {
    hwnd: HWND,
    role: ShowcaseRole,
    title: &'static str,
    rect: PhysicalRect,
    dpi: Dpi,
}

impl OwnedWindow {
    fn create(class: &WindowClass, role: ShowcaseRole, work: PhysicalRect) -> Result<Self> {
        let (title_wide, title) = match role {
            ShowcaseRole::Topbar => (w!("Minha UI Topbar"), "Minha UI Topbar"),
            ShowcaseRole::Dock => (w!("Minha UI Dock"), "Minha UI Dock"),
        };
        let ex_style: WINDOW_EX_STYLE = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        // SAFETY: Category 8 (FFI boundary). The class is registered, parameters are
        // value types or static strings, and no application pointer crosses the API.
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                CLASS_NAME,
                title_wide,
                WS_POPUP,
                work.x,
                work.y,
                1,
                1,
                None,
                None,
                Some(class.instance),
                None,
            )
        }?;
        LIVE_WINDOWS.fetch_add(1, Ordering::AcqRel);
        // SAFETY: Category 8 (FFI boundary). `hwnd` was created successfully above
        // and therefore has a stable effective DPI on the primary monitor.
        let dpi = Dpi::from_raw(unsafe { GetDpiForWindow(hwnd) }.max(96));
        let metrics = ShellMetrics::default();
        let rect = match role {
            ShowcaseRole::Topbar => topbar_rect(work, dpi, metrics),
            ShowcaseRole::Dock => dock_showcase_rect(work, dpi, metrics),
        };
        // SAFETY: Category 8 (FFI boundary). The dimensions are positive placement
        // results and NOACTIVATE preserves the tool-window interaction contract.
        unsafe {
            SetWindowPos(
                hwnd,
                None,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        }?;
        // SAFETY: Category 8 (FFI boundary). The live window is shown with the
        // documented non-activating command.
        let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        Ok(Self {
            hwnd,
            role,
            title,
            rect,
            dpi,
        })
    }
}

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). This guard is the sole owner of the
        // HWND; DestroyWindow is idempotently skipped by Windows if already closed.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

struct TimerGuard {
    hwnd: HWND,
}

impl TimerGuard {
    fn start(hwnd: HWND, milliseconds: u32) -> Result<Self> {
        let bounded = milliseconds.clamp(100, 60_000);
        // SAFETY: Category 8 (FFI boundary). The live HWND owns the numeric timer;
        // messages are delivered to its window procedure without a callback pointer.
        let timer = unsafe { SetTimer(Some(hwnd), TIMER_ID, bounded, None) };
        if timer == 0 {
            return Err(windows::core::Error::from_thread());
        }
        Ok(Self { hwnd })
    }
}

impl Drop for TimerGuard {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). This guard uniquely owns TIMER_ID for
        // its HWND and cancellation occurs before the HWND owner is dropped.
        let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_ID) };
    }
}

fn print_window(window: &OwnedWindow, device: shell_renderer::native::DeviceKind) {
    // SAFETY: Category 8 (FFI boundary). The owner guarantees the HWND is live while
    // its metadata line is emitted.
    let visible = unsafe { IsWindowVisible(window.hwnd) }.as_bool();
    println!(
        "WINDOW role={} hwnd={:p} title=\"{}\" class=\"MinhaUi.NativeShell.Window.v1\" rect={},{},{},{} dpi={} visible={} renderer={}",
        role_name(window.role),
        window.hwnd.0,
        window.title,
        window.rect.x,
        window.rect.y,
        window.rect.width,
        window.rect.height,
        window.dpi.raw(),
        visible,
        match device {
            shell_renderer::native::DeviceKind::Hardware => "hardware",
            shell_renderer::native::DeviceKind::Warp => "warp",
        }
    );
}

const fn role_name(role: ShowcaseRole) -> &'static str {
    match role {
        ShowcaseRole::Topbar => "topbar",
        ShowcaseRole::Dock => "dock",
    }
}
