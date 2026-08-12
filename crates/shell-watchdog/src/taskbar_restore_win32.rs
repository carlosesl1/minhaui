use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
use windows::Win32::Graphics::Gdi::{
    COMPLEXREGION, CreateRectRgn, DeleteObject, ERROR, GetMonitorInfoW, GetWindowRgn, HGDIOBJ,
    MONITOR_DEFAULTTONULL, MONITORINFO, MONITORINFOEXW, MonitorFromWindow, NULLREGION,
    SIMPLEREGION, SetWindowRgn,
};
use windows::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETAUTOHIDEBAREX, ABM_GETSTATE,
    ABM_SETAUTOHIDEBAREX, ABM_SETSTATE, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, SW_HIDE,
    SW_SHOWNOACTIVATE, ShowWindowAsync,
};
use windows::core::{BOOL, Error as WindowsError, PCWSTR, PWSTR};

use crate::RecoveryJournalV1;
use crate::TaskbarBounds;
use crate::taskbar_restore::{
    TaskbarObservation, TaskbarObservationId, TaskbarRestoreBackend, TaskbarRestoreError,
    TaskbarRestorePlan, TaskbarRestoreTarget, plan_taskbar_restore,
};

const PRIMARY_TASKBAR_CLASS: &str = "Shell_TrayWnd";
const SECONDARY_TASKBAR_CLASS: &str = "Shell_SecondaryTrayWnd";
const MAX_WINDOWS_PATH_UNITS: usize = 32_768;
const VERIFICATION_BUDGET: Duration = Duration::from_secs(2);
const VERIFICATION_INTERVAL: Duration = Duration::from_millis(50);

/// Atomically publishes a fully synchronized journal replacement on Windows.
///
/// Both paths are created in the same directory by the safe journal layer.
/// `MOVEFILE_REPLACE_EXISTING` is required because the durable `Prepared`
/// journal already exists, while `MOVEFILE_WRITE_THROUGH` keeps the transition
/// from reporting success before Windows flushes the move to disk.
pub(crate) fn replace_recovery_journal(staging_path: &Path, path: &Path) -> std::io::Result<()> {
    let staging = wide_path(staging_path)?;
    let destination = wide_path(path)?;
    // SAFETY: Both buffers are NUL-terminated, remain live for the synchronous
    // call, and identify same-directory regular files owned by the journal
    // transaction. The flags explicitly replace the existing Prepared file and
    // request a durable move before returning.
    unsafe {
        MoveFileExW(
            PCWSTR(staging.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| std::io::Error::other(error.to_string()))
}

fn wide_path(path: &Path) -> std::io::Result<Vec<u16>> {
    let mut value = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if value.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "recovery journal path contains an embedded NUL",
        ));
    }
    value.push(0);
    Ok(value)
}

pub(crate) struct Win32TaskbarRestoreBackend;

impl TaskbarRestoreBackend for Win32TaskbarRestoreBackend {
    fn observe(&self) -> Result<Vec<TaskbarObservation>, TaskbarRestoreError> {
        Ok(discover_explorer_taskbars()?
            .into_iter()
            .map(|taskbar| taskbar.observation)
            .collect())
    }

    fn preflight(
        &self,
        journal: &RecoveryJournalV1,
        _plan: &TaskbarRestorePlan,
    ) -> Result<(), TaskbarRestoreError> {
        preflight_current(journal).map(|_| ())
    }

    fn apply_and_verify(
        &self,
        journal: &RecoveryJournalV1,
        _plan: &TaskbarRestorePlan,
    ) -> Result<(), TaskbarRestoreError> {
        let (current, current_plan, disposition) = preflight_current(journal)?;
        if disposition == NativeRecoveryDisposition::AlreadyRestored {
            return Ok(());
        }

        for target in current_plan.targets() {
            let taskbar = native_target(&current, target)?;
            reset_empty_window_region(taskbar)?;
        }

        let primary = current_plan.primary_target().ok_or_else(|| {
            TaskbarRestoreError::verification("the primary Explorer taskbar disappeared")
        })?;
        let primary = native_target(&current, primary)?;
        set_appbar_state(primary.hwnd, current_plan.appbar_state())?;

        if current_plan.appbar_state() & ABS_AUTOHIDE != 0 {
            for target in current_plan.targets() {
                let taskbar = native_target(&current, target)?;
                set_auto_hide_owner(taskbar)?;
            }
        }

        for target in current_plan.targets() {
            let taskbar = native_target(&current, target)?;
            restore_visibility(taskbar.hwnd, target.snapshot.visible)?;
        }

        wait_until_verified(journal)
    }
}

