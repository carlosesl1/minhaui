use core::ffi::c_void;
use std::path::Path;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, EnumChildWindows, FindWindowW, GetClassNameW, GetCursorPos,
    GetWindowThreadProcessId, IsWindow, PostMessageW, SetCursorPos,
};
use windows::core::{BOOL, Error, Result as WindowsResult, w};

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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ShellAppTargetKind {
    NotificationIcon,
    TaskbarButton,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellAppTarget {
    name: String,
    kind: ShellAppTargetKind,
    point: TrayScreenPoint,
}

impl ShellAppTarget {
    #[cfg(test)]
    fn test(name: &str, kind: ShellAppTargetKind, point: (i32, i32)) -> Self {
        Self {
            name: name.to_owned(),
            kind,
            point: point.into(),
        }
    }
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
    let _owner_process_id = validate_native_tray_identity(ops, identity, current_generation)?;

    let request = TrayActivationRequest::new(
        executable,
        identity.owner_window(),
        identity.icon_id(),
        identity.callback_message(),
        identity.version(),
        point,
    );
    let mut sink = NativeTraySink { ops };
    Ok(coordinator.begin(request, &mut sink))
}

pub(crate) fn validate_native_tray_identity(
    ops: &dyn NativeTrayOps,
    identity: NativeTrayIdentity,
    current_generation: u64,
) -> Result<u32, NativeTrayActivationError> {
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
    Ok(owner_process_id)
}

pub(crate) fn shell_menu_owner_process_id() -> Option<u32> {
    // SAFETY: Looks up the system-owned shell tray window by a static class
    // name before querying its scalar owner PID.
    let shell_window = unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.ok()?;
    let mut process_id = 0_u32;
    // SAFETY: The shell HWND is used only for this synchronous ownership query.
    let thread_id = unsafe { GetWindowThreadProcessId(shell_window, Some(&mut process_id)) };
    (thread_id != 0 && process_id != 0).then_some(process_id)
}

pub(crate) fn activate_windows_shell_app_context_menu_if_current(
    label: &str,
    executable: &str,
    is_current: &dyn Fn() -> bool,
) -> WindowsResult<crate::shell_menu_worker::ShellMenuActivationOutcome> {
    use crate::shell_menu_worker::ShellMenuActivationOutcome;

    if !is_current() {
        return Ok(ShellMenuActivationOutcome::Superseded);
    }
    // SAFETY: The owned shell-menu worker initializes COM before this call.
    // UI Automation is used only to read Explorer-owned element metadata.
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }?;
    // SAFETY: Looks up a system-owned top-level window by a static class name.
    let shell_window = unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }?;
    let shell_root = shell_automation_root(shell_window);

    let taskbar_targets =
        collect_shell_targets(&automation, shell_root, ShellAppTargetKind::TaskbarButton)?;
    if let Some(target) = select_shell_app_target(&taskbar_targets, label, executable) {
        if !is_current() {
            return Ok(ShellMenuActivationOutcome::Superseded);
        }
        record_shell_menu_target("taskbar", target.point);
        return right_click_shell_target(target.point)
            .map(|()| ShellMenuActivationOutcome::Activated);
    }

    let visible_notification_targets = collect_shell_targets(
        &automation,
        shell_root,
        ShellAppTargetKind::NotificationIcon,
    )?;
    if let Some(target) = select_shell_app_target(&visible_notification_targets, label, executable)
    {
        if !is_current() {
            return Ok(ShellMenuActivationOutcome::Superseded);
        }
        record_shell_menu_target("notification", target.point);
        return right_click_shell_target(target.point)
            .map(|()| ShellMenuActivationOutcome::Activated);
    }

    Ok(ShellMenuActivationOutcome::Unavailable)
}

fn record_shell_menu_target(kind: &'static str, point: TrayScreenPoint) {
    if !crate::diagnostics::enabled(crate::diagnostics::DiagnosticModule::AppLifecycle) {
        return;
    }
    let x = point.x().to_string();
    let y = point.y().to_string();
    crate::diagnostics::record(
        crate::diagnostics::DiagnosticModule::AppLifecycle,
        crate::diagnostics::LogLevel::Info,
        "background_app_shell_menu.target",
        &[("kind", kind), ("x", &x), ("y", &y)],
    );
}

fn shell_automation_root(shell_window: HWND) -> HWND {
    shell_automation_root_with(shell_window, find_shell_xaml_host)
}

fn shell_automation_root_with(
    shell_window: HWND,
    find_xaml_host: impl FnOnce(HWND) -> Option<HWND>,
) -> HWND {
    find_xaml_host(shell_window).unwrap_or(shell_window)
}

fn find_shell_xaml_host(shell_window: HWND) -> Option<HWND> {
    let mut search = ShellXamlHostSearch::default();
    // SAFETY: The search storage remains live for the complete synchronous
    // enumeration and the callback restores this exact pointer type.
    let _ = unsafe {
        EnumChildWindows(
            Some(shell_window),
            Some(enum_shell_xaml_host),
            LPARAM(&raw mut search as isize),
        )
    };
    search.window
}

