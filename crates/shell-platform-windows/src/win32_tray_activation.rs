use core::ffi::c_void;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, GetWindowThreadProcessId, IsWindow, PostMessageW,
};

use crate::native_tray::NativeTrayIdentity;
use crate::{
    NativeWindowId, TrayActivationCoordinator, TrayActivationRequest, TrayActivationResult,
    TrayCallbackSink, TrayMessage, TrayScreenPoint,
};

pub(crate) trait NativeTrayOps {
    fn is_window(&self, window: NativeWindowId) -> bool;
    fn owner_process_id(&self, window: NativeWindowId) -> Option<u32>;
    fn allow_foreground(&self, process_id: u32) -> bool;
    fn post_message(&self, window: NativeWindowId, message: TrayMessage) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum NativeTrayActivationError {
    #[error("tray observation generation is stale")]
    StaleGeneration,
    #[error("tray owner window is unavailable")]
    InvalidOwnerWindow,
    #[error("tray owner process is unavailable")]
    OwnerProcessUnavailable,
    #[error("tray owner process changed")]
    OwnerProcessMismatch,
    #[error("tray owner cannot be permitted to take foreground")]
    ForegroundPermissionDenied,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Win32NativeTrayOps;

impl NativeTrayOps for Win32NativeTrayOps {
    fn is_window(&self, window: NativeWindowId) -> bool {
        // SAFETY: Category 8 (FFI boundary). The scalar observation is converted
        // back to an HWND only for this documented validity probe.
        unsafe { IsWindow(Some(hwnd(window))).as_bool() }
    }

    fn owner_process_id(&self, window: NativeWindowId) -> Option<u32> {
        let mut process_id = 0_u32;
        // SAFETY: Category 8 (FFI boundary). `process_id` is writable for the
        // synchronous ownership query and no application memory is dereferenced.
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd(window), Some(&mut process_id)) };
        (thread_id != 0 && process_id != 0).then_some(process_id)
    }

    fn allow_foreground(&self, process_id: u32) -> bool {
        // SAFETY: Category 8 (FFI boundary). This grants the already revalidated
        // owner PID permission to participate in Win32 foreground activation.
        unsafe { AllowSetForegroundWindow(process_id).is_ok() }
    }

    fn post_message(&self, window: NativeWindowId, message: TrayMessage) -> bool {
        // SAFETY: Category 8 (FFI boundary). Only the captured scalar callback
        // envelope is posted; PostMessageW never waits for the application owner.
        unsafe {
            PostMessageW(
                Some(hwnd(window)),
                message.message(),
                WPARAM(message.wparam()),
                LPARAM(message.lparam()),
            )
            .is_ok()
        }
    }
}

pub(crate) fn activate_native_tray_context_menu(
    coordinator: &mut TrayActivationCoordinator,
    ops: &dyn NativeTrayOps,
    identity: NativeTrayIdentity,
    current_generation: u64,
    executable: &str,
    point: TrayScreenPoint,
) -> Result<TrayActivationResult, NativeTrayActivationError> {
    if !identity.is_current(current_generation) {
        return Err(NativeTrayActivationError::StaleGeneration);
    }
    let owner_window = identity.owner_window();
    if !ops.is_window(owner_window) {
        return Err(NativeTrayActivationError::InvalidOwnerWindow);
    }
    let owner_process_id = ops
        .owner_process_id(owner_window)
        .ok_or(NativeTrayActivationError::OwnerProcessUnavailable)?;
    if owner_process_id != identity.owner_process_id() {
        return Err(NativeTrayActivationError::OwnerProcessMismatch);
    }
    if !ops.allow_foreground(owner_process_id) {
        return Err(NativeTrayActivationError::ForegroundPermissionDenied);
    }

    let request = TrayActivationRequest::new(
        executable,
        owner_window,
        identity.icon_id(),
        identity.callback_message(),
        identity.version(),
        point,
    );
    let mut sink = NativeTraySink { ops };
    Ok(coordinator.begin(request, &mut sink))
}

struct NativeTraySink<'a> {
    ops: &'a dyn NativeTrayOps,
}

impl TrayCallbackSink for NativeTraySink<'_> {
    fn post(&mut self, target: NativeWindowId, message: TrayMessage) -> bool {
        self.ops.post_message(target, message)
    }
}

