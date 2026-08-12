use std::sync::atomic::{AtomicI32, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use std::{ffi::c_void, path::PathBuf};
use std::{io::Write, thread::Builder};

use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, RegisterWindowMessageW,
    SMTO_ABORTIFHUNG, SMTO_BLOCK, SMTO_ERRORONEXIT, SendMessageTimeoutW,
};
use windows::core::{Result, w};

use crate::PlatformEvent;
use crate::win32_shell_observation::ShellObservationRuntime;
use crate::win32_slots::{
    SlotFeatures, collect_dock_frame_waitables, create_slots, dispatch_event,
    handle_broadcast_event, print_monitor_placements,
};
use crate::win32_taskbar_visibility::ExplorerTaskbarVisibilityGuard;
use crate::win32_timer::TimerGuard;
use crate::win32_window::{CLASS_NAME, SETTINGS_TITLE, WindowClass};
use crate::win32_windowing::{MessageLoopAction, message_loop, monitor_placement_inputs};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShowcaseRunConfig {
    pub force_warp: bool,
    pub qa_exit_ms: Option<u32>,
    pub simulate_device_loss_once: bool,
    pub simulate_lifecycle_events: bool,
    pub safe_mode: bool,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub liquid_glass: bool,
    pub watchdog_heartbeat: bool,
}

pub(super) const TIMER_ID: usize = 0x4D55;
pub(super) const SYNC_TIMER_ID: usize = 0x4D56;
pub(super) const DOCK_ANIMATION_TIMER_ID: usize = 0x4D57;
pub(super) const PREVIEW_TIMER_ID: usize = 0x4D58;
pub(super) const DRAG_ESCAPE_TIMER_ID: usize = 0x4D59;
pub(super) const DOCK_EDGE_PROBE_TIMER_ID: usize = 0x4D5A;
pub(super) const EXTERNAL_MENU_TIMER_ID: usize = 0x4D5B;
pub(super) static LIVE_WINDOWS: AtomicI32 = AtomicI32::new(0);
pub(super) static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
pub(super) static ACTIVATE_INSTANCE: AtomicU32 = AtomicU32::new(0);
pub(super) static DOCK_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub(super) static TOPBAR_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub(super) static POPOVER_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub(super) static APP_MENU_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub(super) static PREVIEW_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub(super) static SETTINGS_WINDOW: AtomicIsize = AtomicIsize::new(0);
static DOCK_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();
static TOPBAR_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();
static POPOVER_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();
static APP_MENU_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();
static PREVIEW_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();
static SETTINGS_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();