fn preflight_current(
    journal: &RecoveryJournalV1,
) -> Result<
    (
        Vec<NativeTaskbar>,
        TaskbarRestorePlan,
        NativeRecoveryDisposition,
    ),
    TaskbarRestoreError,
> {
    // Rediscover immediately before mutation or a check-only verdict. The HWND
    // is absent from the durable journal and lives only in this bounded result.
    let current = discover_explorer_taskbars()?;
    let observations = current
        .iter()
        .map(|taskbar| taskbar.observation.clone())
        .collect::<Vec<_>>();
    let plan = plan_taskbar_restore(journal, &observations)?;

    let mut empty_regions = 0_usize;
    for target in plan.targets() {
        let taskbar = native_target(&current, target)?;
        match recoverable_region_state(taskbar)? {
            RecoverableRegionState::Empty => empty_regions += 1,
            RecoverableRegionState::NoRegion => {}
        }
        ensure_auto_hide_edge_available(taskbar)?;
    }
    let disposition = if empty_regions == 0 {
        // A crash after successful native restoration but before journal removal
        // must be idempotent. NoRegion is accepted only if every other journaled
        // state is already verified; otherwise V1 cannot prove it is ours.
        verify_current_state(journal)?;
        NativeRecoveryDisposition::AlreadyRestored
    } else {
        NativeRecoveryDisposition::NeedsRestore
    };
    Ok((current, plan, disposition))
}

struct NativeTaskbar {
    hwnd: HWND,
    process_id: u32,
    monitor_bounds: RECT,
    monitor_work_area: RECT,
    observation: TaskbarObservation,
}

struct EnumContext<'a> {
    expected_explorer_path: &'a [u16],
    taskbars: Vec<NativeTaskbar>,
    process_id: Option<u32>,
    failure: Option<TaskbarRestoreError>,
}

struct OwnedHandle(HANDLE);

