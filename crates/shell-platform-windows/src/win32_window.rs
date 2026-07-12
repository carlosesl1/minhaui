use shell_renderer::native::ShowcaseRole;
use shell_renderer::{
    Dpi, PhysicalRect, ShellMetrics, dock_showcase_rect, popover_anchor_rect, topbar_rect,
};
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::DragAcceptFiles;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DestroyWindow, IsWindowVisible, RegisterClassExW,
    SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos,
    ShowWindow, UnregisterClassW, WINDOW_EX_STYLE, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use crate::win32::{
    LIVE_WINDOWS, register_dock_window, register_popover_window, register_topbar_window,
    unregister_dock_window, unregister_popover_window, unregister_topbar_window,
};
use crate::win32_windowing::window_proc;
use crate::{DockPhysicalPlacement, DockRuntimeConfig};

const CLASS_NAME: PCWSTR = w!("MinhaUi.NativeShell.Window.v1");

pub(super) struct WindowClass {
    instance: HINSTANCE,
}

impl WindowClass {
    pub(super) fn register() -> Result<Self> {
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

pub(super) struct OwnedWindow {
    pub(super) hwnd: HWND,
    pub(super) role: ShowcaseRole,
    title: &'static str,
    pub(super) rect: PhysicalRect,
    dpi: Dpi,
}

impl OwnedWindow {
    pub(super) fn create(
        class: &WindowClass,
        role: ShowcaseRole,
        work: PhysicalRect,
    ) -> Result<Self> {
        let (title_wide, title) = match role {
            ShowcaseRole::Topbar => (w!("Minha UI Topbar"), "Minha UI Topbar"),
            ShowcaseRole::Dock => (w!("Minha UI Dock"), "Minha UI Dock"),
            ShowcaseRole::Popover => (w!("Minha UI Popover"), "Minha UI Popover"),
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
        LIVE_WINDOWS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        match role {
            ShowcaseRole::Topbar => register_topbar_window(hwnd),
            ShowcaseRole::Dock => {
                register_dock_window(hwnd);
                // SAFETY: Category 8 (FFI boundary). The dock HWND is live and owned by
                // this guard; enabling documented shell file-drop delivery is reversible
                // on window destruction.
                unsafe { DragAcceptFiles(hwnd, true) };
            }
            ShowcaseRole::Popover => register_popover_window(hwnd),
        }
        // SAFETY: Category 8 (FFI boundary). `hwnd` was created successfully above
        // and therefore has a stable effective DPI on the primary monitor.
        let dpi = Dpi::from_raw(unsafe { GetDpiForWindow(hwnd) }.max(96));
        let metrics = ShellMetrics::default();
        let rect = match role {
            ShowcaseRole::Topbar => topbar_rect(work, dpi, metrics),
            ShowcaseRole::Dock => dock_showcase_rect(work, dpi, metrics),
            ShowcaseRole::Popover => {
                popover_anchor_rect(topbar_rect(work, dpi, metrics), work, dpi)
            }
        };
        set_rect(hwnd, rect)?;
        // SAFETY: Category 8 (FFI boundary). The live window is shown with the
        // documented non-activating command.
        let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        if role == ShowcaseRole::Popover {
            // SAFETY: Category 8 (FFI boundary). The popover owner starts hidden and
            // is shown only after a typed topbar intent selects active content.
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
        Ok(Self {
            hwnd,
            role,
            title,
            rect,
            dpi,
        })
    }

    pub(super) fn reposition(&mut self, work: PhysicalRect) -> Result<()> {
        // SAFETY: Category 8 (FFI boundary). The owned HWND is live while its
        // current effective DPI is queried for work-area placement.
        self.dpi = Dpi::from_raw(unsafe { GetDpiForWindow(self.hwnd) }.max(96));
        let metrics = ShellMetrics::default();
        self.rect = match self.role {
            ShowcaseRole::Topbar => topbar_rect(work, self.dpi, metrics),
            ShowcaseRole::Dock => dock_showcase_rect(work, self.dpi, metrics),
            ShowcaseRole::Popover => {
                popover_anchor_rect(topbar_rect(work, self.dpi, metrics), work, self.dpi)
            }
        };
        set_rect(self.hwnd, self.rect)
    }

    pub(super) fn place_popover(&mut self, work: PhysicalRect, anchor: PhysicalRect) -> Result<()> {
        if self.role != ShowcaseRole::Popover {
            return Ok(());
        }
        // SAFETY: Category 8 (FFI boundary). The owned HWND is live while its
        // current effective DPI is queried for popover placement.
        self.dpi = Dpi::from_raw(unsafe { GetDpiForWindow(self.hwnd) }.max(96));
        self.rect = popover_anchor_rect(anchor, work, self.dpi);
        set_rect(self.hwnd, self.rect)
    }

    pub(super) fn hide(&self) {
        // SAFETY: Category 8 (FFI boundary). The owned HWND remains valid for this
        // idempotent visibility update.
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    pub(super) fn apply_dock_visibility(
        &mut self,
        work: PhysicalRect,
        config: DockRuntimeConfig,
        hidden: bool,
    ) -> Result<()> {
        if self.role != ShowcaseRole::Dock {
            return Ok(());
        }
        // SAFETY: Category 8 (FFI boundary). The owned HWND is live while its
        // current effective DPI is queried for work-area placement.
        self.dpi = Dpi::from_raw(unsafe { GetDpiForWindow(self.hwnd) }.max(96));
        let normal = dock_showcase_rect(work, self.dpi, ShellMetrics::default());
        let placement =
            DockPhysicalPlacement::from_visibility(normal, config, hidden, self.dpi.scale());
        self.rect = placement.rect();
        set_rect(self.hwnd, self.rect)
    }

    pub(super) fn refresh_rect(&mut self) -> Result<()> {
        let mut rect = windows::Win32::Foundation::RECT::default();
        // SAFETY: Category 8 (FFI boundary). The HWND is live and `rect` is writable
        // storage for this synchronous bounds query.
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(self.hwnd, &mut rect) }?;
        self.rect = PhysicalRect::new(
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
        );
        Ok(())
    }

    pub(super) const fn dpi(&self) -> Dpi {
        self.dpi
    }
}

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        match self.role {
            ShowcaseRole::Topbar => unregister_topbar_window(self.hwnd),
            ShowcaseRole::Dock => unregister_dock_window(self.hwnd),
            ShowcaseRole::Popover => unregister_popover_window(self.hwnd),
        }
        // SAFETY: Category 8 (FFI boundary). This guard is the sole owner of the
        // HWND; DestroyWindow is idempotently skipped by Windows if already closed.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

pub(super) fn print_window(window: &OwnedWindow, device: shell_renderer::native::DeviceKind) {
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

fn set_rect(hwnd: HWND, rect: PhysicalRect) -> Result<()> {
    // SAFETY: Category 8 (FFI boundary). The owned HWND and positive calculated
    // dimensions remain valid for this non-activating placement call.
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
    }
}

const fn role_name(role: ShowcaseRole) -> &'static str {
    match role {
        ShowcaseRole::Topbar => "topbar",
        ShowcaseRole::Dock => "dock",
        ShowcaseRole::Popover => "popover",
    }
}
