use core::ffi::c_void;

use shell_renderer::native::ShowcaseRole;
use shell_renderer::{
    Dpi, PhysicalRect, PopoverSurfaceSize, ShellMetrics, context_menu_anchor_rect,
    dock_showcase_rect, physical_from_dip, popover_anchor_rect, popover_placement,
    topbar_height_for_text_scale, topbar_rect,
};
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::DragAcceptFiles;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowRect,
    HTTRANSPARENT, HWND_NOTOPMOST, HWND_TOPMOST, IsWindowVisible, RegisterClassExW, SW_HIDE,
    SW_SHOW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowPos, ShowWindow, UnregisterClassW, WINDOW_EX_STYLE,
    WM_ERASEBKGND, WM_NCHITTEST, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

use crate::win32::{
    LIVE_WINDOWS, register_app_menu_window, register_dock_window, register_popover_window,
    register_preview_window, register_settings_window, register_topbar_window,
    unregister_app_menu_window, unregister_dock_window, unregister_popover_window,
    unregister_preview_window, unregister_settings_window, unregister_topbar_window,
};
use crate::win32_backdrop::apply_if_supported;
use crate::win32_windowing::window_proc;
use crate::{DockEdgeGeometry, DockPhysicalPlacement, DockRuntimeConfig, NativeWindowId};

const CLASS_NAME: PCWSTR = w!("MinhaUi.NativeShell.Window.v1");
pub(super) const THUMBNAIL_HOST_CLASS_NAME: PCWSTR = w!("MinhaUi.NativeShell.ThumbnailHost.v1");

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
        let thumbnail_host_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(thumbnail_host_window_proc),
            hInstance: instance,
            lpszClassName: THUMBNAIL_HOST_CLASS_NAME,
            ..Default::default()
        };
        // SAFETY: Category 8 (FFI boundary). This lightweight transparent class
        // owns only DWM thumbnails and never participates in shell event routing.
        let thumbnail_atom = unsafe { RegisterClassExW(&thumbnail_host_class) };
        if thumbnail_atom == 0 {
            // SAFETY: Category 8 (FFI boundary). The primary class was registered
            // immediately above and no window has been created from it yet.
            let _ = unsafe { UnregisterClassW(CLASS_NAME, Some(instance)) };
            return Err(windows::core::Error::from_thread());
        }
        Ok(Self { instance })
    }
}

impl Drop for WindowClass {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). Both windows are earlier fields in the
        // owner and are destroyed before this class guard is dropped.
        let _ = unsafe { UnregisterClassW(THUMBNAIL_HOST_CLASS_NAME, Some(self.instance)) };
        // SAFETY: Category 8 (FFI boundary). All shell windows are destroyed before
        // their shared registered class guard is dropped.
        let _ = unsafe { UnregisterClassW(CLASS_NAME, Some(self.instance)) };
    }
}