struct ThreadDpiAwarenessGuard(DPI_AWARENESS_CONTEXT);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowRegionState {
    NoRegion,
    Empty,
    Simple,
    Complex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeRecoveryDisposition {
    NeedsRestore,
    AlreadyRestored,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoverableRegionState {
    NoRegion,
    Empty,
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: The handle was returned by OpenProcess to this owner and is
        // closed exactly once here after all synchronous path queries finish.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

impl ThreadDpiAwarenessGuard {
    fn per_monitor_v2() -> Result<Self, TaskbarRestoreError> {
        // SAFETY: This changes only the current watchdog thread for the bounded
        // discovery scope and returns the previous context for restoration.
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.0.is_null() {
            return Err(native_last_error(
                "entering per-monitor DPI awareness for taskbar discovery",
            ));
        }
        Ok(Self(previous))
    }
}

impl Drop for ThreadDpiAwarenessGuard {
    fn drop(&mut self) {
        // SAFETY: The value was returned as this thread's previous context and
        // is restored on the same thread before the discovery stack unwinds.
        let _ = unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}

fn discover_explorer_taskbars() -> Result<Vec<NativeTaskbar>, TaskbarRestoreError> {
    let _dpi_awareness = ThreadDpiAwarenessGuard::per_monitor_v2()?;
    let expected_explorer_path = expected_explorer_path()?;
    let mut context = EnumContext {
        expected_explorer_path: &expected_explorer_path,
        taskbars: Vec::new(),
        process_id: None,
        failure: None,
    };

    // SAFETY: EnumWindows is synchronous. The callback borrows `context` only
    // for the duration of this call and never retains its LPARAM pointer.
    let enumeration = unsafe {
        EnumWindows(
            Some(enum_taskbar_window),
            LPARAM((&raw mut context).cast::<()>() as isize),
        )
    };
    if let Some(error) = context.failure {
        return Err(error);
    }
    enumeration.map_err(|error| native_error("enumerating top-level windows", error))?;
    Ok(context.taskbars)
}

unsafe extern "system" fn enum_taskbar_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let context = lparam.0 as *mut EnumContext<'_>;
    if context.is_null() {
        return false.into();
    }
    // SAFETY: The non-null pointer was created from the live EnumContext by the
    // synchronous EnumWindows caller and is never used after enumeration.
    let context = unsafe { &mut *context };

    match query_taskbar(hwnd, context.expected_explorer_path) {
        Ok(None) => true.into(),
        Ok(Some(taskbar)) => {
            if context
                .process_id
                .is_some_and(|process_id| process_id != taskbar.process_id)
            {
                context.failure = Some(TaskbarRestoreError::verification(
                    "Explorer taskbar windows were owned by multiple processes",
                ));
                return false.into();
            }
            context.process_id = Some(taskbar.process_id);
            context.taskbars.push(taskbar);
            true.into()
        }
        Err(error) => {
            context.failure = Some(error);
            false.into()
        }
    }
}

fn query_taskbar(
    hwnd: HWND,
    expected_explorer_path: &[u16],
) -> Result<Option<NativeTaskbar>, TaskbarRestoreError> {
    let class_name = window_class(hwnd);
    if !matches!(
        class_name.as_deref(),
        Some(PRIMARY_TASKBAR_CLASS | SECONDARY_TASKBAR_CLASS)
    ) {
        return Ok(None);
    }
    let class_name = class_name.ok_or_else(|| {
        TaskbarRestoreError::verification("a candidate taskbar had no readable window class")
    })?;

    let (process_id, process_path) = window_process_path(hwnd)?;
    // SAFETY: Both slices are bounded owned UTF-16 paths, so CompareStringOrdinal
    // performs a locale-independent, case-insensitive synchronous comparison.
    let trusted =
        unsafe { CompareStringOrdinal(&process_path, expected_explorer_path, true) } == CSTR_EQUAL;
    if !trusted {
        return Ok(None);
    }

    let monitor = window_monitor_and_bounds(hwnd)?;
    // SAFETY: `hwnd` comes directly from the synchronous EnumWindows callback.
    let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
    Ok(Some(NativeTaskbar {
        hwnd,
        process_id,
        monitor_bounds: monitor.monitor_bounds,
        monitor_work_area: monitor.monitor_work_area,
        observation: TaskbarObservation::new(
            observation_id(hwnd),
            monitor.device,
            class_name,
            visible,
            monitor.window_bounds,
        ),
    }))
}

struct WindowMonitorObservation {
    device: String,
    window_bounds: TaskbarBounds,
    monitor_bounds: RECT,
    monitor_work_area: RECT,
}

fn window_class(hwnd: HWND) -> Option<String> {
    let mut buffer = [0_u16; 64];
    // SAFETY: The fixed stack buffer remains writable for this synchronous
    // class-name query; HWND comes from live top-level enumeration.
    let copied = unsafe { GetClassNameW(hwnd, &mut buffer) };
    (copied > 0).then(|| String::from_utf16_lossy(&buffer[..copied as usize]))
}

fn window_process_path(hwnd: HWND) -> Result<(u32, Vec<u16>), TaskbarRestoreError> {
    let mut process_id = 0_u32;
    // SAFETY: The writable process-id storage outlives this synchronous query.
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut process_id)) };
    if thread_id == 0 || process_id == 0 {
        return Err(native_last_error("resolving a candidate taskbar owner"));
    }