/// Reveals the Settings window owned by the already-running per-user instance.
pub fn activate_existing_instance() -> Result<()> {
    // SAFETY: Category 8 (FFI boundary). Registration uses a static protocol
    // name and returns the same message identifier in every desktop process.
    let message = unsafe { RegisterWindowMessageW(w!("MinhaUi.ActivateInstance.v1")) };
    if message == 0 {
        return Err(windows::core::Error::from_thread());
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // SAFETY: Category 8 (FFI boundary). Both class and title are static,
        // null-terminated UTF-16 strings shared with the registered owner window.
        if let Ok(hwnd) = unsafe { FindWindowW(CLASS_NAME, SETTINGS_TITLE) } {
            let mut process_id = 0;
            // SAFETY: Category 8 (FFI boundary). The OS writes one process ID to
            // live stack storage for the HWND it just returned.
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
            if process_id != 0 {
                // SAFETY: Category 8 (FFI boundary). The secondary process was
                // user-launched and delegates its foreground right to the primary.
                let _ = unsafe { AllowSetForegroundWindow(process_id) };
            }
            let mut delivered = 0;
            // SAFETY: Category 8 (FFI boundary). The registered message carries
            // no pointers. The bounded timeout prevents a hung primary blocking
            // the secondary process indefinitely.
            let result = unsafe {
                SendMessageTimeoutW(
                    hwnd,
                    message,
                    WPARAM(0),
                    LPARAM(0),
                    SMTO_ABORTIFHUNG | SMTO_BLOCK | SMTO_ERRORONEXIT,
                    2_000,
                    Some(&mut delivered),
                )
            };
            if result.0 == 0 || delivered != 1 {
                return Err(windows::core::Error::new(
                    E_FAIL,
                    "the primary Minha UI instance did not acknowledge activation",
                ));
            }
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(windows::core::Error::new(
                E_FAIL,
                "the primary Minha UI instance owns the lock but has no Settings window",
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Resolves Local AppData through the Windows known-folder API.
pub fn local_app_data_path() -> Result<PathBuf> {
    // SAFETY: Category 8 (FFI boundary). The API allocates a null-terminated
    // string for the current user and transfers ownership to the caller.
    let raw = unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None) }?;
    // SAFETY: Category 8 (FFI boundary). `raw` is a live null-terminated string
    // returned by SHGetKnownFolderPath above.
    let value = unsafe { raw.to_string() };
    // SAFETY: Category 8 (FFI boundary). Balance the successful known-folder
    // allocation exactly once after copying the UTF-16 value.
    unsafe { CoTaskMemFree(Some(raw.0.cast::<c_void>())) };
    value
        .map(PathBuf::from)
        .map_err(|_| windows::core::Error::new(E_FAIL, "Local AppData contains invalid UTF-16"))
}

/// Returns the current Windows terminal-services session identifier.
pub fn current_session_id() -> Result<u32> {
    let mut session_id = 0;
    // SAFETY: Category 8 (FFI boundary). The current process ID is valid and the
    // API writes exactly one u32 to live stack storage.
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session_id) }?;
    Ok(session_id)
}

pub fn run_showcase(config: ShowcaseRunConfig) -> Result<()> {
    let safe_mode = config.safe_mode.to_string();
    let high_contrast = config.high_contrast.to_string();
    let reduced_motion = config.reduced_motion.to_string();
    crate::diagnostics::record(
        crate::diagnostics::DiagnosticModule::AppLifecycle,
        crate::diagnostics::LogLevel::Info,
        "app.started",
        &[
            ("safe_mode", &safe_mode),
            ("high_contrast", &high_contrast),
            ("reduced_motion", &reduced_motion),
        ],
    );
    let result = run_showcase_runtime(config);
    match &result {
        Ok(()) => crate::diagnostics::record(
            crate::diagnostics::DiagnosticModule::AppLifecycle,
            crate::diagnostics::LogLevel::Info,
            "app.stopped",
            &[],
        ),
        Err(error) => {
            let message = error.to_string();
            crate::diagnostics::record(
                crate::diagnostics::DiagnosticModule::AppLifecycle,
                crate::diagnostics::LogLevel::Error,
                "app.runtime.failed",
                &[("error", &message)],
            );
        }
    }
    crate::diagnostics::shutdown();
    result
}

fn run_showcase_runtime(config: ShowcaseRunConfig) -> Result<()> {
    let _com = ComApartment::initialize()?;
    // SAFETY: Category 8 (FFI boundary). DPI awareness is set before any HWND is
    // created and uses the documented process-wide per-monitor-v2 constant.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }?;
    let class = WindowClass::register()?;
    // SAFETY: Category 8 (FFI boundary). The registered message name is a static,
    // null-terminated UTF-16 string and the returned identifier is process-global.
    let taskbar_message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    TASKBAR_CREATED.store(taskbar_message, Ordering::Release);
    // SAFETY: Category 8 (FFI boundary). The activation protocol name is static
    // and process-global; secondary instances register the identical name.
    let activate_message = unsafe { RegisterWindowMessageW(w!("MinhaUi.ActivateInstance.v1")) };
    if activate_message == 0 {
        return Err(windows::core::Error::from_thread());
    }
    ACTIVATE_INSTANCE.store(activate_message, Ordering::Release);
    let monitors = monitor_placement_inputs()?;
    if monitors.is_empty() {
        return Err(windows::core::Error::new(
            invalid_arg(),
            "no display monitors were enumerated",
        ));
    }
    let mut shell_config = crate::win32_config::load_config();
    let explicit_taskbar_opt_in = std::env::var_os("MINHA_UI_UNSAFE_TASKBAR_REPLACEMENT")
        .is_some_and(|value| value == "I_ACCEPT_NO_NATIVE_RECOVERY");
    let mut explorer_taskbars = taskbar_replacement_enabled(
        shell_config.taskbar_policy(),
        config.safe_mode,
        explicit_taskbar_opt_in,
    )
    .then(ExplorerTaskbarVisibilityGuard::prepare_work_area)
    .transpose()?;
    print_monitor_placements(&monitors, &shell_config);
    let liquid_glass = config.liquid_glass && !config.safe_mode && !config.high_contrast;
    println!(
        "ACCESSIBILITY safe_mode={} high_contrast={} reduced_motion={} liquid_glass={}",
        config.safe_mode, config.high_contrast, config.reduced_motion, liquid_glass
    );
    let backdrop_enabled = !config.safe_mode
        && !config.high_contrast
        && std::env::var_os("MINHA_UI_DISABLE_BACKDROP").is_none();
    let features = SlotFeatures {
        force_warp: config.force_warp,
        solid_material: solid_material_for_accessibility(config.safe_mode, config.high_contrast),
        backdrop_enabled,
        reduced_motion: config.reduced_motion,
        liquid_glass,
    };
    let mut observation_runtime =
        ShellObservationRuntime::new(1_000, crate::win32_owner::now_ms())?;
    let initial_observation = observation_runtime.current();
    let mut slots = create_slots(
        &class,
        &monitors,
        features,
        &shell_config,
        initial_observation,
    )?;
    let heartbeat = WatchdogHeartbeat::new(config.watchdog_heartbeat);
    let timer = config
        .qa_exit_ms
        .map(|milliseconds| TimerGuard::start(slots[0].topbar_hwnd(), milliseconds))
        .transpose()?;
    let dock_handles = slots
        .iter()
        .map(|slot| slot.dock_hwnd())
        .collect::<Vec<_>>();
    if let Some(taskbars) = explorer_taskbars.as_mut() {
        taskbars.reconcile_and_hide(&dock_handles)?;
    }
    for slot in &mut slots {
        slot.show_shells();
        slot.print_windows();
    }
    heartbeat.pulse();
    if config.simulate_device_loss_once {
        for slot in &mut slots {
            slot.handle_event(PlatformEvent::DeviceLost)?;
        }
    }
    if config.simulate_lifecycle_events {
        for event in [
            PlatformEvent::DisplayChanged,
            PlatformEvent::PowerResumed,
            PlatformEvent::TaskbarCreated,
        ] {
            let taskbar_layout_changed = matches!(
                event,
                PlatformEvent::DisplayChanged | PlatformEvent::TaskbarCreated
            );
            handle_broadcast_event(
                &class,
                features,
                &shell_config,
                initial_observation,
                &mut slots,
                event,
            )?;
            if taskbar_layout_changed {
                let dock_handles = slots
                    .iter()
                    .map(|slot| slot.dock_hwnd())
                    .collect::<Vec<_>>();
                if let Some(taskbars) = explorer_taskbars.as_mut() {
                    taskbars.reconcile_and_hide(&dock_handles)?;
                }
            }
        }
    }
    let result = message_loop(|action| match action {
        MessageLoopAction::CollectDockFrameWaitables(waitables) => {
            collect_dock_frame_waitables(&slots, waitables);
            Ok(true)
        }
        MessageLoopAction::Dispatch(event) => {
            if matches!(event.event(), PlatformEvent::SyncWindows) {
                heartbeat.pulse();
            }
            if let PlatformEvent::SettingsConfigCommitted(config) = event.event() {
                shell_config = config.as_ref().clone();
            }
            let taskbar_layout_changed = matches!(
                event.event(),
                PlatformEvent::DisplayChanged | PlatformEvent::TaskbarCreated
            );
            let keep_running = dispatch_event(
                &class,
                features,
                &shell_config,
                &mut observation_runtime,
                &mut slots,
                event,
            )?;
            if taskbar_layout_changed {
                let dock_handles = slots
                    .iter()
                    .map(|slot| slot.dock_hwnd())
                    .collect::<Vec<_>>();
                if let Some(taskbars) = explorer_taskbars.as_mut() {
                    taskbars.reconcile_and_hide(&dock_handles)?;
                }
            }
            Ok(keep_running)
        }
    });
    drop(observation_runtime);
    drop(slots);
    drop(timer);
    drop(class);
    drop(explorer_taskbars);
    result
}

struct WatchdogHeartbeat {
    sender: Option<SyncSender<()>>,
}

impl WatchdogHeartbeat {
    fn new(enabled: bool) -> Self {
        if !enabled {
            return Self { sender: None };
        }
        let (sender, receiver) = sync_channel::<()>(1);
        let spawned = Builder::new()
            .name("minha-ui-watchdog-heartbeat".to_owned())
            .spawn(move || {
                while receiver.recv().is_ok() {
                    // Acquire stdout only while emitting one protocol frame.
                    // Holding StdoutLock across the blocking receive would
                    // deadlock normal startup diagnostics in the UI thread.
                    let stdout = std::io::stdout();
                    let mut output = stdout.lock();
                    if writeln!(output, "MINHA_UI_HEARTBEAT v1").is_err() || output.flush().is_err()
                    {
                        break;
                    }
                }
            });
        match spawned {
            Ok(_thread) => Self {
                sender: Some(sender),
            },
            Err(_) => Self { sender: None },
        }
    }

    fn pulse(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(());
        }
    }
}

