use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, ERROR, GetMonitorInfoW, GetWindowRgn, HGDIOBJ, HRGN,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, SetWindowRgn,
};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABM_GETAUTOHIDEBAREX, ABM_GETSTATE, ABM_NEW, ABM_REMOVE, ABM_SETAUTOHIDEBAREX,
    ABM_SETSTATE, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, IsWindow, IsWindowVisible, SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow,
    WM_APP,
};
use windows::core::{BOOL, Error, Result};

const EDGE_APPBAR_CALLBACK: u32 = WM_APP + 0x311;
const WORK_AREA_SETTLE_BUDGET: Duration = Duration::from_millis(100);
const WORK_AREA_SETTLE_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScreenRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl ScreenRect {
    const fn from_win32(rect: RECT) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }

    const fn to_win32(self) -> RECT {
        RECT {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TrackedTaskbar {
    handle: isize,
    restore_on_drop: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DiscoveredTaskbar {
    handle: isize,
    is_visible: bool,
    is_primary: bool,
    monitor: ScreenRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DockWindow {
    handle: isize,
    monitor: ScreenRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TaskbarEdgeClaim {
    dock_handle: isize,
    monitor: ScreenRect,
}

struct TaskbarRegionGuard {
    handle: isize,
    original_region: Option<HRGN>,
    had_original_region: bool,
    suppressed: bool,
}

fn reconcile_taskbars(
    tracked: &[TrackedTaskbar],
    discovered: &[DiscoveredTaskbar],
) -> Vec<TrackedTaskbar> {
    discovered
        .iter()
        .map(|window| TrackedTaskbar {
            handle: window.handle,
            restore_on_drop: tracked
                .iter()
                .find(|entry| entry.handle == window.handle)
                .map_or(window.is_visible, |entry| entry.restore_on_drop),
        })
        .collect()
}

fn appbar_owner_handle(discovered: &[DiscoveredTaskbar]) -> Option<isize> {
    discovered
        .iter()
        .find(|taskbar| taskbar.is_primary)
        .map(|taskbar| taskbar.handle)
}

const fn active_taskbar_state(original: u32) -> u32 {
    original | ABS_AUTOHIDE
}

const fn bottom_work_area_is_released(bounds: ScreenRect, work_area: ScreenRect) -> bool {
    work_area.bottom == bounds.bottom
}

pub(super) struct ExplorerTaskbarVisibilityGuard {
    taskbars: Vec<TrackedTaskbar>,
    edge_claims: Vec<TaskbarEdgeClaim>,
    taskbar_regions: Vec<TaskbarRegionGuard>,
    appbar_owner: Option<isize>,
    original_appbar_state: u32,
}

impl ExplorerTaskbarVisibilityGuard {
    pub(super) fn prepare_work_area() -> Result<Self> {
        let discovered = discover_explorer_taskbars()?;
        let taskbar_regions = discovered
            .iter()
            .map(|taskbar| TaskbarRegionGuard::capture(taskbar.handle))
            .collect::<Result<Vec<_>>>()?;
        let appbar_owner = appbar_owner_handle(&discovered);
        let original_appbar_state = explorer_taskbar_state();
        set_explorer_taskbar_state(active_taskbar_state(original_appbar_state), appbar_owner);
        wait_for_released_bottom_work_areas();
        Ok(Self {
            taskbars: reconcile_taskbars(&[], &discovered),
            edge_claims: Vec::new(),
            taskbar_regions,
            appbar_owner,
            original_appbar_state,
        })
    }

    pub(super) fn reconcile_and_hide(&mut self, dock_handles: &[HWND]) -> Result<()> {
        let discovered = discover_explorer_taskbars()?;
        self.appbar_owner = appbar_owner_handle(&discovered);
        set_explorer_taskbar_state(
            active_taskbar_state(self.original_appbar_state),
            self.appbar_owner,
        );
        self.taskbars = reconcile_taskbars(&self.taskbars, &discovered);
        let docks = discover_dock_windows(dock_handles)?;
        self.reconcile_edge_claims(&docks, &discovered)?;
        for taskbar in &discovered {
            if taskbar.is_visible {
                // SAFETY: Category 8 (FFI boundary). The HWND was returned by
                // synchronous top-level window enumeration. The dock owns the
                // autohide edge before Explorer is hidden.
                let _ = unsafe { ShowWindow(HWND(taskbar.handle as *mut _), SW_HIDE) };
            }
        }
        self.reconcile_taskbar_regions(&discovered)?;
        Ok(())
    }

    fn reconcile_edge_claims(
        &mut self,
        docks: &[DockWindow],
        taskbars: &[DiscoveredTaskbar],
    ) -> Result<()> {
        let mut index = 0;
        while index < self.edge_claims.len() {
            let claim = self.edge_claims[index];
            let remains_current = docks
                .iter()
                .any(|dock| dock.handle == claim.dock_handle && dock.monitor == claim.monitor);
            if remains_current {
                index += 1;
            } else {
                let stale = self.edge_claims.remove(index);
                release_edge_claim(stale, taskbars);
            }
        }

        for dock in docks {
            let already_claimed = self
                .edge_claims
                .iter()
                .any(|claim| claim.dock_handle == dock.handle && claim.monitor == dock.monitor);
            if !already_claimed {
                self.edge_claims.push(claim_taskbar_edge(*dock, taskbars)?);
            }
        }
        Ok(())
    }

    fn reconcile_taskbar_regions(&mut self, discovered: &[DiscoveredTaskbar]) -> Result<()> {
        self.taskbar_regions.retain(|tracked| {
            discovered
                .iter()
                .any(|taskbar| taskbar.handle == tracked.handle)
        });
        for taskbar in discovered {
            if !self
                .taskbar_regions
                .iter()
                .any(|tracked| tracked.handle == taskbar.handle)
            {
                self.taskbar_regions
                    .push(TaskbarRegionGuard::capture(taskbar.handle)?);
            }
        }
        for region in &mut self.taskbar_regions {
            region.suppress()?;
        }
        Ok(())
    }
}

impl Drop for ExplorerTaskbarVisibilityGuard {
    fn drop(&mut self) {
        let current_taskbars = discover_explorer_taskbars().unwrap_or_default();
        for claim in std::mem::take(&mut self.edge_claims) {
            release_edge_claim(claim, &current_taskbars);
        }
        let current_owner = appbar_owner_handle(&current_taskbars).or(self.appbar_owner);
        set_explorer_taskbar_state(self.original_appbar_state, current_owner);
        for region in &mut self.taskbar_regions {
            region.restore();
        }
        for taskbar in &self.taskbars {
            let hwnd = HWND(taskbar.handle as *mut _);
            // SAFETY: Category 8 (FFI boundary). Stale HWNDs can occur when
            // Explorer restarts, so validity is checked before restoration.
            let valid = unsafe { IsWindow(Some(hwnd)) }.as_bool();
            if taskbar.restore_on_drop && valid {
                // SAFETY: Category 8 (FFI boundary). Restore without activation
                // so shutdown cannot steal focus from the foreground app.
                let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
            }
        }
    }
}

impl TaskbarRegionGuard {
    fn capture(handle: isize) -> Result<Self> {
        // SAFETY: Category 8 (FFI boundary). The zero-area region is writable
        // storage for the copied window region and remains caller-owned.
        let original_region = unsafe { CreateRectRgn(0, 0, 0, 0) };
        if original_region.0.is_null() {
            return Err(Error::from_thread());
        }
        let hwnd = HWND(handle as *mut _);
        // SAFETY: Category 8 (FFI boundary). The taskbar HWND was synchronously
        // discovered and the destination region is live for this call.
        let region_type = unsafe { GetWindowRgn(hwnd, original_region) };
        let had_original_region = region_type.0 != ERROR;
        let original_region = if had_original_region {
            Some(original_region)
        } else {
            // SAFETY: Category 8 (FFI boundary). The temporary region was not
            // transferred and must be released by its creator.
            let _ = unsafe { DeleteObject(HGDIOBJ(original_region.0)) };
            None
        };
        Ok(Self {
            handle,
            original_region,
            had_original_region,
            suppressed: false,
        })
    }

    fn suppress(&mut self) -> Result<()> {
        if self.suppressed {
            return Ok(());
        }
        // SAFETY: Category 8 (FFI boundary). A zero-area region prevents both
        // composition and pointer hit-testing of the native taskbar HWND.
        let empty_region = unsafe { CreateRectRgn(0, 0, 0, 0) };
        if empty_region.0.is_null() {
            return Err(Error::from_thread());
        }
        let hwnd = HWND(self.handle as *mut _);
        // SAFETY: Category 8 (FFI boundary). Windows owns the region on
        // success; on failure this function retains and releases ownership.
        if unsafe { SetWindowRgn(hwnd, Some(empty_region), true) } == 0 {
            // SAFETY: Category 8 (FFI boundary). SetWindowRgn did not take it.
            let _ = unsafe { DeleteObject(HGDIOBJ(empty_region.0)) };
            return Err(Error::from_thread());
        }
        self.suppressed = true;
        Ok(())
    }

    fn restore(&mut self) {
        let hwnd = HWND(self.handle as *mut _);
        // SAFETY: Category 8 (FFI boundary). Explorer may have restarted, so a
        // stale HWND is ignored and the copied region remains locally owned.
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return;
        }
        if self.had_original_region {
            if let Some(original_region) = self.original_region {
                // SAFETY: Category 8 (FFI boundary). Success transfers the
                // saved region back to Windows; failure retains local ownership.
                if unsafe { SetWindowRgn(hwnd, Some(original_region), true) } != 0 {
                    self.original_region = None;
                }
            }
        } else {
            // SAFETY: Category 8 (FFI boundary). None restores the default
            // rectangular window region present before app startup.
            let _ = unsafe { SetWindowRgn(hwnd, None, true) };
        }
        self.suppressed = false;
    }
}

impl Drop for TaskbarRegionGuard {
    fn drop(&mut self) {
        if let Some(original_region) = self.original_region.take() {
            // SAFETY: Category 8 (FFI boundary). This copied GDI region was not
            // transferred back to Windows and is still owned by the guard.
            let _ = unsafe { DeleteObject(HGDIOBJ(original_region.0)) };
        }
    }
}

fn claim_taskbar_edge(
    dock: DockWindow,
    taskbars: &[DiscoveredTaskbar],
) -> Result<TaskbarEdgeClaim> {
    let previous_owner = auto_hide_owner(dock.monitor);
    if previous_owner == Some(dock.handle) {
        return Ok(TaskbarEdgeClaim {
            dock_handle: dock.handle,
            monitor: dock.monitor,
        });
    }
    if let Some(owner) = previous_owner {
        let _ = set_auto_hide_owner(owner, dock.monitor, false);
    }
    if !register_appbar(dock.handle) {
        restore_previous_auto_hide_owner(previous_owner, dock.monitor);
        return Err(Error::new(
            E_FAIL,
            "failed to register the dock as an appbar edge owner",
        ));
    }
    if !set_auto_hide_owner(dock.handle, dock.monitor, true) {
        remove_appbar(dock.handle);
        restore_previous_auto_hide_owner(previous_owner, dock.monitor);
        return Err(Error::new(
            E_FAIL,
            "failed to claim the taskbar autohide edge for the dock",
        ));
    }

    if let Some(taskbar) = taskbars
        .iter()
        .find(|taskbar| taskbar.monitor == dock.monitor)
    {
        // SAFETY: Category 8 (FFI boundary). The dock has already become the
        // registered edge owner, so hiding Explorer cannot expose an edge gap.
        let _ = unsafe { ShowWindow(HWND(taskbar.handle as *mut _), SW_HIDE) };
    }
    Ok(TaskbarEdgeClaim {
        dock_handle: dock.handle,
        monitor: dock.monitor,
    })
}

fn release_edge_claim(claim: TaskbarEdgeClaim, taskbars: &[DiscoveredTaskbar]) {
    let _ = set_auto_hide_owner(claim.dock_handle, claim.monitor, false);
    remove_appbar(claim.dock_handle);
    if let Some(taskbar) = taskbars
        .iter()
        .find(|taskbar| taskbar.monitor == claim.monitor)
    {
        let _ = set_auto_hide_owner(taskbar.handle, claim.monitor, true);
    }
}

fn restore_previous_auto_hide_owner(owner: Option<isize>, monitor: ScreenRect) {
    if let Some(owner) = owner {
        let _ = set_auto_hide_owner(owner, monitor, true);
    }
}

fn register_appbar(handle: isize) -> bool {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: HWND(handle as *mut _),
        uCallbackMessage: EDGE_APPBAR_CALLBACK,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). The dock HWND is live and the
    // APPBARDATA buffer remains valid for the synchronous registration.
    unsafe { SHAppBarMessage(ABM_NEW, &mut data) != 0 }
}

fn remove_appbar(handle: isize) {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: HWND(handle as *mut _),
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). Removing an already-destroyed or
    // unregistered appbar is a bounded no-op reported by the Shell.
    unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) };
}