    // SAFETY: The process id was returned by Windows for the candidate HWND;
    // only limited query access is requested and no remote memory is touched.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map(OwnedHandle)
        .map_err(|error| native_error("opening a candidate taskbar owner", error))?;
    let mut path = vec![0_u16; MAX_WINDOWS_PATH_UNITS];
    let mut length = u32::try_from(path.len()).map_err(|_| {
        TaskbarRestoreError::verification("the bounded process-path buffer was invalid")
    })?;
    // SAFETY: `path` is a writable buffer of `length` UTF-16 units and the
    // limited-query handle remains owned for the complete synchronous call.
    unsafe {
        QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &raw mut length,
        )
    }
    .map_err(|error| native_error("reading a candidate taskbar owner path", error))?;
    let length = usize::try_from(length).map_err(|_| {
        TaskbarRestoreError::verification("Windows returned an invalid process-path length")
    })?;
    if length == 0 || length > path.len() {
        return Err(TaskbarRestoreError::verification(
            "Windows returned an empty or oversized process path",
        ));
    }
    path.truncate(length);
    Ok((process_id, path))
}

fn expected_explorer_path() -> Result<Vec<u16>, TaskbarRestoreError> {
    let mut windows_directory = [0_u16; MAX_WINDOWS_PATH_UNITS];
    // SAFETY: The fixed UTF-16 buffer remains writable for this synchronous
    // system-directory query and is larger than the documented maximum path.
    let length = unsafe { GetWindowsDirectoryW(Some(&mut windows_directory)) } as usize;
    if length == 0 || length >= windows_directory.len() {
        return Err(native_last_error("resolving the trusted Windows directory"));
    }
    let mut path = windows_directory[..length].to_vec();
    if !path.ends_with(&[u16::from(b'\\')]) && !path.ends_with(&[u16::from(b'/')]) {
        path.push(u16::from(b'\\'));
    }
    path.extend("explorer.exe".encode_utf16());
    Ok(path)
}

fn window_monitor_and_bounds(hwnd: HWND) -> Result<WindowMonitorObservation, TaskbarRestoreError> {
    // SAFETY: HWND was synchronously enumerated. MONITOR_DEFAULTTONULL prevents
    // silently assigning an unrelated monitor when topology changes.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) };
    if monitor.0.is_null() {
        return Err(TaskbarRestoreError::verification(
            "a candidate taskbar was not attached to a current monitor",
        ));
    }
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: MONITORINFOEXW begins with MONITORINFO and advertises its full
    // size, as required by GetMonitorInfoW for the extended device name.
    if !unsafe { GetMonitorInfoW(monitor, &raw mut info.monitorInfo) }.as_bool() {
        return Err(native_last_error("reading a candidate taskbar monitor"));
    }
    let device_length = info
        .szDevice
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(info.szDevice.len());
    if device_length == 0 {
        return Err(TaskbarRestoreError::verification(
            "a candidate taskbar monitor had no device identity",
        ));
    }

    let mut rect = RECT::default();
    // SAFETY: The RECT is writable for this synchronous query and HWND was
    // enumerated and ownership-validated immediately before this call.
    unsafe { GetWindowRect(hwnd, &raw mut rect) }
        .map_err(|error| native_error("reading candidate taskbar bounds", error))?;
    Ok(WindowMonitorObservation {
        device: String::from_utf16_lossy(&info.szDevice[..device_length]),
        window_bounds: TaskbarBounds::new(rect.left, rect.top, rect.right, rect.bottom),
        monitor_bounds: info.monitorInfo.rcMonitor,
        monitor_work_area: info.monitorInfo.rcWork,
    })
}

fn reset_empty_window_region(taskbar: &NativeTaskbar) -> Result<(), TaskbarRestoreError> {
    match recoverable_region_state(taskbar)? {
        RecoverableRegionState::NoRegion => return Ok(()),
        RecoverableRegionState::Empty => {}
    }
    // SAFETY: HWND was rediscovered by class, Explorer image path, monitor and
    // bounds in this call. None removes the empty region created by the current
    // experimental mutation and transfers no caller-owned GDI object.
    if unsafe { SetWindowRgn(taskbar.hwnd, None, true) } == 0 {
        return Err(native_last_error(
            "resetting an Explorer taskbar window region",
        ));
    }
    Ok(())
}