const fn solid_material_for_accessibility(safe_mode: bool, high_contrast: bool) -> bool {
    safe_mode || high_contrast
}

const fn taskbar_replacement_enabled(
    policy: shell_core::TaskbarPolicy,
    safe_mode: bool,
    explicit_opt_in: bool,
) -> bool {
    !safe_mode
        && explicit_opt_in
        && cfg!(feature = "experimental-taskbar-replacement")
        && matches!(policy, shell_core::TaskbarPolicy::Hide)
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self> {
        // SAFETY: Category 8 (FFI boundary). The UI thread initializes one STA
        // before creating shell, WIC, or composition resources.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok()?;
        // SAFETY: Category 8 (FFI boundary). The same long-lived STA owns WinRT
        // activation used by native Quick Settings actions.
        if let Err(error) = unsafe { RoInitialize(RO_INIT_SINGLETHREADED) } {
            // SAFETY: Category 8 (FFI boundary). Balance the successful COM
            // initialization when WinRT initialization cannot be completed.
            unsafe { CoUninitialize() };
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). This balances the successful
        // RoInitialize call on the same UI thread.
        unsafe { RoUninitialize() };
        // SAFETY: Category 8 (FFI boundary). This balances the successful
        // CoInitializeEx call on the same UI thread.
        unsafe { CoUninitialize() };
    }
}