fn auto_hide_owner(monitor: ScreenRect) -> Option<isize> {
    let mut data = edge_appbar_data(0, monitor, false);
    // SAFETY: Category 8 (FFI boundary). The monitor rectangle and edge fully
    // identify the synchronous autohide-owner query.
    let owner = unsafe { SHAppBarMessage(ABM_GETAUTOHIDEBAREX, &mut data) };
    (owner != 0).then_some(owner as isize)
}

fn set_auto_hide_owner(handle: isize, monitor: ScreenRect, registered: bool) -> bool {
    let mut data = edge_appbar_data(handle, monitor, registered);
    // SAFETY: Category 8 (FFI boundary). APPBARDATA contains the documented
    // HWND, edge, monitor rectangle, and scalar registration flag.
    unsafe { SHAppBarMessage(ABM_SETAUTOHIDEBAREX, &mut data) != 0 }
}

fn edge_appbar_data(handle: isize, monitor: ScreenRect, registered: bool) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: HWND(handle as *mut _),
        uEdge: ABE_BOTTOM,
        rc: monitor.to_win32(),
        lParam: LPARAM(isize::from(registered)),
        ..Default::default()
    }
}

fn wait_for_released_bottom_work_areas() {
    let deadline = Instant::now() + WORK_AREA_SETTLE_BUDGET;
    loop {
        let released = crate::win32_work_area::monitor_placement_inputs().is_ok_and(|monitors| {
            !monitors.is_empty()
                && monitors.iter().all(|monitor| {
                    let bounds = monitor.bounds();
                    let work = monitor.work_area();
                    bottom_work_area_is_released(
                        ScreenRect {
                            left: bounds.x,
                            top: bounds.y,
                            right: bounds.x.saturating_add(bounds.width),
                            bottom: bounds.y.saturating_add(bounds.height),
                        },
                        ScreenRect {
                            left: work.x,
                            top: work.y,
                            right: work.x.saturating_add(work.width),
                            bottom: work.y.saturating_add(work.height),
                        },
                    )
                })
        });
        if released || Instant::now() >= deadline {
            return;
        }
        thread::sleep(WORK_AREA_SETTLE_INTERVAL);
    }
}