fn recoverable_region_state(
    taskbar: &NativeTaskbar,
) -> Result<RecoverableRegionState, TaskbarRestoreError> {
    let region = window_region_state(taskbar.hwnd)?;
    match region {
        WindowRegionState::Empty => Ok(RecoverableRegionState::Empty),
        WindowRegionState::NoRegion => Ok(RecoverableRegionState::NoRegion),
        WindowRegionState::Simple | WindowRegionState::Complex => {
            Err(TaskbarRestoreError::verification(format!(
                "taskbar region on {} was {region:?}, but journal V1 can restore only the known empty mutation region",
                taskbar.observation.device,
            )))
        }
    }
}

fn set_appbar_state(hwnd: HWND, state: u32) -> Result<(), TaskbarRestoreError> {
    let mut data = APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        lParam: LPARAM(state as isize),
        ..Default::default()
    };
    // SAFETY: APPBARDATA is correctly sized and contains only the rediscovered
    // primary Explorer HWND plus the planner-validated scalar state bits.
    if unsafe { SHAppBarMessage(ABM_SETSTATE, &raw mut data) } == 0 {
        return Err(native_last_error("restoring the Explorer appbar state"));
    }
    Ok(())
}

fn restore_visibility(hwnd: HWND, visible: bool) -> Result<(), TaskbarRestoreError> {
    let command = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
    // SAFETY: HWND was rediscovered immediately before use. ShowWindowAsync
    // posts a bounded visibility request without activating or waiting on Explorer.
    if !unsafe { ShowWindowAsync(hwnd, command) }.as_bool() {
        return Err(native_last_error(
            "posting Explorer taskbar visibility restoration",
        ));
    }
    Ok(())
}

fn set_auto_hide_owner(taskbar: &NativeTaskbar) -> Result<(), TaskbarRestoreError> {
    let existing = auto_hide_owner(taskbar)?;
    if existing == taskbar.hwnd {
        return Ok(());
    }
    if !existing.0.is_null() {
        return Err(TaskbarRestoreError::verification(format!(
            "another appbar already owns the autohide edge on {}",
            taskbar.observation.device
        )));
    }
    let mut data = auto_hide_data(taskbar, true)?;
    // SAFETY: The taskbar was rediscovered and Explorer-owned. APPBARDATA uses
    // its live HWND, current monitor rectangle, derived edge and a scalar flag.
    if unsafe { SHAppBarMessage(ABM_SETAUTOHIDEBAREX, &raw mut data) } == 0 {
        return Err(native_last_error(
            "restoring an Explorer autohide edge owner",
        ));
    }
    Ok(())
}

fn ensure_auto_hide_edge_available(taskbar: &NativeTaskbar) -> Result<(), TaskbarRestoreError> {
    let owner = auto_hide_owner(taskbar)?;
    if owner.0.is_null() || owner == taskbar.hwnd {
        return Ok(());
    }
    Err(TaskbarRestoreError::verification(format!(
        "another appbar already owns the autohide edge on {}",
        taskbar.observation.device
    )))
}

fn auto_hide_owner(taskbar: &NativeTaskbar) -> Result<HWND, TaskbarRestoreError> {
    let mut data = auto_hide_data(taskbar, false)?;
    // SAFETY: This synchronous query uses only the current monitor rectangle and
    // planner-derived edge; the returned HWND is compared, never persisted.
    let owner = unsafe { SHAppBarMessage(ABM_GETAUTOHIDEBAREX, &raw mut data) };
    Ok(HWND(owner as *mut _))
}

fn auto_hide_data(
    taskbar: &NativeTaskbar,
    register: bool,
) -> Result<APPBARDATA, TaskbarRestoreError> {
    Ok(APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: taskbar.hwnd,
        uEdge: taskbar_edge(taskbar)?,
        rc: taskbar.monitor_bounds,
        lParam: LPARAM(isize::from(register)),
        ..Default::default()
    })
}