#[derive(Default)]
struct ShellXamlHostSearch {
    window: Option<HWND>,
}

unsafe extern "system" fn enum_shell_xaml_host(window: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: The parameter points to the live stack search owned by the
    // synchronous EnumChildWindows call above.
    let search = unsafe { &mut *(parameter.0 as *mut ShellXamlHostSearch) };
    let mut class_name = [0_u16; 64];
    // SAFETY: The child HWND came from EnumChildWindows and the fixed buffer is
    // valid writable storage for the duration of this call.
    let copied = unsafe { GetClassNameW(window, &mut class_name) };
    let class_name = String::from_utf16_lossy(&class_name[..copied.max(0) as usize]);
    if class_name == "Windows.UI.Composition.DesktopWindowContentBridge" {
        search.window = Some(window);
        return false.into();
    }
    true.into()
}

fn collect_shell_targets(
    automation: &IUIAutomation,
    window: HWND,
    kind: ShellAppTargetKind,
) -> WindowsResult<Vec<ShellAppTarget>> {
    // SAFETY: The HWND is a live Explorer top-level window resolved immediately
    // before this read, and every COM object remains scoped to this call.
    let root = unsafe { automation.ElementFromHandle(window) }?;
    // SAFETY: Creates a condition owned by this UI Automation instance.
    let condition = unsafe { automation.CreateTrueCondition() }?;
    // SAFETY: Reads descendants from the live UI Automation element.
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &condition) }?;
    // SAFETY: Reads the size of the UI Automation array returned above.
    let length = unsafe { elements.Length() }?.clamp(0, 512);
    let mut targets = Vec::new();
    for index in 0..length {
        // SAFETY: Index is bounded by the array length returned above.
        let Ok(element) = (unsafe { elements.GetElement(index) }) else {
            continue;
        };
        // SAFETY: Reads an immutable property from the live UI Automation element.
        let Ok(name) = (unsafe { element.CurrentName() }) else {
            continue;
        };
        let name = name.to_string();
        if name.trim().is_empty() || !element_matches_kind(&element, kind) {
            continue;
        }
        // SAFETY: Reads an immutable property from the live UI Automation element.
        let Ok(rect) = (unsafe { element.CurrentBoundingRectangle() }) else {
            continue;
        };
        if rect.right <= rect.left || rect.bottom <= rect.top {
            continue;
        }
        targets.push(ShellAppTarget {
            name,
            kind,
            point: TrayScreenPoint::new(
                rect.left + (rect.right - rect.left) / 2,
                rect.top + (rect.bottom - rect.top) / 2,
            ),
        });
    }
    Ok(targets)
}

fn element_matches_kind(element: &IUIAutomationElement, kind: ShellAppTargetKind) -> bool {
    // SAFETY: Reads an immutable property from the live UI Automation element.
    let class_name = unsafe { element.CurrentClassName() }
        .map(|value| value.to_string())
        .unwrap_or_default();
    // SAFETY: Reads an immutable property from the live UI Automation element.
    let automation_id = unsafe { element.CurrentAutomationId() }
        .map(|value| value.to_string())
        .unwrap_or_default();
    match kind {
        ShellAppTargetKind::TaskbarButton => class_name == "Taskbar.TaskListButtonAutomationPeer",
        ShellAppTargetKind::NotificationIcon => automation_id == "NotifyItemIcon",
    }
}

fn right_click_shell_target(point: TrayScreenPoint) -> WindowsResult<()> {
    let mut previous = POINT::default();
    // SAFETY: Reads and restores the scalar cursor position and injects exactly
    // one user-requested right-button press/release at the verified shell icon.
    unsafe { GetCursorPos(&mut previous) }?;
    // SAFETY: Moves the cursor to the verified shell element center.
    unsafe { SetCursorPos(point.x(), point.y()) }?;
    let inputs = [
        mouse_input(MOUSEEVENTF_RIGHTDOWN),
        mouse_input(MOUSEEVENTF_RIGHTUP),
    ];
    // SAFETY: Sends two fully initialized mouse inputs with the correct element size.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    thread::sleep(Duration::from_millis(25));
    // SAFETY: Restores the cursor position captured immediately before injection.
    let restore = unsafe { SetCursorPos(previous.x, previous.y) };
    if sent != inputs.len() as u32 {
        let _ = restore;
        return Err(Error::new(
            E_FAIL,
            "Windows shell right-click injection failed",
        ));
    }
    restore
}

fn mouse_input(flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..MOUSEINPUT::default()
            },
        },
    }
}

fn select_shell_app_target<'a>(
    targets: &'a [ShellAppTarget],
    label: &str,
    executable: &str,
) -> Option<&'a ShellAppTarget> {
    let mut aliases = shell_name_tokens(label);
    if let Some(stem) = Path::new(executable)
        .file_stem()
        .and_then(|stem| stem.to_str())
    {
        aliases.extend(shell_name_tokens(stem));
    }
    aliases.sort();
    aliases.dedup();
    targets
        .iter()
        .filter_map(|target| {
            let score = shell_name_match_score(&aliases, &shell_name_tokens(&target.name));
            (score >= 16).then_some((target.kind, score, target))
        })
        .max_by_key(|(kind, score, _)| (*kind, *score))
        .map(|(_, _, target)| target)
}