fn explorer_taskbar_state() -> u32 {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). ABM_GETSTATE reads no pointer payload
    // beyond the correctly sized APPBARDATA owned by this call.
    unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) as u32 }
}

fn set_explorer_taskbar_state(state: u32, appbar_owner: Option<isize>) {
    let Some(appbar_owner) = appbar_owner else {
        return;
    };
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: HWND(appbar_owner as *mut _),
        lParam: LPARAM(state as isize),
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). ABM_SETSTATE consumes the scalar state
    // and the documented live taskbar HWND in the sized APPBARDATA.
    unsafe { SHAppBarMessage(ABM_SETSTATE, &mut data) };
}

fn discover_dock_windows(handles: &[HWND]) -> Result<Vec<DockWindow>> {
    handles
        .iter()
        .map(|handle| {
            Ok(DockWindow {
                handle: handle.0 as isize,
                monitor: monitor_rect_for_window(*handle)?,
            })
        })
        .collect()
}

fn monitor_rect_for_window(hwnd: HWND) -> Result<ScreenRect> {
    // SAFETY: Category 8 (FFI boundary). The nearest-monitor fallback returns a
    // monitor for live dock and taskbar windows during topology changes.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). The monitor-info buffer has the
    // documented size and remains writable for this synchronous query.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(Error::from_thread());
    }
    Ok(ScreenRect::from_win32(info.rcMonitor))
}