fn taskbar_edge(taskbar: &NativeTaskbar) -> Result<u32, TaskbarRestoreError> {
    let bounds = taskbar.observation.bounds;
    let monitor = taskbar.monitor_bounds;
    let candidates = [
        (
            bounds.top == monitor.top && bounds.bottom < monitor.bottom,
            ABE_TOP,
        ),
        (
            bounds.bottom == monitor.bottom && bounds.top > monitor.top,
            ABE_BOTTOM,
        ),
        (
            bounds.left == monitor.left && bounds.right < monitor.right,
            ABE_LEFT,
        ),
        (
            bounds.right == monitor.right && bounds.left > monitor.left,
            ABE_RIGHT,
        ),
    ];
    let mut edges = candidates
        .into_iter()
        .filter_map(|(matches, edge)| matches.then_some(edge));
    let Some(edge) = edges.next() else {
        return Err(TaskbarRestoreError::verification(format!(
            "taskbar bounds did not identify an edge on {}",
            taskbar.observation.device
        )));
    };
    if edges.next().is_some() {
        return Err(TaskbarRestoreError::verification(format!(
            "taskbar bounds ambiguously touched multiple edges on {}",
            taskbar.observation.device
        )));
    }
    Ok(edge)
}

fn wait_until_verified(journal: &RecoveryJournalV1) -> Result<(), TaskbarRestoreError> {
    let deadline = Instant::now() + VERIFICATION_BUDGET;
    loop {
        let error = match verify_current_state(journal) {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        if Instant::now() >= deadline {
            return Err(TaskbarRestoreError::verification(error.to_string()));
        }
        thread::sleep(VERIFICATION_INTERVAL);
    }
}

fn verify_current_state(journal: &RecoveryJournalV1) -> Result<(), TaskbarRestoreError> {
    let current = discover_explorer_taskbars()?;
    let observations = current
        .iter()
        .map(|taskbar| taskbar.observation.clone())
        .collect::<Vec<_>>();
    let plan = plan_taskbar_restore(journal, &observations)?;
    let state = current_appbar_state();
    if state != plan.appbar_state() {
        return Err(TaskbarRestoreError::verification(format!(
            "appbar state was 0x{state:08x}, expected 0x{:08x}",
            plan.appbar_state()
        )));
    }
    for target in plan.targets() {
        let taskbar = native_target(&current, target)?;
        if taskbar.observation.visible != target.snapshot.visible {
            return Err(TaskbarRestoreError::verification(format!(
                "visibility did not settle for {} on {}",
                target.snapshot.class_name, target.snapshot.device
            )));
        }
        if window_region_state(taskbar.hwnd)? != WindowRegionState::NoRegion {
            return Err(TaskbarRestoreError::verification(format!(
                "the default window region was not restored for {} on {}",
                target.snapshot.class_name, target.snapshot.device
            )));
        }
        verify_work_area(taskbar, plan.appbar_state())?;
        if plan.appbar_state() & ABS_AUTOHIDE != 0 && auto_hide_owner(taskbar)? != taskbar.hwnd {
            return Err(TaskbarRestoreError::verification(format!(
                "Explorer did not own the restored autohide edge on {}",
                target.snapshot.device
            )));
        }
    }
    Ok(())
}

fn verify_work_area(taskbar: &NativeTaskbar, appbar_state: u32) -> Result<(), TaskbarRestoreError> {
    let mut expected = taskbar.monitor_bounds;
    if appbar_state & ABS_AUTOHIDE == 0 {
        let bounds = taskbar.observation.bounds;
        match taskbar_edge(taskbar)? {
            ABE_TOP => expected.top = bounds.bottom,
            ABE_BOTTOM => expected.bottom = bounds.top,
            ABE_LEFT => expected.left = bounds.right,
            ABE_RIGHT => expected.right = bounds.left,
            _ => {
                return Err(TaskbarRestoreError::verification(
                    "the derived taskbar edge was unsupported",
                ));
            }
        }
    }
    if taskbar.monitor_work_area != expected {
        return Err(TaskbarRestoreError::verification(format!(
            "monitor work area did not settle for {}",
            taskbar.observation.device
        )));
    }
    Ok(())
}

fn current_appbar_state() -> u32 {
    let mut data = APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };
    // SAFETY: ABM_GETSTATE reads only the correctly sized synchronous buffer
    // and returns the current scalar ABS_* flags.
    unsafe { SHAppBarMessage(ABM_GETSTATE, &raw mut data) as u32 }
}