unsafe extern "system" fn thumbnail_host_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    match message {
        WM_NCHITTEST => windows::Win32::Foundation::LRESULT(HTTRANSPARENT as isize),
        WM_ERASEBKGND => windows::Win32::Foundation::LRESULT(1),
        _ => {
            // SAFETY: Category 8 (FFI boundary). Unhandled host messages and their
            // exact parameters are forwarded to the documented default procedure.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}

pub(super) struct OwnedWindow {
    pub(super) hwnd: HWND,
    pub(super) role: ShowcaseRole,
    title: &'static str,
    pub(super) rect: PhysicalRect,
    dpi: Dpi,
    dock_width_dip: f32,
    dock_height_dip: f32,
    topbar_height_dip: f32,
    popover_width_dip: f32,
    popover_height_dip: f32,
    popover_anchor_x_dip: f32,
    backdrop_allowed: bool,
    backdrop_active: bool,
}

pub(super) struct ExternalMenuPopoverLayerGuard {
    hwnd: HWND,
}

impl ExternalMenuPopoverLayerGuard {
    pub(super) fn lower(popover: &OwnedWindow) -> Result<Self> {
        set_external_menu_popover_layer(popover.hwnd, true)?;
        Ok(Self { hwnd: popover.hwnd })
    }
}

impl Drop for ExternalMenuPopoverLayerGuard {
    fn drop(&mut self) {
        let _ = set_external_menu_popover_layer(self.hwnd, false);
    }
}

const fn external_menu_popover_is_topmost(active: bool) -> bool {
    !active
}

fn set_external_menu_popover_layer(hwnd: HWND, active: bool) -> Result<()> {
    let insert_after = if external_menu_popover_is_topmost(active) {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    // SAFETY: The owned popover HWND remains live for the guard lifetime. Only
    // its z-order band changes; geometry and activation are preserved.
    unsafe {
        SetWindowPos(
            hwnd,
            Some(insert_after),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    }
}

pub(super) fn place_external_menu(
    window: NativeWindowId,
    anchor: PhysicalRect,
    work: PhysicalRect,
) -> Result<()> {
    let hwnd = HWND(window.value() as *mut c_void);
    let mut native_rect = windows::Win32::Foundation::RECT::default();
    // SAFETY: The menu HWND originates from EVENT_SYSTEM_MENUPOPUPSTART and is
    // queried synchronously while that popup is live.
    unsafe { GetWindowRect(hwnd, &mut native_rect) }?;
    let rect = external_menu_placement(
        native_rect.right.saturating_sub(native_rect.left).max(1),
        native_rect.bottom.saturating_sub(native_rect.top).max(1),
        anchor,
        work,
    );
    // SAFETY: Only the live menu popup's position and z-order are adjusted. Its
    // owner retains activation, sizing, command routing, and lifetime.
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            rect.x,
            rect.y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        )
    }
}

fn external_menu_placement(
    width: i32,
    height: i32,
    anchor: PhysicalRect,
    work: PhysicalRect,
) -> PhysicalRect {
    const GAP: i32 = 8;

    let width = width.max(1);
    let height = height.max(1);
    let max_x = work
        .x
        .saturating_add(work.width.saturating_sub(width).max(0));
    let max_y = work
        .y
        .saturating_add(work.height.saturating_sub(height).max(0));
    let left = anchor.x.saturating_sub(width).saturating_sub(GAP);
    let right = anchor.x.saturating_add(anchor.width).saturating_add(GAP);
    let x = if left >= work.x { left } else { right }.clamp(work.x, max_x);
    let centered_y = anchor
        .y
        .saturating_add(anchor.height / 2)
        .saturating_sub(height / 2);
    PhysicalRect::new(x, centered_y.clamp(work.y, max_y), width, height)
}

impl OwnedWindow {
    pub(super) fn create(
        class: &WindowClass,
        role: ShowcaseRole,
        work: PhysicalRect,
        backdrop_enabled: bool,
    ) -> Result<Self> {
        let (title_wide, title) = match role {
            ShowcaseRole::Topbar => (w!("Minha UI Topbar"), "Minha UI Topbar"),
            ShowcaseRole::Dock => (w!("Minha UI Dock"), "Minha UI Dock"),
            ShowcaseRole::Popover => (w!("Minha UI Popover"), "Minha UI Popover"),
            ShowcaseRole::AppMenu => (w!("Minha UI App Menu"), "Minha UI App Menu"),
            ShowcaseRole::Preview => (w!("Minha UI Preview"), "Minha UI Preview"),
            ShowcaseRole::Settings => (w!("Minha UI Settings"), "Minha UI Settings"),
        };
        let ex_style = window_ex_style(role);
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
        let backdrop_active = apply_if_supported(
            hwnd,
            role,
            backdrop_enabled && !matches!(role, ShowcaseRole::Popover | ShowcaseRole::AppMenu),
        );
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
            ShowcaseRole::AppMenu => register_app_menu_window(hwnd),
            ShowcaseRole::Preview => register_preview_window(hwnd),
            ShowcaseRole::Settings => register_settings_window(hwnd),
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
            ShowcaseRole::AppMenu => app_menu_initial_rect(work, dpi),
            ShowcaseRole::Preview => preview_initial_rect(work, dpi),
            ShowcaseRole::Settings => settings_rect(work, dpi),
        };
        set_rect(hwnd, rect)?;
        // SAFETY: Category 8 (FFI boundary). Every shell owner remains hidden
        // until its first complete DirectComposition frame is ready.
        let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
        Ok(Self {
            hwnd,
            role,
            title,
            rect,
            dpi,
            dock_width_dip: 760.0,
            dock_height_dip: ShellMetrics::default().dock_height_dip(),
            topbar_height_dip: 32.0,
            popover_width_dip: 244.0,
            popover_height_dip: 420.0,
            popover_anchor_x_dip: 122.0,
            backdrop_allowed: backdrop_enabled,
            backdrop_active,
        })
    }

    pub(super) fn reposition(&mut self, work: PhysicalRect) -> Result<()> {
        // SAFETY: Category 8 (FFI boundary). The owned HWND is live while its
        // current effective DPI is queried for work-area placement.
        self.dpi = Dpi::from_raw(unsafe { GetDpiForWindow(self.hwnd) }.max(96));
        let metrics = ShellMetrics::default()
            .with_topbar_height(self.topbar_height_dip)
            .with_dock_height(self.dock_height_dip);
        let rect = match self.role {
            ShowcaseRole::Topbar => topbar_rect(work, self.dpi, metrics),
            ShowcaseRole::Dock => {
                dock_showcase_rect(work, self.dpi, metrics.with_dock_width(self.dock_width_dip))
            }
            ShowcaseRole::Popover => {
                let placement = popover_placement(
                    topbar_rect(work, self.dpi, metrics),
                    work,
                    self.dpi,
                    PopoverSurfaceSize::new(self.popover_width_dip, self.popover_height_dip),
                );
                self.popover_anchor_x_dip = placement.anchor_x_dip();
                placement.rect()
            }
            ShowcaseRole::AppMenu => app_menu_initial_rect(work, self.dpi),
            ShowcaseRole::Preview => preview_initial_rect(work, self.dpi),
            ShowcaseRole::Settings => settings_rect(work, self.dpi),
        };
        self.rect = rect;
        set_rect(self.hwnd, self.rect)
    }

    pub(super) fn place_preview(&mut self, rect: PhysicalRect) -> Result<()> {
        if self.role != ShowcaseRole::Preview {
            return Ok(());
        }
        self.rect = rect;
        set_rect(self.hwnd, rect)
    }

    pub(super) fn place_settings(&mut self, work: PhysicalRect) -> Result<()> {
        if self.role != ShowcaseRole::Settings {
            return Ok(());
        }
        self.reposition(work)
    }

    pub(super) fn set_topbar_text_scale(
        &mut self,
        work: PhysicalRect,
        text_scale: f32,
    ) -> Result<()> {
        if self.role != ShowcaseRole::Topbar {
            return Ok(());
        }
        self.topbar_height_dip = topbar_height_for_text_scale(text_scale);
        self.reposition(work)
    }

    pub(super) fn place_popover(
        &mut self,
        work: PhysicalRect,
        anchor: PhysicalRect,
        size: PopoverSurfaceSize,
    ) -> Result<()> {
        if self.role != ShowcaseRole::Popover {
            return Ok(());
        }
        // SAFETY: Category 8 (FFI boundary). The owned HWND is live while its
        // current effective DPI is queried for popover placement.
        self.dpi = Dpi::from_raw(unsafe { GetDpiForWindow(self.hwnd) }.max(96));
        self.popover_width_dip = size.width().clamp(244.0, 440.0);
        let max_height_dip = work.height as f32 / self.dpi.scale();
        self.popover_height_dip = size.height().clamp(58.0, max_height_dip);
        let placement = popover_placement(
            anchor,
            work,
            self.dpi,
            PopoverSurfaceSize::new(self.popover_width_dip, self.popover_height_dip),
        );
        self.popover_anchor_x_dip = placement.anchor_x_dip();
        self.rect = placement.rect();
        set_rect(self.hwnd, self.rect)
    }

    pub(super) const fn popover_anchor_x_dip(&self) -> f32 {
        self.popover_anchor_x_dip
    }

    pub(super) fn place_context_menu(
        &mut self,
        work: PhysicalRect,
        anchor: PhysicalRect,
        height_dip: f32,
    ) -> Result<()> {
        if self.role != ShowcaseRole::Popover {
            return Ok(());
        }
        // SAFETY: Category 8 (FFI boundary). The owned HWND is live while its
        // effective DPI is queried; the result only drives pure placement math.
        self.dpi = Dpi::from_raw(unsafe { GetDpiForWindow(self.hwnd) }.max(96));
        self.popover_height_dip = height_dip.max(1.0);
        self.rect = context_menu_anchor_rect(anchor, work, self.dpi, self.popover_height_dip);
        set_rect(self.hwnd, self.rect)
    }

    #[allow(
        dead_code,
        reason = "Task 10B invokes placement when the native background-app coordinator opens the menu"
    )]
    pub(super) fn place_app_menu(
        &mut self,
        work: PhysicalRect,
        anchor: PhysicalRect,
        height_dip: f32,
    ) -> Result<()> {
        if self.role != ShowcaseRole::AppMenu {
            return Ok(());
        }
        // SAFETY: Category 8 (FFI boundary). The owned HWND is live while its
        // effective DPI is queried; only pure work-area placement consumes it.
        self.dpi = Dpi::from_raw(unsafe { GetDpiForWindow(self.hwnd) }.max(96));
        self.popover_height_dip = height_dip.max(1.0);
        self.rect =
            crate::background_app_menu_rect(anchor, work, self.dpi, self.popover_height_dip);
        set_rect(self.hwnd, self.rect)
    }

    pub(super) fn hide(&self) {
        // SAFETY: Category 8 (FFI boundary). The owned HWND remains valid for this
        // idempotent visibility update.
        let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    pub(super) fn show(&self) {
        // SAFETY: Category 8 (FFI boundary). The live owned HWND is revealed
        // without activation after its first composition frame is complete.
        let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
    }

    pub(super) fn show_activating(&self) {
        // SAFETY: Category 8 (FFI boundary). The owned panel HWND is live and
        // activation lets Escape and focus-loss dismissal reach its window proc.
        let _ = unsafe { ShowWindow(self.hwnd, SW_SHOW) };
        // SAFETY: Category 8 (FFI boundary). This popup is shown directly in
        // response to user input and remains live while Windows transfers focus.
        let _ = unsafe { SetForegroundWindow(self.hwnd) };
        // SAFETY: Category 8 (FFI boundary). The popup belongs to this UI thread;
        // assigning keyboard focus makes deactivation observable and reversible.
        let _ = unsafe { SetFocus(Some(self.hwnd)) };
    }

    pub(super) fn restore_focus(&self) {
        // SAFETY: Category 8 (FFI boundary). The top-bar HWND is owned by this UI
        // thread and remains live while a child popup is dismissed.
        let _ = unsafe { SetForegroundWindow(self.hwnd) };
        // SAFETY: Category 8 (FFI boundary). Focus returns to the invoker after the
        // popup releases activation.
        let _ = unsafe { SetFocus(Some(self.hwnd)) };
    }

    pub(super) fn set_backdrop_enabled(&mut self, enabled: bool) {
        let requested = self.backdrop_allowed && enabled;
        self.backdrop_active = apply_if_supported(self.hwnd, self.role, requested);
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
        let target = self.dock_visibility_target(work, config, hidden)?;
        self.place_dock_motion(target)
    }

    pub(super) fn dock_visibility_target(
        &mut self,
        work: PhysicalRect,
        config: DockRuntimeConfig,
        hidden: bool,
    ) -> Result<PhysicalRect> {
        // SAFETY: Category 8 (FFI boundary). The owned HWND is live while its
        // current effective DPI is queried for work-area placement.
        self.dpi = Dpi::from_raw(unsafe { GetDpiForWindow(self.hwnd) }.max(96));
        let normal = dock_showcase_rect(
            work,
            self.dpi,
            ShellMetrics::default()
                .with_dock_width(self.dock_width_dip)
                .with_dock_height(self.dock_height_dip),
        );
        let monitor = crate::win32_windowing::window_monitor_bounds(self.hwnd)?;
        Ok(DockPhysicalPlacement::from_visibility_at_monitor_edge(
            DockEdgeGeometry::new(normal, monitor, self.dpi.scale()),
            config,
            hidden,
        )
        .rect())
    }

    pub(super) fn place_dock_motion(&mut self, rect: PhysicalRect) -> Result<()> {
        if self.role != ShowcaseRole::Dock {
            return Ok(());
        }
        if self.rect == rect {
            return Ok(());
        }
        self.rect = rect;
        set_rect(self.hwnd, rect)
    }

    pub(super) fn set_dock_size(
        &mut self,
        work: PhysicalRect,
        width_dip: f32,
        height_dip: f32,
        config: DockRuntimeConfig,
        hidden: bool,
    ) -> Result<()> {
        if self.role != ShowcaseRole::Dock {
            return Ok(());
        }
        self.dock_width_dip = width_dip.max(1.0);
        self.dock_height_dip = height_dip.max(1.0);
        let target = self.dock_visibility_target(work, config, hidden)?;
        self.place_dock_motion(target)
    }

    pub(super) fn set_dock_width_preserving_y(
        &mut self,
        work: PhysicalRect,
        width_dip: f32,
    ) -> Result<()> {
        if self.role != ShowcaseRole::Dock {
            return Ok(());
        }
        let current = self.rect;
        self.dock_width_dip = width_dip.max(1.0);
        let mut normal = self.dock_visibility_target(work, DockRuntimeConfig::default(), false)?;
        normal.y = current.y;
        normal.height = current.height;
        self.place_dock_motion(normal)
    }

    pub(super) const fn dock_width_dip(&self) -> f32 {
        self.dock_width_dip
    }

    pub(super) fn surface_physical_size(&self) -> (u32, u32) {
        if self.role == ShowcaseRole::Dock {
            let width = physical_from_dip(self.dock_width_dip, self.dpi);
            let height = physical_from_dip(self.dock_height_dip, self.dpi);
            return (width.max(1) as u32, height.max(1) as u32);
        }
        (
            self.rect.width.max(1) as u32,
            self.rect.height.max(1) as u32,
        )
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
        // SAFETY: Category 8 (FFI boundary). The owned HWND remains live here.
        self.dpi = Dpi::from_raw(unsafe { GetDpiForWindow(self.hwnd) }.max(96));
        Ok(())
    }

    pub(super) const fn dpi(&self) -> Dpi {
        self.dpi
    }
}

