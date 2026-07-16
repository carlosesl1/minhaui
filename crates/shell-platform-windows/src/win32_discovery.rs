use std::collections::{HashMap, HashSet};

use shell_core::{MonitorId, WindowId};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, Result};

use crate::{FullscreenObservation, ObservedWindow, PreviewCapture};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct WindowIdentityKey {
    hwnd: isize,
    process: u32,
}

impl WindowIdentityKey {
    const fn new(hwnd: isize, process: u32) -> Self {
        Self { hwnd, process }
    }
}

pub(super) struct WindowIdentityCache<T = crate::win32_app_identity::ResolvedAppIdentity> {
    entries: HashMap<WindowIdentityKey, Option<T>>,
}

impl<T> Default for WindowIdentityCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T: Clone> WindowIdentityCache<T> {
    fn resolve_with(
        &mut self,
        key: WindowIdentityKey,
        resolve: impl FnOnce() -> Option<T>,
    ) -> Option<T> {
        if let Some(identity) = self.entries.get(&key) {
            return identity.clone();
        }
        let identity = resolve();
        self.entries.insert(key, identity.clone());
        identity
    }

    fn retain_keys(&mut self, live: &HashSet<WindowIdentityKey>) {
        self.entries.retain(|key, _| live.contains(key));
    }
}

pub(super) fn discover_running_windows(
    excluded: &[HWND],
    previews_enabled: bool,
    identity_cache: &mut WindowIdentityCache,
) -> Result<Vec<ObservedWindow>> {
    let mut state = EnumState {
        excluded: excluded.iter().map(|hwnd| hwnd.0 as isize).collect(),
        foreground: {
            // SAFETY: Category 8 (FFI boundary). Foreground HWND lookup has no
            // parameters and returns null when no window owns foreground.
            unsafe { GetForegroundWindow().0 as isize }
        },
        current_process: {
            // SAFETY: Category 8 (FFI boundary). The call has no parameters and
            // returns the current process identifier.
            unsafe { GetCurrentProcessId() }
        },
        previews_enabled,
        identity_cache,
        live_identity_keys: HashSet::new(),
        windows: Vec::new(),
    };
    // SAFETY: Category 8 (FFI boundary). `state` lives for the whole synchronous
    // enumeration call and the callback only casts the lparam back to this type.
    unsafe { EnumWindows(Some(enum_window), LPARAM(&mut state as *mut _ as isize)) }?;
    state.identity_cache.retain_keys(&state.live_identity_keys);
    Ok(state.windows)
}

struct EnumState<'cache> {
    excluded: Vec<isize>,
    foreground: isize,
    current_process: u32,
    previews_enabled: bool,
    identity_cache: &'cache mut WindowIdentityCache,
    live_identity_keys: HashSet<WindowIdentityKey>,
    windows: Vec<ObservedWindow>,
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: Category 8 (FFI boundary). `lparam` was created from `&mut
    // EnumState` in `discover_running_windows` and EnumWindows is synchronous.
    let state = unsafe { &mut *(lparam.0 as *mut EnumState) };
    if let Some(window) = observed_window(hwnd, state) {
        state.windows.push(window);
    }
    true.into()
}

fn observed_window(hwnd: HWND, state: &mut EnumState<'_>) -> Option<ObservedWindow> {
    if state.excluded.contains(&(hwnd.0 as isize)) || !eligible_window(hwnd) {
        return None;
    }
    let process = process_id(hwnd)?;
    if process == state.current_process {
        return None;
    }
    let identity_key = WindowIdentityKey::new(hwnd.0 as isize, process);
    state.live_identity_keys.insert(identity_key);
    let identity = state.identity_cache.resolve_with(identity_key, || {
        crate::win32_app_identity::app_for_window(hwnd)
    })?;
    let window_id = WindowId::new(hwnd.0 as usize as u64);
    let observed = ObservedWindow::new(
        window_id,
        identity.app().clone(),
        hwnd.0 as isize == state.foreground,
        // SAFETY: Category 8 (FFI boundary). The HWND is from EnumWindows and
        // still valid for this synchronous minimized-state query.
        unsafe { IsIconic(hwnd) }.as_bool(),
    )
    .with_aliases(identity.aliases().to_vec())
    .with_title(window_title(hwnd));
    Some(
        match preview_capture_for_discovery(state.previews_enabled) {
            Some(capture) => observed.with_preview(capture),
            None => observed,
        },
    )
    .map(|window| match fullscreen_observation(hwnd, window_id) {
        Some(fullscreen) => window.with_fullscreen(fullscreen),
        None => window,
    })
}