fn window_region_state(hwnd: HWND) -> Result<WindowRegionState, TaskbarRestoreError> {
    // SAFETY: This creates caller-owned temporary GDI storage for GetWindowRgn.
    let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
    if region.0.is_null() {
        return Err(native_last_error(
            "allocating taskbar region verification storage",
        ));
    }
    // SAFETY: The rediscovered HWND and caller-owned region are valid for this
    // synchronous query. A zero result means the window has the default region.
    let region_type = unsafe { GetWindowRgn(hwnd, region) };
    // SAFETY: GetWindowRgn never takes ownership of the supplied region.
    let deleted = unsafe { DeleteObject(HGDIOBJ(region.0)) }.as_bool();
    if !deleted {
        return Err(native_last_error(
            "releasing taskbar region verification storage",
        ));
    }
    // GetWindowRgn documents ERROR for both "no window region" and a query
    // error. This adapter accepts it as NoRegion only after the HWND has been
    // freshly enumerated and validated as the Explorer-owned class/device/bounds
    // target; every custom region has a distinct documented complexity result.
    region_state_from_code(region_type.0)
}

fn region_state_from_code(value: i32) -> Result<WindowRegionState, TaskbarRestoreError> {
    match value {
        ERROR => Ok(WindowRegionState::NoRegion),
        value if value == NULLREGION.0 => Ok(WindowRegionState::Empty),
        value if value == SIMPLEREGION.0 => Ok(WindowRegionState::Simple),
        value if value == COMPLEXREGION.0 => Ok(WindowRegionState::Complex),
        value => Err(TaskbarRestoreError::verification(format!(
            "GetWindowRgn returned unsupported region type {value}"
        ))),
    }
}

fn native_target<'a>(
    current: &'a [NativeTaskbar],
    target: &TaskbarRestoreTarget,
) -> Result<&'a NativeTaskbar, TaskbarRestoreError> {
    current
        .iter()
        .find(|taskbar| taskbar.observation.id == target.id)
        .ok_or_else(|| {
            TaskbarRestoreError::verification(format!(
                "the rediscovered {} on {} changed identity",
                target.snapshot.class_name, target.snapshot.device
            ))
        })
}

fn observation_id(hwnd: HWND) -> TaskbarObservationId {
    TaskbarObservationId::new(hwnd.0 as usize as u64)
}

fn native_last_error(operation: &'static str) -> TaskbarRestoreError {
    native_error(operation, WindowsError::from_thread())
}

fn native_error(operation: &'static str, error: WindowsError) -> TaskbarRestoreError {
    TaskbarRestoreError::native(operation, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{WindowRegionState, region_state_from_code, replace_recovery_journal};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use windows::Win32::Graphics::Gdi::{COMPLEXREGION, ERROR, NULLREGION, SIMPLEREGION};

    #[test]
    fn documented_region_codes_have_explicit_recovery_meaning() {
        assert_eq!(
            region_state_from_code(ERROR).ok(),
            Some(WindowRegionState::NoRegion)
        );
        assert_eq!(
            region_state_from_code(NULLREGION.0).ok(),
            Some(WindowRegionState::Empty)
        );
        assert_eq!(
            region_state_from_code(SIMPLEREGION.0).ok(),
            Some(WindowRegionState::Simple)
        );
        assert_eq!(
            region_state_from_code(COMPLEXREGION.0).ok(),
            Some(WindowRegionState::Complex)
        );
        assert!(region_state_from_code(99).is_err());
    }

    #[test]
    fn windows_journal_commit_replaces_an_existing_prepared_file() -> Result<(), std::io::Error> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "minhaui-watchdog-replace-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let prepared = directory.join("taskbar-recovery-v1.json");
        let staging = directory.join(".taskbar-recovery-v1.json.staging.tmp");
        fs::write(&prepared, b"prepared")?;
        fs::write(&staging, b"applied")?;

        replace_recovery_journal(&staging, &prepared)?;

        assert_eq!(fs::read(&prepared)?, b"applied");
        assert!(!staging.exists());
        fs::remove_file(prepared)?;
        fs::remove_dir(directory)?;
        Ok(())
    }
}