fn hwnd(window: NativeWindowId) -> HWND {
    HWND(window.value() as *mut c_void)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::{NativeTrayActivationError, NativeTrayOps, activate_native_tray_context_menu};
    use crate::native_tray::NativeTrayIdentity;
    use crate::{
        NativeWindowId, TrayActivationCoordinator, TrayActivationStatus, TrayMessage,
        TrayScreenPoint, WM_RBUTTONDOWN, WM_RBUTTONUP,
    };

    const OWNER_WINDOW: NativeWindowId = NativeWindowId::new(41);
    const OWNER_PROCESS_ID: u32 = 9001;
    const GENERATION: u64 = 12;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum NativeCall {
        IsWindow(NativeWindowId),
        OwnerProcessId(NativeWindowId),
        AllowForeground(u32),
        PostMessage(NativeWindowId, TrayMessage),
    }

    struct FakeNativeTrayOps {
        is_window: bool,
        owner_process_id: Option<u32>,
        allow_foreground: bool,
        post_results: RefCell<VecDeque<bool>>,
        calls: RefCell<Vec<NativeCall>>,
    }

    impl Default for FakeNativeTrayOps {
        fn default() -> Self {
            Self {
                is_window: true,
                owner_process_id: Some(OWNER_PROCESS_ID),
                allow_foreground: true,
                post_results: RefCell::new(VecDeque::new()),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl NativeTrayOps for FakeNativeTrayOps {
        fn is_window(&self, window: NativeWindowId) -> bool {
            self.calls.borrow_mut().push(NativeCall::IsWindow(window));
            self.is_window
        }

        fn owner_process_id(&self, window: NativeWindowId) -> Option<u32> {
            self.calls
                .borrow_mut()
                .push(NativeCall::OwnerProcessId(window));
            self.owner_process_id
        }

        fn allow_foreground(&self, process_id: u32) -> bool {
            self.calls
                .borrow_mut()
                .push(NativeCall::AllowForeground(process_id));
            self.allow_foreground
        }

        fn post_message(&self, window: NativeWindowId, message: TrayMessage) -> bool {
            self.calls
                .borrow_mut()
                .push(NativeCall::PostMessage(window, message));
            self.post_results.borrow_mut().pop_front().unwrap_or(true)
        }
    }

    fn identity() -> NativeTrayIdentity {
        identity_with_version(4)
    }

    fn identity_with_version(version: u32) -> NativeTrayIdentity {
        NativeTrayIdentity::new(
            OWNER_WINDOW,
            OWNER_PROCESS_ID,
            7,
            0x8061,
            version,
            None,
            GENERATION,
        )
        .expect("test identity is valid")
    }

    #[test]
    fn generation_mismatch_rejects_before_any_native_call() {
        let ops = FakeNativeTrayOps::default();
        let mut coordinator = TrayActivationCoordinator::new();

        let result = activate_native_tray_context_menu(
            &mut coordinator,
            &ops,
            identity(),
            GENERATION + 1,
            "test.exe",
            TrayScreenPoint::new(10, 20),
        );

        assert_eq!(result, Err(NativeTrayActivationError::StaleGeneration));
        assert!(ops.calls.borrow().is_empty());
    }

    #[test]
    fn invalid_owner_window_rejects_before_pid_or_foreground_calls() {
        let ops = FakeNativeTrayOps {
            is_window: false,
            ..Default::default()
        };
        let mut coordinator = TrayActivationCoordinator::new();

        let result = activate_native_tray_context_menu(
            &mut coordinator,
            &ops,
            identity(),
            GENERATION,
            "test.exe",
            TrayScreenPoint::new(10, 20),
        );

        assert_eq!(result, Err(NativeTrayActivationError::InvalidOwnerWindow));
        assert_eq!(
            ops.calls.borrow().as_slice(),
            [NativeCall::IsWindow(OWNER_WINDOW)]
        );
    }

    #[test]
    fn reused_owner_window_with_a_different_pid_is_rejected() {
        let ops = FakeNativeTrayOps {
            owner_process_id: Some(OWNER_PROCESS_ID + 1),
            ..Default::default()
        };
        let mut coordinator = TrayActivationCoordinator::new();

        let result = activate_native_tray_context_menu(
            &mut coordinator,
            &ops,
            identity(),
            GENERATION,
            "test.exe",
            TrayScreenPoint::new(10, 20),
        );

        assert_eq!(result, Err(NativeTrayActivationError::OwnerProcessMismatch));
        assert_eq!(
            ops.calls.borrow().as_slice(),
            [
                NativeCall::IsWindow(OWNER_WINDOW),
                NativeCall::OwnerProcessId(OWNER_WINDOW),
            ]
        );
    }

    #[test]
    fn foreground_permission_failure_is_reported_without_posting_or_arming() {
        let ops = FakeNativeTrayOps {
            allow_foreground: false,
            ..Default::default()
        };
        let mut coordinator = TrayActivationCoordinator::new();

        let result = activate_native_tray_context_menu(
            &mut coordinator,
            &ops,
            identity(),
            GENERATION,
            "test.exe",
            TrayScreenPoint::new(10, 20),
        );

        assert_eq!(
            result,
            Err(NativeTrayActivationError::ForegroundPermissionDenied)
        );
        assert_eq!(
            ops.calls.borrow().as_slice(),
            [
                NativeCall::IsWindow(OWNER_WINDOW),
                NativeCall::OwnerProcessId(OWNER_WINDOW),
                NativeCall::AllowForeground(OWNER_PROCESS_ID),
            ]
        );
        assert_eq!(coordinator.pending_id(), None);
    }

    #[test]
    fn post_failure_is_reported_without_a_blocking_or_alternate_delivery() {
        let ops = FakeNativeTrayOps {
            post_results: RefCell::new([false].into_iter().collect()),
            ..Default::default()
        };
        let mut coordinator = TrayActivationCoordinator::new();

        let result = activate_native_tray_context_menu(
            &mut coordinator,
            &ops,
            identity(),
            GENERATION,
            "test.exe",
            TrayScreenPoint::new(10, 20),
        )
        .expect("native validation succeeds");

        assert_eq!(result.status(), TrayActivationStatus::PostFailed);
        assert!(result.id().is_some());
        assert_eq!(result.delivered_count(), 0);
        assert_eq!(result.expected_count(), 1);
        assert!(!result.is_pending());
        assert_eq!(coordinator.pending_id(), None);
        assert_eq!(ops.calls.borrow().len(), 4);
        assert!(matches!(
            ops.calls.borrow().last(),
            Some(NativeCall::PostMessage(OWNER_WINDOW, _))
        ));
    }

    #[test]
    fn foreground_is_allowed_before_posts_and_result_exposes_delivery_token() {
        let ops = FakeNativeTrayOps::default();
        let mut coordinator = TrayActivationCoordinator::new();

        let result = activate_native_tray_context_menu(
            &mut coordinator,
            &ops,
            identity(),
            GENERATION,
            "test.exe",
            TrayScreenPoint::new(10, 20),
        )
        .expect("native validation succeeds");

        assert_eq!(result.status(), TrayActivationStatus::Posted);
        assert!(result.id().is_some());
        assert_eq!(result.delivered_count(), 1);
        assert_eq!(result.expected_count(), 1);
        assert!(result.is_pending());
        assert_eq!(
            ops.calls.borrow().as_slice(),
            [
                NativeCall::IsWindow(OWNER_WINDOW),
                NativeCall::OwnerProcessId(OWNER_WINDOW),
                NativeCall::AllowForeground(OWNER_PROCESS_ID),
                NativeCall::PostMessage(
                    OWNER_WINDOW,
                    TrayMessage::modern_context_with_callback(0x8061, 7, (10, 20)),
                ),
            ]
        );
    }

    #[test]
    fn every_legacy_callback_delivery_uses_non_blocking_post_message() {
        let ops = FakeNativeTrayOps::default();
        let mut coordinator = TrayActivationCoordinator::new();

        let result = activate_native_tray_context_menu(
            &mut coordinator,
            &ops,
            identity_with_version(3),
            GENERATION,
            "legacy.exe",
            TrayScreenPoint::new(10, 20),
        )
        .expect("native validation succeeds");

        assert_eq!(result.status(), TrayActivationStatus::Posted);
        assert_eq!(result.delivered_count(), 2);
        assert_eq!(
            ops.calls.borrow().as_slice(),
            [
                NativeCall::IsWindow(OWNER_WINDOW),
                NativeCall::OwnerProcessId(OWNER_WINDOW),
                NativeCall::AllowForeground(OWNER_PROCESS_ID),
                NativeCall::PostMessage(
                    OWNER_WINDOW,
                    TrayMessage::legacy_with_callback(0x8061, 7, WM_RBUTTONDOWN),
                ),
                NativeCall::PostMessage(
                    OWNER_WINDOW,
                    TrayMessage::legacy_with_callback(0x8061, 7, WM_RBUTTONUP),
                ),
            ]
        );
    }
}
