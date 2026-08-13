use std::thread;
use std::time::{Duration, Instant};

use shell_core::{
    TaskbarFingerprintBounds, TaskbarFingerprintSnapshot, TaskbarStateFingerprint,
    TaskbarWindowClass, taskbar_arming_fingerprint_v1,
};
use windows::Win32::Foundation::{E_ACCESSDENIED, E_FAIL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, ERROR, GetMonitorInfoW, GetWindowRgn, HGDIOBJ,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromWindow, SetWindowRgn,
};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABM_GETAUTOHIDEBAREX, ABM_GETSTATE, ABM_NEW, ABM_REMOVE, ABM_SETAUTOHIDEBAREX,
    ABM_SETSTATE, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
    SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow, WM_APP,
};
use windows::core::{BOOL, Error, Result};

use crate::win32_watchdog_arming::TaskbarMutationLease;

const EDGE_APPBAR_CALLBACK: u32 = WM_APP + 0x311;
const WORK_AREA_SETTLE_BUDGET: Duration = Duration::from_secs(2);
const WORK_AREA_SETTLE_INTERVAL: Duration = Duration::from_millis(10);

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

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiscoveredTaskbar {
    handle: isize,
    process_id: u32,
    is_visible: bool,
    is_primary: bool,
    monitor: ScreenRect,
    device: String,
    window_bounds: ScreenRect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AuthorizedTaskbarIdentity {
    stable: StableTaskbarIdentity,
    window_bounds: ScreenRect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StableTaskbarIdentity {
    handle: isize,
    process_id: u32,
    is_primary: bool,
    monitor: ScreenRect,
    device: String,
}

impl From<&DiscoveredTaskbar> for AuthorizedTaskbarIdentity {
    fn from(taskbar: &DiscoveredTaskbar) -> Self {
        Self {
            stable: StableTaskbarIdentity::from(taskbar),
            window_bounds: taskbar.window_bounds,
        }
    }
}

impl From<&DiscoveredTaskbar> for StableTaskbarIdentity {
    fn from(taskbar: &DiscoveredTaskbar) -> Self {
        Self {
            handle: taskbar.handle,
            process_id: taskbar.process_id,
            is_primary: taskbar.is_primary,
            monitor: taskbar.monitor,
            device: taskbar.device.clone(),
        }
    }
}

struct TaskbarArmingObservation {
    fingerprint: TaskbarStateFingerprint,
    appbar_state: u32,
    taskbars: Vec<DiscoveredTaskbar>,
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
    previous_owner: Option<isize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EdgeOwnerAuthorization {
    Unowned,
    DockAlreadyOwns,
    Explorer(isize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkAreaWaitDecision {
    Ready,
    Retry,
    TimedOut,
}

struct TaskbarRegionGuard {
    handle: isize,
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
    authorized_taskbars: Vec<AuthorizedTaskbarIdentity>,
    taskbars: Vec<TrackedTaskbar>,
    edge_claims: Vec<TaskbarEdgeClaim>,
    taskbar_regions: Vec<TaskbarRegionGuard>,
    appbar_owner: Option<isize>,
    original_appbar_state: u32,
}

impl ExplorerTaskbarVisibilityGuard {
    pub(super) fn prepare_work_area(lease: &TaskbarMutationLease) -> Result<Self> {
        ensure_valid_lease(lease)?;
        // This post-LEASE observation owns the exact HWND set consumed below.
        // Discovery is not repeated between the fingerprint comparison and
        // capture, so an Explorer restart or topology change cannot substitute
        // an unverified second set of taskbars for mutation.
        let observation = observe_taskbar_arming_state()?;
        if observation.fingerprint != lease.fingerprint() {
            return Err(Error::new(
                E_ACCESSDENIED,
                "taskbar state changed after the watchdog granted its lease",
            ));
        }
        let discovered_before_appbar = observation.taskbars;
        let taskbar_regions = discovered_before_appbar
            .iter()
            .map(|taskbar| TaskbarRegionGuard::capture_default(taskbar.handle))
            .collect::<Result<Vec<_>>>()?;
        let appbar_owner = appbar_owner_handle(&discovered_before_appbar);
        let original_appbar_state = observation.appbar_state;
        if let Err(error) =
            set_explorer_taskbar_state(active_taskbar_state(original_appbar_state), appbar_owner)
                .and_then(|()| wait_for_released_bottom_work_areas())
        {
            let _ = set_explorer_taskbar_state(original_appbar_state, appbar_owner);
            return Err(error);
        }
        let discovered_after_appbar = match discover_explorer_taskbars() {
            Ok(discovered) => discovered,
            Err(error) => {
                let _ = set_explorer_taskbar_state(original_appbar_state, appbar_owner);
                return Err(error);
            }
        };
        let Some(authorized_taskbars) =
            authorize_post_appbar_taskbars(&discovered_before_appbar, &discovered_after_appbar)
        else {
            let _ = set_explorer_taskbar_state(original_appbar_state, appbar_owner);
            return Err(Error::new(
                E_ACCESSDENIED,
                "taskbar identity or monitor topology changed while enabling autohide",
            ));
        };
        Ok(Self {
            authorized_taskbars,
            taskbars: reconcile_taskbars(&[], &discovered_before_appbar),
            edge_claims: Vec::new(),
            taskbar_regions,
            appbar_owner,
            original_appbar_state,
        })
    }

    pub(super) fn reconcile_and_hide(
        &mut self,
        lease: &TaskbarMutationLease,
        dock_handles: &[HWND],
    ) -> Result<()> {
        ensure_valid_lease(lease)?;
        let discovered = discover_explorer_taskbars()?;
        if !same_authorized_taskbar_set(&self.authorized_taskbars, &discovered) {
            return Err(Error::new(
                E_ACCESSDENIED,
                "taskbar identity or topology changed after the watchdog lease",
            ));
        }
        self.appbar_owner = appbar_owner_handle(&discovered);
        set_explorer_taskbar_state(
            active_taskbar_state(self.original_appbar_state),
            self.appbar_owner,
        )?;
        self.preflight_taskbar_regions(&discovered)?;
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
        self.preflight_taskbar_regions(discovered)?;
        run_two_phase_mutation(
            &mut self.taskbar_regions,
            |region| {
                if region.suppressed {
                    Ok(())
                } else {
                    ensure_default_window_region(region.handle)
                }
            },
            TaskbarRegionGuard::suppress,
        )
    }

    fn preflight_taskbar_regions(&self, discovered: &[DiscoveredTaskbar]) -> Result<()> {
        if self.taskbar_regions.len() != discovered.len()
            || self.taskbar_regions.iter().any(|tracked| {
                !discovered
                    .iter()
                    .any(|taskbar| taskbar.handle == tracked.handle)
            })
        {
            return Err(Error::new(
                E_ACCESSDENIED,
                "protocol V1 cannot adopt a taskbar outside the leased identity set",
            ));
        }
        // Finish the read-only phase for every HWND before the first
        // SetWindowRgn. A custom region introduced after arming therefore
        // aborts the batch without partially suppressing another taskbar.
        for region in self
            .taskbar_regions
            .iter()
            .filter(|region| !region.suppressed)
        {
            ensure_default_window_region(region.handle)?;
        }
        Ok(())
    }
}

fn run_two_phase_mutation<T, E>(
    items: &mut [T],
    mut preflight: impl FnMut(&T) -> std::result::Result<(), E>,
    mut mutate: impl FnMut(&mut T) -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    for item in items.iter() {
        preflight(item)?;
    }
    for item in items {
        mutate(item)?;
    }
    Ok(())
}

fn authorize_post_appbar_taskbars(
    before: &[DiscoveredTaskbar],
    after: &[DiscoveredTaskbar],
) -> Option<Vec<AuthorizedTaskbarIdentity>> {
    same_stable_taskbar_set(before, after)
        .then(|| after.iter().map(AuthorizedTaskbarIdentity::from).collect())
}

fn same_stable_taskbar_set(first: &[DiscoveredTaskbar], second: &[DiscoveredTaskbar]) -> bool {
    first.len() == second.len()
        && first.iter().all(|expected| {
            let expected = StableTaskbarIdentity::from(expected);
            second
                .iter()
                .any(|taskbar| expected == StableTaskbarIdentity::from(taskbar))
        })
}

fn same_authorized_taskbar_set(
    authorized: &[AuthorizedTaskbarIdentity],
    current: &[DiscoveredTaskbar],
) -> bool {
    authorized.len() == current.len()
        && authorized.iter().all(|expected| {
            current
                .iter()
                .any(|taskbar| *expected == AuthorizedTaskbarIdentity::from(taskbar))
        })
}

fn ensure_valid_lease(lease: &TaskbarMutationLease) -> Result<()> {
    if lease.is_valid() {
        Ok(())
    } else {
        Err(Error::new(
            E_ACCESSDENIED,
            "taskbar mutation blocked because the watchdog lease is no longer valid",
        ))
    }
}

impl Drop for ExplorerTaskbarVisibilityGuard {
    fn drop(&mut self) {
        let current_taskbars = discover_explorer_taskbars().unwrap_or_default();
        for claim in std::mem::take(&mut self.edge_claims) {
            release_edge_claim(claim, &current_taskbars);
        }
        let _ = set_explorer_taskbar_state(self.original_appbar_state, self.appbar_owner);
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
    fn capture_default(handle: isize) -> Result<Self> {
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
        // SAFETY: GetWindowRgn never transfers ownership of the destination.
        let deleted = unsafe { DeleteObject(HGDIOBJ(original_region.0)) }.as_bool();
        if !deleted {
            return Err(Error::from_thread());
        }
        if region_type.0 != ERROR {
            return Err(Error::new(
                E_ACCESSDENIED,
                "protocol V1 refuses to replace a pre-existing taskbar window region",
            ));
        }
        Ok(Self {
            handle,
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
        // SAFETY: Protocol V1 accepts only a pre-existing default region. None
        // therefore restores the exact supported state after our empty region.
        let _ = unsafe { SetWindowRgn(hwnd, None, true) };
        self.suppressed = false;
    }
}

fn claim_taskbar_edge(
    dock: DockWindow,
    taskbars: &[DiscoveredTaskbar],
) -> Result<TaskbarEdgeClaim> {
    let previous_owner = auto_hide_owner(dock.monitor);
    match authorize_edge_owner(previous_owner, dock, taskbars)? {
        EdgeOwnerAuthorization::DockAlreadyOwns => {
            return Ok(TaskbarEdgeClaim {
                dock_handle: dock.handle,
                monitor: dock.monitor,
                previous_owner: None,
            });
        }
        EdgeOwnerAuthorization::Explorer(owner) => {
            ensure_live_authorized_explorer_owner(owner, dock.monitor, taskbars)?;
            if !set_auto_hide_owner(owner, dock.monitor, false) {
                return Err(Error::new(
                    E_FAIL,
                    "failed to release Explorer's authorized autohide edge",
                ));
            }
        }
        EdgeOwnerAuthorization::Unowned => {}
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
        previous_owner,
    })
}

fn authorize_edge_owner(
    owner: Option<isize>,
    dock: DockWindow,
    taskbars: &[DiscoveredTaskbar],
) -> Result<EdgeOwnerAuthorization> {
    match owner {
        None => Ok(EdgeOwnerAuthorization::Unowned),
        Some(owner) if owner == dock.handle => Ok(EdgeOwnerAuthorization::DockAlreadyOwns),
        Some(owner)
            if taskbars
                .iter()
                .any(|taskbar| taskbar.handle == owner && taskbar.monitor == dock.monitor) =>
        {
            Ok(EdgeOwnerAuthorization::Explorer(owner))
        }
        Some(_) => Err(Error::new(
            E_ACCESSDENIED,
            "the taskbar edge is owned by an unjournaled third-party appbar",
        )),
    }
}

fn ensure_live_authorized_explorer_owner(
    owner: isize,
    monitor: ScreenRect,
    authorized: &[DiscoveredTaskbar],
) -> Result<()> {
    let expected = authorized
        .iter()
        .find(|taskbar| taskbar.handle == owner && taskbar.monitor == monitor)
        .map(StableTaskbarIdentity::from)
        .ok_or_else(|| Error::new(E_ACCESSDENIED, "Explorer edge owner is not authorized"))?;
    let current = discover_explorer_taskbars()?;
    if current
        .iter()
        .any(|taskbar| StableTaskbarIdentity::from(taskbar) == expected)
    {
        Ok(())
    } else {
        Err(Error::new(
            E_ACCESSDENIED,
            "Explorer edge owner changed before the autohide claim",
        ))
    }
}

fn release_edge_claim(claim: TaskbarEdgeClaim, _taskbars: &[DiscoveredTaskbar]) {
    let _ = set_auto_hide_owner(claim.dock_handle, claim.monitor, false);
    remove_appbar(claim.dock_handle);
    if let Some(previous_owner) = claim.previous_owner {
        let _ = set_auto_hide_owner(previous_owner, claim.monitor, true);
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

fn wait_for_released_bottom_work_areas() -> Result<()> {
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
        match work_area_wait_decision(released, Instant::now() >= deadline) {
            WorkAreaWaitDecision::Ready => return Ok(()),
            WorkAreaWaitDecision::TimedOut => {
                return Err(Error::new(
                    E_FAIL,
                    "taskbar work area was not released before the safety deadline",
                ));
            }
            WorkAreaWaitDecision::Retry => {}
        }
        thread::sleep(WORK_AREA_SETTLE_INTERVAL);
    }
}

const fn work_area_wait_decision(released: bool, deadline_reached: bool) -> WorkAreaWaitDecision {
    if released {
        WorkAreaWaitDecision::Ready
    } else if deadline_reached {
        WorkAreaWaitDecision::TimedOut
    } else {
        WorkAreaWaitDecision::Retry
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

fn set_explorer_taskbar_state(state: u32, appbar_owner: Option<isize>) -> Result<()> {
    let appbar_owner = appbar_owner.ok_or_else(|| {
        Error::new(
            E_FAIL,
            "the primary Explorer taskbar is unavailable for ABM_SETSTATE",
        )
    })?;
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: HWND(appbar_owner as *mut _),
        lParam: LPARAM(state as isize),
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). ABM_SETSTATE consumes the scalar state
    // and the documented live taskbar HWND in the sized APPBARDATA.
    let response = unsafe { SHAppBarMessage(ABM_SETSTATE, &mut data) };
    if response == 0 {
        return Err(Error::new(
            E_FAIL,
            "ABM_SETSTATE rejected the taskbar state",
        ));
    }
    Ok(())
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

/// Produces the child's independent, read-only V1 observation. The watchdog
/// repeats discovery with its stricter Explorer image validation and must match
/// this fingerprint before it can persist an arming transaction.
pub(super) fn taskbar_state_fingerprint() -> Result<TaskbarStateFingerprint> {
    Ok(observe_taskbar_arming_state()?.fingerprint)
}

fn observe_taskbar_arming_state() -> Result<TaskbarArmingObservation> {
    let taskbars = discover_explorer_taskbars()?;
    for taskbar in &taskbars {
        ensure_default_window_region(taskbar.handle)?;
    }
    let explorer_process_id = taskbars.first().map_or(0, |taskbar| taskbar.process_id);
    if taskbars
        .iter()
        .any(|taskbar| taskbar.process_id != explorer_process_id)
    {
        return Err(Error::new(
            E_FAIL,
            "taskbar windows were owned by more than one process",
        ));
    }
    let snapshots = taskbars
        .iter()
        .map(|taskbar| {
            TaskbarFingerprintSnapshot::new(
                taskbar.device.clone(),
                if taskbar.is_primary {
                    TaskbarWindowClass::Primary
                } else {
                    TaskbarWindowClass::Secondary
                },
                taskbar.is_visible,
                TaskbarFingerprintBounds::new(
                    taskbar.window_bounds.left,
                    taskbar.window_bounds.top,
                    taskbar.window_bounds.right,
                    taskbar.window_bounds.bottom,
                ),
            )
        })
        .collect::<Vec<_>>();
    let appbar_state = explorer_taskbar_state();
    let fingerprint = taskbar_arming_fingerprint_v1(
        crate::win32::current_session_id()?,
        explorer_process_id,
        appbar_state,
        &snapshots,
    )
    .map_err(|error| {
        Error::new(
            E_FAIL,
            format!("the current taskbar state cannot be armed by protocol V1: {error}"),
        )
    })?;
    Ok(TaskbarArmingObservation {
        fingerprint,
        appbar_state,
        taskbars,
    })
}

fn ensure_default_window_region(handle: isize) -> Result<()> {
    // `capture_default` performs the same bounded read-only region query and
    // rejects Empty/Simple/Complex. The returned guard has not mutated state.
    let _guard = TaskbarRegionGuard::capture_default(handle)?;
    Ok(())
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
    ) && let Ok((monitor, device, window_bounds)) = taskbar_geometry(hwnd)
    {
        // SAFETY: Category 8 (FFI boundary). Visibility is queried synchronously
        // for the HWND supplied by EnumWindows.
        let is_visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
        let mut process_id = 0;
        // SAFETY: The callback supplies a live HWND and Windows writes one
        // process id to stack storage for this synchronous query.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        taskbars.push(DiscoveredTaskbar {
            handle: hwnd.0 as isize,
            process_id,
            is_visible,
            is_primary: class_name == "Shell_TrayWnd",
            monitor,
            device,
            window_bounds,
        });
    }
    true.into()
}

fn taskbar_geometry(hwnd: HWND) -> Result<(ScreenRect, String, ScreenRect)> {
    // SAFETY: The taskbar HWND was synchronously enumerated. The nearest
    // monitor is used only to obtain the current device and physical bounds.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: MONITORINFOEXW starts with the correctly sized MONITORINFO
    // header and stays writable for this synchronous query.
    if !unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) }.as_bool() {
        return Err(Error::from_thread());
    }
    let device_length = info
        .szDevice
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(info.szDevice.len());
    if device_length == 0 {
        return Err(Error::new(E_FAIL, "taskbar monitor has no device identity"));
    }
    let mut bounds = RECT::default();
    // SAFETY: The stack RECT is writable and the HWND came from the current
    // synchronous enumeration callback.
    unsafe { GetWindowRect(hwnd, &mut bounds) }?;
    Ok((
        ScreenRect::from_win32(info.monitorInfo.rcMonitor),
        String::from_utf16_lossy(&info.szDevice[..device_length]),
        ScreenRect::from_win32(bounds),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        AuthorizedTaskbarIdentity, DiscoveredTaskbar, DockWindow, EdgeOwnerAuthorization,
        ScreenRect, TrackedTaskbar, WorkAreaWaitDecision, active_taskbar_state,
        appbar_owner_handle, authorize_edge_owner, authorize_post_appbar_taskbars,
        bottom_work_area_is_released, order_taskbars_for_hide, reconcile_taskbars,
        run_two_phase_mutation, same_authorized_taskbar_set, work_area_wait_decision,
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

    fn taskbar(handle: isize, is_visible: bool, is_primary: bool) -> DiscoveredTaskbar {
        let monitor = if is_primary {
            PRIMARY_MONITOR
        } else {
            SECONDARY_MONITOR
        };
        DiscoveredTaskbar {
            handle,
            process_id: 100,
            is_visible,
            is_primary,
            monitor,
            device: if is_primary { "DISPLAY1" } else { "DISPLAY2" }.to_owned(),
            window_bounds: ScreenRect {
                top: monitor.bottom - 40,
                ..monitor
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
        assert_eq!(
            work_area_wait_decision(false, true),
            WorkAreaWaitDecision::TimedOut
        );
    }

    #[test]
    fn leased_identity_set_rejects_a_new_taskbar_before_any_reconcile_effect() {
        let prepared = [taskbar(11, true, true)];
        let authorized = prepared
            .iter()
            .map(AuthorizedTaskbarIdentity::from)
            .collect::<Vec<_>>();
        let current = vec![taskbar(11, true, true), taskbar(22, true, false)];

        assert!(!same_authorized_taskbar_set(&authorized, &current));
    }

    #[test]
    fn appbar_transition_authorizes_its_new_bounds_but_not_a_later_bounds_change() {
        let before = [taskbar(11, true, true)];
        let mut after = before.clone();
        after[0].window_bounds.top = PRIMARY_MONITOR.bottom;

        let authorized = authorize_post_appbar_taskbars(&before, &after)
            .expect("ABM may change bounds while stable identity remains exact");
        assert!(same_authorized_taskbar_set(&authorized, &after));

        let mut changed_later = after;
        changed_later[0].window_bounds.left += 1;
        assert!(!same_authorized_taskbar_set(&authorized, &changed_later));
    }

    #[test]
    fn appbar_transition_rejects_device_monitor_and_topology_changes() {
        let before = [taskbar(11, true, true)];
        let mut changed_device = before.clone();
        changed_device[0].device = "DISPLAY9".to_owned();
        assert!(authorize_post_appbar_taskbars(&before, &changed_device).is_none());

        let mut changed_monitor = before.clone();
        changed_monitor[0].monitor.right -= 1;
        assert!(authorize_post_appbar_taskbars(&before, &changed_monitor).is_none());

        let added = [taskbar(11, true, true), taskbar(22, true, false)];
        assert!(authorize_post_appbar_taskbars(&before, &added).is_none());
    }

    #[test]
    fn region_batch_completes_every_preflight_before_any_mutation() {
        let mut mutated = [false, false];
        let mut inspected = 0;
        let result = run_two_phase_mutation(
            &mut mutated,
            |_| {
                inspected += 1;
                if inspected == 2 { Err(()) } else { Ok(()) }
            },
            |value| {
                *value = true;
                Ok(())
            },
        );

        assert_eq!(result, Err(()));
        assert_eq!(mutated, [false, false]);
    }

    #[test]
    fn arbitrary_edge_owner_is_rejected_before_appbar_mutation() {
        let dock = DockWindow {
            handle: 99,
            monitor: PRIMARY_MONITOR,
        };
        let authorized = [taskbar(11, true, true)];

        assert!(authorize_edge_owner(Some(777), dock, &authorized).is_err());
        assert_eq!(
            authorize_edge_owner(Some(11), dock, &authorized).expect("Explorer owner"),
            EdgeOwnerAuthorization::Explorer(11)
        );
    }
}