fn window_title(hwnd: HWND) -> String {
    // SAFETY: Category 8 (FFI boundary). The HWND comes from synchronous
    // EnumWindows enumeration; the length query does not retain the handle.
    let length = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut buffer = vec![0_u16; length.saturating_add(1)];
    // SAFETY: Category 8 (FFI boundary). The UTF-16 buffer is writable for its
    // full capacity and the call copies at most that capacity synchronously.
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied.min(buffer.len())])
}

const fn preview_capture_for_discovery(previews_enabled: bool) -> Option<PreviewCapture> {
    if previews_enabled {
        Some(PreviewCapture::dwm_thumbnail())
    } else {
        None
    }
}

fn eligible_window(hwnd: HWND) -> bool {
    // SAFETY: Category 8 (FFI boundary). The HWND is supplied by EnumWindows and
    // is valid during this callback.
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return false;
    }
    // SAFETY: Category 8 (FFI boundary). Owner lookup is read-only for this live
    // HWND; an owner means this is not a top-level app window.
    if unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok() {
        return false;
    }
    // SAFETY: Category 8 (FFI boundary). Reads the extended style from the live
    // HWND so tool windows can be filtered.
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
        return false;
    }
    // SAFETY: Category 8 (FFI boundary). Length query is read-only and returns 0
    // for untitled surfaces, which are not useful dock app windows.
    (unsafe { GetWindowTextLengthW(hwnd) }) > 0
}

fn process_id(hwnd: HWND) -> Option<u32> {
    let mut process = 0;
    // SAFETY: Category 8 (FFI boundary). The process-id out pointer is valid for
    // the duration of the call and the HWND came from EnumWindows.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process)) };
    (process != 0).then_some(process)
}

fn fullscreen_observation(hwnd: HWND, window: WindowId) -> Option<FullscreenObservation> {
    let mut window_rect = RECT::default();
    // SAFETY: Category 8 (FFI boundary). The HWND is supplied by EnumWindows and
    // `window_rect` is valid writable storage for the synchronous query.
    unsafe { GetWindowRect(hwnd, &mut window_rect) }.ok()?;
    // SAFETY: Category 8 (FFI boundary). The default-nearest flag gives the
    // monitor containing the enumerated HWND.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: Category 8 (FFI boundary). `info` has the documented size field
    // and valid writable storage for this monitor query.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    (window_rect.left == info.rcMonitor.left
        && window_rect.top == info.rcMonitor.top
        && window_rect.right == info.rcMonitor.right
        && window_rect.bottom == info.rcMonitor.bottom)
        .then_some(FullscreenObservation::new(
            window,
            MonitorId::new(monitor.0 as usize as u64),
            rect_from_win32(info.rcMonitor),
        ))
}

const fn rect_from_win32(rect: RECT) -> shell_renderer::PhysicalRect {
    shell_renderer::PhysicalRect::new(
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use crate::PreviewCapture;

    use super::{WindowIdentityCache, WindowIdentityKey, preview_capture_for_discovery};

    #[test]
    fn stable_window_identity_is_resolved_only_once() {
        let calls = Cell::new(0);
        let mut cache = WindowIdentityCache::default();
        let key = WindowIdentityKey::new(55, 9001);

        for _ in 0..3 {
            let identity = cache.resolve_with(key, || {
                calls.set(calls.get() + 1);
                Some("browser.exe".to_owned())
            });
            assert_eq!(identity.as_deref(), Some("browser.exe"));
        }

        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn inaccessible_window_identity_failure_is_cached() {
        let calls = Cell::new(0);
        let mut cache = WindowIdentityCache::<String>::default();
        let key = WindowIdentityKey::new(77, 9002);

        for _ in 0..3 {
            let identity = cache.resolve_with(key, || {
                calls.set(calls.get() + 1);
                None
            });
            assert_eq!(identity, None);
        }

        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn discovery_defers_dwm_validation_until_preview_open() {
        assert_eq!(
            preview_capture_for_discovery(true),
            Some(PreviewCapture::dwm_thumbnail())
        );
        assert_eq!(preview_capture_for_discovery(false), None);
    }
}
