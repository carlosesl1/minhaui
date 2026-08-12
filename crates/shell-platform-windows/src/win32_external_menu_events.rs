use core::ffi::c_void;
use std::sync::{Mutex, MutexGuard, OnceLock};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_MENUPOPUPEND, EVENT_SYSTEM_MENUPOPUPSTART, GetForegroundWindow,
    GetWindowThreadProcessId, IsWindow, PostMessageW, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP,
};

use crate::win32_event_queue::{RoutedPlatformEvent, queue_event_with_wake};
use crate::{NativeWindowId, PlatformEvent};

pub(super) const EXTERNAL_MENU_WAKE_MESSAGE: u32 = WM_APP + 0x66;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ForegroundWindowOwner {
    pub(super) window: NativeWindowId,
    pub(super) process_id: u32,
}

/// Returns the live foreground window and its current owner PID.
///
/// Runtime callers invoke this only while an external-menu hold is active;
/// it is deliberately not a background polling source.
pub(super) fn foreground_window_owner() -> Option<ForegroundWindowOwner> {
    // SAFETY: Category 8 (FFI boundary). The call returns a process-owned HWND
    // value; no application memory is dereferenced.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    // SAFETY: Category 8 (FFI boundary). IsWindow validates the scalar HWND
    // before the owner PID query below.
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        return None;
    }
    let process_id = window_owner_process_id(hwnd)?;
    Some(ForegroundWindowOwner {
        window: NativeWindowId::new(hwnd.0 as isize),
        process_id,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExternalMenuEventKind {
    Started,
    Ended,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum ExternalMenuHookError {
    #[error("external menu hook registration is invalid")]
    InvalidRegistration,
    #[error("external menu hook is unavailable")]
    HookUnavailable,
}

trait SharedHookOps {
    fn install_hook(&self) -> Option<isize>;
    fn uninstall_hook(&self, hook: isize);
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HookRegistration {
    id: u64,
    wake_window: NativeWindowId,
    owner_process_ids: Box<[u32]>,
}

#[derive(Debug, Default)]
struct HookRegistry {
    hook: Option<isize>,
    next_id: u64,
    registrations: Vec<HookRegistration>,
}

impl HookRegistry {
    fn register(
        &mut self,
        ops: &dyn SharedHookOps,
        wake_window: NativeWindowId,
        owner_process_id: u32,
    ) -> Result<u64, ExternalMenuHookError> {
        self.register_for_owner_pids(ops, wake_window, &[owner_process_id])
    }

    fn register_for_owner_pids(
        &mut self,
        ops: &dyn SharedHookOps,
        wake_window: NativeWindowId,
        owner_process_ids: &[u32],
    ) -> Result<u64, ExternalMenuHookError> {
        let owner_process_ids = owner_process_ids
            .iter()
            .copied()
            .filter(|owner_process_id| *owner_process_id != 0)
            .collect::<Vec<_>>();
        if wake_window.value() == 0 || owner_process_ids.is_empty() {
            return Err(ExternalMenuHookError::InvalidRegistration);
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or(ExternalMenuHookError::HookUnavailable)?;
        if self.hook.is_none() {
            self.hook = Some(
                ops.install_hook()
                    .ok_or(ExternalMenuHookError::HookUnavailable)?,
            );
        }
        self.next_id = id;
        self.registrations.push(HookRegistration {
            id,
            wake_window,
            owner_process_ids: owner_process_ids.into_boxed_slice(),
        });
        Ok(id)
    }

    fn remove_registration(&mut self, id: u64) -> Option<isize> {
        let index = self
            .registrations
            .iter()
            .position(|registration| registration.id == id)?;
        self.registrations.remove(index);
        self.registrations
            .is_empty()
            .then(|| self.hook.take())
            .flatten()
    }

    fn platform_event(
        &self,
        event: u32,
        window: NativeWindowId,
        owner_process_id: u32,
        dismissed_by_pointer: bool,
    ) -> Option<PlatformEvent> {
        if window.value() == 0
            || owner_process_id == 0
            || !self
                .registrations
                .iter()
                .any(|registration| registration.matches_owner_process_id(owner_process_id))
        {
            return None;
        }
        match classify_event(event)? {
            ExternalMenuEventKind::Started => Some(PlatformEvent::ExternalMenuPopupStarted {
                window,
                owner_process_id,
            }),
            ExternalMenuEventKind::Ended => Some(PlatformEvent::ExternalMenuPopupEnded {
                window,
                owner_process_id,
                dismissed_by_pointer,
            }),
        }
    }

    fn wake_for_owner_pid(
        &self,
        owner_process_id: u32,
        mut wake: impl FnMut(NativeWindowId) -> bool,
    ) -> bool {
        for registration in &self.registrations {
            if registration.matches_owner_process_id(owner_process_id)
                && wake(registration.wake_window)
            {
                return true;
            }
        }
        for registration in &self.registrations {
            if !registration.matches_owner_process_id(owner_process_id)
                && wake(registration.wake_window)
            {
                return true;
            }
        }
        false
    }
}

impl HookRegistration {
    fn matches_owner_process_id(&self, owner_process_id: u32) -> bool {
        self.owner_process_ids.contains(&owner_process_id)
    }
}

const fn classify_event(event: u32) -> Option<ExternalMenuEventKind> {
    match event {
        EVENT_SYSTEM_MENUPOPUPSTART => Some(ExternalMenuEventKind::Started),
        EVENT_SYSTEM_MENUPOPUPEND => Some(ExternalMenuEventKind::Ended),
        _ => None,
    }
}

#[derive(Debug)]
pub(crate) struct ExternalMenuEventRegistration {
    id: u64,
}

impl ExternalMenuEventRegistration {
    pub(crate) fn register(
        wake_window: NativeWindowId,
        owner_process_id: u32,
    ) -> Result<Self, ExternalMenuHookError> {
        let id = shared_registry().register(&Win32SharedHookOps, wake_window, owner_process_id)?;
        Ok(Self { id })
    }

    pub(crate) fn register_for_owner_pids(
        wake_window: NativeWindowId,
        owner_process_ids: &[u32],
    ) -> Result<Self, ExternalMenuHookError> {
        let id = shared_registry().register_for_owner_pids(
            &Win32SharedHookOps,
            wake_window,
            owner_process_ids,
        )?;
        Ok(Self { id })
    }
}

impl Drop for ExternalMenuEventRegistration {
    fn drop(&mut self) {
        let hook = shared_registry().remove_registration(self.id);
        if let Some(hook) = hook {
            Win32SharedHookOps.uninstall_hook(hook);
        }
    }
}

struct Win32SharedHookOps;

impl SharedHookOps for Win32SharedHookOps {
    fn install_hook(&self) -> Option<isize> {
        // SAFETY: Category 8 (FFI boundary). The process-global, out-of-context
        // hook copies only scalar menu lifecycle data in `win_event_callback`.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_MENUPOPUPSTART,
                EVENT_SYSTEM_MENUPOPUPEND,
                None,
                Some(win_event_callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        (!hook.0.is_null()).then_some(hook.0 as isize)
    }

    fn uninstall_hook(&self, hook: isize) {
        // SAFETY: Category 8 (FFI boundary). The scalar token is returned by the
        // matching SetWinEventHook call and is consumed once by the last guard.
        let _ = unsafe { UnhookWinEvent(HWINEVENTHOOK(hook as *mut c_void)) };
    }
}

static HOOK_REGISTRY: OnceLock<Mutex<HookRegistry>> = OnceLock::new();

fn shared_registry() -> MutexGuard<'static, HookRegistry> {
    let registry = HOOK_REGISTRY.get_or_init(|| Mutex::new(HookRegistry::default()));
    match registry.lock() {
        Ok(registry) => registry,
        Err(poisoned) => poisoned.into_inner(),
    }
}

unsafe extern "system" fn win_event_callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _object_id: i32,
    _child_id: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    handle_win_event(event, hwnd);
}

fn handle_win_event(event: u32, hwnd: HWND) {
    let window = NativeWindowId::new(hwnd.0 as isize);
    if window.value() == 0 {
        return;
    }
    let Some(owner_process_id) = window_owner_process_id(hwnd) else {
        return;
    };
    let dismissed_by_pointer =
        event == EVENT_SYSTEM_MENUPOPUPEND && external_menu_pointer_button_is_down();
    let registry = shared_registry();
    let Some(event) =
        registry.platform_event(event, window, owner_process_id, dismissed_by_pointer)
    else {
        return;
    };
    let _ = queue_event_with_wake(RoutedPlatformEvent::broadcast(event), || {
        registry.wake_for_owner_pid(owner_process_id, post_wake)
    });
}

fn external_menu_pointer_button_is_down() -> bool {
    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2]
        .into_iter()
        .any(|button| {
            // SAFETY: Category 8 (FFI boundary). The query accepts a scalar
            // virtual-key value and does not retain or dereference memory.
            unsafe { GetAsyncKeyState(i32::from(button.0)) < 0 }
        })
}

fn window_owner_process_id(hwnd: HWND) -> Option<u32> {
    let mut process_id = 0_u32;
    // SAFETY: Category 8 (FFI boundary). The callback-provided HWND is used only
    // for this scalar ownership query, with writable local PID storage.
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    (thread_id != 0 && process_id != 0).then_some(process_id)
}

fn post_wake(window: NativeWindowId) -> bool {
    let hwnd = HWND(window.value() as *mut c_void);
    // SAFETY: Category 8 (FFI boundary). This private wake carries no payload and
    // PostMessageW returns immediately; stale HWND failures trigger another target.
    unsafe { PostMessageW(Some(hwnd), EXTERNAL_MENU_WAKE_MESSAGE, WPARAM(0), LPARAM(0)).is_ok() }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::{
        EVENT_SYSTEM_MENUPOPUPEND, EVENT_SYSTEM_MENUPOPUPSTART, ExternalMenuEventKind,
        HookRegistry, SharedHookOps, classify_event,
    };
    use crate::{NativeWindowId, PlatformEvent};

    #[derive(Default)]
    struct FakeSharedHookOps {
        installs: Cell<usize>,
        uninstalls: RefCell<Vec<isize>>,
    }

    impl SharedHookOps for FakeSharedHookOps {
        fn install_hook(&self) -> Option<isize> {
            self.installs.set(self.installs.get() + 1);
            Some(444)
        }

        fn uninstall_hook(&self, hook: isize) {
            self.uninstalls.borrow_mut().push(hook);
        }
    }

    #[test]
    fn classifies_only_menu_popup_start_and_end() {
        assert_eq!(
            classify_event(EVENT_SYSTEM_MENUPOPUPSTART),
            Some(ExternalMenuEventKind::Started)
        );
        assert_eq!(
            classify_event(EVENT_SYSTEM_MENUPOPUPEND),
            Some(ExternalMenuEventKind::Ended)
        );
        assert_eq!(classify_event(EVENT_SYSTEM_MENUPOPUPEND + 1), None);
    }

    #[test]
    fn filters_events_by_registered_owner_pid_and_keeps_only_scalar_data() {
        let ops = FakeSharedHookOps::default();
        let mut registry = HookRegistry::default();
        registry
            .register(&ops, NativeWindowId::new(11), 77)
            .expect("fake hook installs");
        let event_window = NativeWindowId::new(901);

        assert_eq!(
            registry.platform_event(EVENT_SYSTEM_MENUPOPUPSTART, event_window, 77, false),
            Some(PlatformEvent::ExternalMenuPopupStarted {
                window: event_window,
                owner_process_id: 77,
            })
        );
        assert_eq!(
            registry.platform_event(EVENT_SYSTEM_MENUPOPUPEND, event_window, 77, true),
            Some(PlatformEvent::ExternalMenuPopupEnded {
                window: event_window,
                owner_process_id: 77,
                dismissed_by_pointer: true,
            })
        );
        assert_eq!(
            registry.platform_event(EVENT_SYSTEM_MENUPOPUPSTART, event_window, 78, false),
            None
        );
    }

    #[test]
    fn accepts_any_registered_owner_pid_alias_for_shell_owned_popups() {
        let ops = FakeSharedHookOps::default();
        let mut registry = HookRegistry::default();
        registry
            .register_for_owner_pids(&ops, NativeWindowId::new(11), &[77, 88])
            .expect("fake hook installs");
        let event_window = NativeWindowId::new(902);

        assert_eq!(
            registry.platform_event(EVENT_SYSTEM_MENUPOPUPSTART, event_window, 88, false),
            Some(PlatformEvent::ExternalMenuPopupStarted {
                window: event_window,
                owner_process_id: 88,
            })
        );
    }

    #[test]
    fn shares_one_hook_until_the_last_registration_is_removed() {
        let ops = FakeSharedHookOps::default();
        let mut registry = HookRegistry::default();
        let first = registry
            .register(&ops, NativeWindowId::new(11), 77)
            .expect("first fake hook registration");
        let second = registry
            .register(&ops, NativeWindowId::new(22), 88)
            .expect("second fake hook registration");

        assert_eq!(ops.installs.get(), 1);
        assert_eq!(registry.remove_registration(first), None);
        assert!(ops.uninstalls.borrow().is_empty());
        let hook = registry
            .remove_registration(second)
            .expect("last registration owns hook teardown");
        ops.uninstall_hook(hook);
        assert_eq!(ops.uninstalls.borrow().as_slice(), [444]);
    }

    #[test]
    fn stale_matching_wake_window_falls_back_to_another_registration() {
        let ops = FakeSharedHookOps::default();
        let mut registry = HookRegistry::default();
        registry
            .register(&ops, NativeWindowId::new(11), 77)
            .expect("first fake hook registration");
        registry
            .register(&ops, NativeWindowId::new(22), 88)
            .expect("second fake hook registration");
        let attempts = RefCell::new(Vec::new());

        assert!(registry.wake_for_owner_pid(77, |window| {
            attempts.borrow_mut().push(window);
            window == NativeWindowId::new(22)
        }));
        assert_eq!(
            attempts.borrow().as_slice(),
            [NativeWindowId::new(11), NativeWindowId::new(22)]
        );
    }
}