pub(super) fn register_dock_window(hwnd: HWND) {
    register_window(hwnd, &DOCK_WINDOWS, &DOCK_WINDOW);
}

pub(super) fn unregister_dock_window(hwnd: HWND) {
    unregister_window(hwnd, &DOCK_WINDOWS, &DOCK_WINDOW);
}

pub(super) fn register_topbar_window(hwnd: HWND) {
    register_window(hwnd, &TOPBAR_WINDOWS, &TOPBAR_WINDOW);
}

pub(super) fn unregister_topbar_window(hwnd: HWND) {
    unregister_window(hwnd, &TOPBAR_WINDOWS, &TOPBAR_WINDOW);
}

pub(super) fn register_popover_window(hwnd: HWND) {
    register_window(hwnd, &POPOVER_WINDOWS, &POPOVER_WINDOW);
}

pub(super) fn unregister_popover_window(hwnd: HWND) {
    unregister_window(hwnd, &POPOVER_WINDOWS, &POPOVER_WINDOW);
}

pub(super) fn register_app_menu_window(hwnd: HWND) {
    register_window(hwnd, &APP_MENU_WINDOWS, &APP_MENU_WINDOW);
}

pub(super) fn unregister_app_menu_window(hwnd: HWND) {
    unregister_window(hwnd, &APP_MENU_WINDOWS, &APP_MENU_WINDOW);
}

pub(super) fn register_preview_window(hwnd: HWND) {
    register_window(hwnd, &PREVIEW_WINDOWS, &PREVIEW_WINDOW);
}

pub(super) fn unregister_preview_window(hwnd: HWND) {
    unregister_window(hwnd, &PREVIEW_WINDOWS, &PREVIEW_WINDOW);
}