fn shell_name_match_score(aliases: &[String], target: &[String]) -> usize {
    aliases
        .iter()
        .filter(|alias| target.contains(alias))
        .map(|alias| alias.len().saturating_mul(alias.len()))
        .sum()
}

fn shell_name_tokens(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut previous_was_lowercase = false;
    for character in value.chars() {
        if character.is_uppercase() && previous_was_lowercase && !word.is_empty() {
            words.push(canonical_shell_word(&word));
            word.clear();
        }
        if let Some(character) = fold_shell_character(character) {
            word.push(character);
            previous_was_lowercase = character.is_ascii_lowercase();
        } else {
            if !word.is_empty() {
                words.push(canonical_shell_word(&word));
                word.clear();
            }
            previous_was_lowercase = false;
        }
    }
    if !word.is_empty() {
        words.push(canonical_shell_word(&word));
    }
    words
}

fn canonical_shell_word(word: &str) -> String {
    match word.as_bytes() {
        b"explorador" => "explorer".to_owned(),
        b"seguranca" => "security".to_owned(),
        _ => word.to_owned(),
    }
}

const fn fold_shell_character(character: char) -> Option<char> {
    match character {
        'a'..='z' | '0'..='9' => Some(character),
        'A'..='Z' => Some(character.to_ascii_lowercase()),
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => Some('a'),
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => Some('e'),
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => Some('i'),
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => Some('o'),
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => Some('u'),
        'ç' | 'Ç' => Some('c'),
        _ => None,
    }
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

    use super::{
        NativeTrayActivationError, NativeTrayOps, ShellAppTarget, ShellAppTargetKind,
        activate_native_tray_context_menu, select_shell_app_target, shell_automation_root_with,
    };
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
        assert_eq!(result.expected_count(), 2);
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
        assert_eq!(result.delivered_count(), 2);
        assert_eq!(result.expected_count(), 2);
        assert!(result.is_pending());
        assert_eq!(
            ops.calls.borrow().as_slice(),
            [
                NativeCall::IsWindow(OWNER_WINDOW),
                NativeCall::OwnerProcessId(OWNER_WINDOW),
                NativeCall::AllowForeground(OWNER_PROCESS_ID),
                NativeCall::PostMessage(
                    OWNER_WINDOW,
                    TrayMessage::modern_pointer_with_callback(0x8061, 7, (10, 20), WM_RBUTTONDOWN,),
                ),
                NativeCall::PostMessage(
                    OWNER_WINDOW,
                    TrayMessage::modern_pointer_with_callback(0x8061, 7, (10, 20), WM_RBUTTONUP,),
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

    #[test]
    fn shell_target_prefers_the_real_taskbar_or_notification_icon() {
        let targets = [
            ShellAppTarget::test("ChatGPT", ShellAppTargetKind::NotificationIcon, (10, 10)),
            ShellAppTarget::test(
                "ChatGPT - 1 janela em execução fixado",
                ShellAppTargetKind::TaskbarButton,
                (20, 20),
            ),
            ShellAppTarget::test(
                "AMD Software: Adrenalin Edition",
                ShellAppTargetKind::NotificationIcon,
                (30, 30),
            ),
            ShellAppTarget::test(
                "Segurança do Windows - Nenhuma ação necessária.",
                ShellAppTargetKind::NotificationIcon,
                (40, 40),
            ),
        ];

        assert_eq!(
            select_shell_app_target(&targets, "ChatGPT", "ChatGPT.exe").map(|target| target.point),
            Some(TrayScreenPoint::new(20, 20))
        );
        assert_eq!(
            select_shell_app_target(&targets, "RadeonSoftware", "RadeonSoftware.exe")
                .map(|target| target.point),
            Some(TrayScreenPoint::new(30, 30))
        );
        assert_eq!(
            select_shell_app_target(
                &targets,
                "SecurityHealthSystray",
                "SecurityHealthSystray.exe",
            )
            .map(|target| target.point),
            Some(TrayScreenPoint::new(40, 40))
        );
    }

    #[test]
    fn windows_11_xaml_host_is_preferred_as_the_shell_automation_root() {
        use windows::Win32::Foundation::HWND;

        let shell = HWND(0x10_usize as *mut core::ffi::c_void);
        let xaml = HWND(0x20_usize as *mut core::ffi::c_void);

        assert_eq!(shell_automation_root_with(shell, |_| Some(xaml)), xaml);
        assert_eq!(shell_automation_root_with(shell, |_| None), shell);
    }

    #[test]
    fn shell_fallback_never_opens_the_hidden_notification_overflow() {
        let source = include_str!("win32_tray_activation.rs");
        let forbidden_call = ["open_notification", "_overflow("].concat();

        assert!(
            !source.contains(&forbidden_call),
            "opening Explorer's hidden-icons surface necessarily composes a visible frame"
        );
    }
}