fn window_ex_style(role: ShowcaseRole) -> WINDOW_EX_STYLE {
    match role {
        ShowcaseRole::Topbar => {
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP
        }
        ShowcaseRole::Dock => WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        ShowcaseRole::Popover | ShowcaseRole::AppMenu => {
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP
        }
        ShowcaseRole::Preview | ShowcaseRole::Settings => WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
    }
}

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        match self.role {
            ShowcaseRole::Topbar => unregister_topbar_window(self.hwnd),
            ShowcaseRole::Dock => unregister_dock_window(self.hwnd),
            ShowcaseRole::Popover => unregister_popover_window(self.hwnd),
            ShowcaseRole::AppMenu => unregister_app_menu_window(self.hwnd),
            ShowcaseRole::Preview => unregister_preview_window(self.hwnd),
            ShowcaseRole::Settings => unregister_settings_window(self.hwnd),
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
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
}

const fn role_name(role: ShowcaseRole) -> &'static str {
    match role {
        ShowcaseRole::Topbar => "topbar",
        ShowcaseRole::Dock => "dock",
        ShowcaseRole::Popover => "popover",
        ShowcaseRole::AppMenu => "app-menu",
        ShowcaseRole::Preview => "preview",
        ShowcaseRole::Settings => "settings",
    }
}