pub(super) fn register_settings_window(hwnd: HWND) {
    register_window(hwnd, &SETTINGS_WINDOWS, &SETTINGS_WINDOW);
}

pub(super) fn unregister_settings_window(hwnd: HWND) {
    unregister_window(hwnd, &SETTINGS_WINDOWS, &SETTINGS_WINDOW);
}

pub(super) fn is_dock_window(hwnd: HWND) -> bool {
    contains_window(hwnd, &DOCK_WINDOWS, &DOCK_WINDOW)
}

pub(super) fn is_topbar_window(hwnd: HWND) -> bool {
    contains_window(hwnd, &TOPBAR_WINDOWS, &TOPBAR_WINDOW)
}

pub(super) fn is_popover_window(hwnd: HWND) -> bool {
    contains_window(hwnd, &POPOVER_WINDOWS, &POPOVER_WINDOW)
}

pub(super) fn is_app_menu_window(hwnd: HWND) -> bool {
    contains_window(hwnd, &APP_MENU_WINDOWS, &APP_MENU_WINDOW)
}

pub(super) fn is_preview_window(hwnd: HWND) -> bool {
    contains_window(hwnd, &PREVIEW_WINDOWS, &PREVIEW_WINDOW)
}

pub(super) fn is_settings_window(hwnd: HWND) -> bool {
    contains_window(hwnd, &SETTINGS_WINDOWS, &SETTINGS_WINDOW)
}

fn register_window(
    hwnd: HWND,
    registry: &'static OnceLock<Mutex<Vec<isize>>>,
    latest: &'static AtomicIsize,
) {
    let raw = hwnd.0 as isize;
    let windows = registry.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut values) = windows.lock()
        && !values.contains(&raw)
    {
        values.push(raw);
    }
    latest.store(raw, Ordering::Release);
}

fn unregister_window(
    hwnd: HWND,
    registry: &'static OnceLock<Mutex<Vec<isize>>>,
    latest: &'static AtomicIsize,
) {
    let raw = hwnd.0 as isize;
    if let Some(windows) = registry.get()
        && let Ok(mut values) = windows.lock()
    {
        values.retain(|value| *value != raw);
        if latest.load(Ordering::Acquire) == raw {
            latest.store(
                values.last().copied().unwrap_or_default(),
                Ordering::Release,
            );
        }
        return;
    }
    if latest.load(Ordering::Acquire) == raw {
        latest.store(0, Ordering::Release);
    }
}

fn contains_window(
    hwnd: HWND,
    registry: &'static OnceLock<Mutex<Vec<isize>>>,
    latest: &'static AtomicIsize,
) -> bool {
    let raw = hwnd.0 as isize;
    latest.load(Ordering::Acquire) == raw
        || registry
            .get()
            .and_then(|windows| windows.lock().ok().map(|values| values.contains(&raw)))
            .unwrap_or(false)
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}

#[cfg(test)]
mod material_policy_tests {
    use super::{solid_material_for_accessibility, taskbar_replacement_enabled};

    #[test]
    fn system_backdrop_is_not_required_for_translucent_composition() {
        assert!(!solid_material_for_accessibility(false, false));
        assert!(solid_material_for_accessibility(true, false));
        assert!(solid_material_for_accessibility(false, true));
    }

    #[test]
    fn stable_build_never_replaces_the_explorer_taskbar() {
        if !cfg!(feature = "experimental-taskbar-replacement") {
            for policy in [
                shell_core::TaskbarPolicy::Off,
                shell_core::TaskbarPolicy::AutoHide,
                shell_core::TaskbarPolicy::Hide,
            ] {
                assert!(!taskbar_replacement_enabled(policy, false, true));
            }
        }
    }

    #[test]
    fn safe_mode_never_replaces_the_explorer_taskbar() {
        assert!(!taskbar_replacement_enabled(
            shell_core::TaskbarPolicy::Hide,
            true,
            true,
        ));
    }

    #[test]
    fn experimental_taskbar_replacement_requires_a_second_explicit_opt_in() {
        assert!(!taskbar_replacement_enabled(
            shell_core::TaskbarPolicy::Hide,
            false,
            false,
        ));
    }
}