fn discover_explorer_taskbars() -> Result<Vec<DiscoveredTaskbar>> {
    let mut taskbars = Vec::new();
    // SAFETY: Category 8 (FFI boundary). `taskbars` remains alive throughout
    // synchronous enumeration and the callback only borrows it via LPARAM.
    unsafe {
        EnumWindows(
            Some(enum_explorer_taskbar),
            LPARAM(&mut taskbars as *mut _ as isize),
        )
    }?;
    order_taskbars_for_hide(&mut taskbars);
    Ok(taskbars)
}

fn order_taskbars_for_hide(taskbars: &mut [DiscoveredTaskbar]) {
    taskbars.sort_by_key(|taskbar| !taskbar.is_primary);
}

unsafe extern "system" fn enum_explorer_taskbar(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: Category 8 (FFI boundary). The LPARAM originates from the live
    // Vec in `discover_explorer_taskbars`, and EnumWindows is synchronous.
    let taskbars = unsafe { &mut *(lparam.0 as *mut Vec<DiscoveredTaskbar>) };
    let mut class_name = [0_u16; 64];
    // SAFETY: Category 8 (FFI boundary). The callback supplies a live top-level
    // HWND and the fixed buffer is writable for the duration of the call.
    let copied = unsafe { GetClassNameW(hwnd, &mut class_name) };
    let class_name = String::from_utf16_lossy(&class_name[..copied.max(0) as usize]);
    if matches!(
        class_name.as_str(),
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    ) && let Ok(monitor) = monitor_rect_for_window(hwnd)
    {
        // SAFETY: Category 8 (FFI boundary). Visibility is queried synchronously
        // for the HWND supplied by EnumWindows.
        let is_visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
        taskbars.push(DiscoveredTaskbar {
            handle: hwnd.0 as isize,
            is_visible,
            is_primary: class_name == "Shell_TrayWnd",
            monitor,
        });
    }
    true.into()
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoveredTaskbar, ScreenRect, TrackedTaskbar, active_taskbar_state, appbar_owner_handle,
        bottom_work_area_is_released, order_taskbars_for_hide, reconcile_taskbars,
    };

    const PRIMARY_MONITOR: ScreenRect = ScreenRect {
        left: 0,
        top: 0,
        right: 2_560,
        bottom: 1_440,
    };
    const SECONDARY_MONITOR: ScreenRect = ScreenRect {
        left: -1_920,
        top: 0,
        right: 0,
        bottom: 1_080,
    };

    const fn taskbar(handle: isize, is_visible: bool, is_primary: bool) -> DiscoveredTaskbar {
        DiscoveredTaskbar {
            handle,
            is_visible,
            is_primary,
            monitor: if is_primary {
                PRIMARY_MONITOR
            } else {
                SECONDARY_MONITOR
            },
        }
    }

    #[test]
    fn newly_visible_taskbars_are_owned_for_restoration() {
        let reconciled =
            reconcile_taskbars(&[], &[taskbar(11, true, true), taskbar(22, false, false)]);

        assert_eq!(
            reconciled,
            vec![
                TrackedTaskbar {
                    handle: 11,
                    restore_on_drop: true,
                },
                TrackedTaskbar {
                    handle: 22,
                    restore_on_drop: false,
                },
            ]
        );
    }

    #[test]
    fn rediscovery_preserves_restore_ownership_after_the_window_is_hidden() {
        let reconciled = reconcile_taskbars(
            &[TrackedTaskbar {
                handle: 11,
                restore_on_drop: true,
            }],
            &[taskbar(11, false, true)],
        );

        assert_eq!(
            reconciled,
            vec![TrackedTaskbar {
                handle: 11,
                restore_on_drop: true,
            }]
        );
    }

    #[test]
    fn explorer_restart_replaces_stale_windows_without_losing_restore_ownership() {
        let reconciled = reconcile_taskbars(
            &[TrackedTaskbar {
                handle: 11,
                restore_on_drop: true,
            }],
            &[taskbar(33, true, true)],
        );

        assert_eq!(
            reconciled,
            vec![TrackedTaskbar {
                handle: 33,
                restore_on_drop: true,
            }]
        );
    }

    #[test]
    fn primary_taskbar_is_hidden_before_secondary_taskbars() {
        let mut discovered = vec![taskbar(22, true, false), taskbar(11, true, true)];

        order_taskbars_for_hide(&mut discovered);

        assert_eq!(
            discovered
                .iter()
                .map(|taskbar| taskbar.handle)
                .collect::<Vec<_>>(),
            vec![11, 22]
        );
    }

    #[test]
    fn active_state_adds_autohide_without_discarding_existing_flags() {
        assert_eq!(active_taskbar_state(0), 1);
        assert_eq!(active_taskbar_state(2), 3);
    }

    #[test]
    fn appbar_state_is_applied_through_the_primary_taskbar_window() {
        let discovered = [taskbar(22, true, false), taskbar(11, true, true)];

        assert_eq!(appbar_owner_handle(&discovered), Some(11));
        assert_eq!(appbar_owner_handle(&discovered[..1]), None);
    }

    #[test]
    fn startup_waits_only_until_the_taskbar_releases_the_bottom_edge() {
        let bounds = PRIMARY_MONITOR;
        let reserved = ScreenRect {
            bottom: 1_392,
            ..bounds
        };
        let topbar_only = ScreenRect { top: 32, ..bounds };

        assert!(!bottom_work_area_is_released(bounds, reserved));
        assert!(bottom_work_area_is_released(bounds, topbar_only));
    }
}