fn app_menu_initial_rect(work: PhysicalRect, dpi: Dpi) -> PhysicalRect {
    crate::background_app_menu_rect(PhysicalRect::new(work.x, work.y, 0, 0), work, dpi, 1.0)
}

fn preview_initial_rect(work: PhysicalRect, dpi: Dpi) -> PhysicalRect {
    let width = shell_renderer::physical_from_dip(344.0, dpi).min(work.width - 32);
    let height = shell_renderer::physical_from_dip(252.0, dpi).min(work.height - 32);
    PhysicalRect::new(
        work.x + (work.width - width) / 2,
        work.y + work.height - height - shell_renderer::physical_from_dip(80.0, dpi),
        width.max(1),
        height.max(1),
    )
}

fn settings_rect(work: PhysicalRect, dpi: Dpi) -> PhysicalRect {
    let scale = dpi.scale();
    let width = shell_renderer::physical_from_dip(992.0, dpi).min(work.width - 32);
    let height = shell_renderer::physical_from_dip(620.0, dpi).min(work.height - 32);
    PhysicalRect::new(
        work.x + (work.width - width) / 2,
        work.y + (work.height - height) / 2,
        width.max((480.0 * scale).round() as i32),
        height.max((360.0 * scale).round() as i32),
    )
}

#[cfg(test)]
mod tests {
    use shell_renderer::{PhysicalRect, native::ShowcaseRole};
    use windows::Win32::UI::WindowsAndMessaging::WS_EX_NOREDIRECTIONBITMAP;

    use super::{external_menu_placement, external_menu_popover_is_topmost, window_ex_style};

    #[test]
    fn custom_alpha_windows_use_no_redirection_bitmap() {
        for role in [
            ShowcaseRole::Topbar,
            ShowcaseRole::Popover,
            ShowcaseRole::AppMenu,
        ] {
            let style = window_ex_style(role);
            assert_ne!(style.0 & WS_EX_NOREDIRECTIONBITMAP.0, 0);
        }
    }

    #[test]
    fn external_native_menu_temporarily_sits_above_the_apps_popover() {
        assert!(!external_menu_popover_is_topmost(true));
        assert!(external_menu_popover_is_topmost(false));
    }

    #[test]
    fn external_native_menu_is_placed_beside_the_clicked_popover_row() {
        let work = PhysicalRect::new(0, 56, 2_560, 1_376);
        let popover_edge_at_row = PhysicalRect::new(2_240, 220, 1, 1);

        assert_eq!(
            external_menu_placement(241, 134, popover_edge_at_row, work),
            PhysicalRect::new(1_991, 153, 241, 134),
        );
    }
}
