use std::sync::atomic::{AtomicI32, AtomicIsize, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;
use windows::core::{Result, w};

use crate::PlatformEvent;
use crate::win32_shell_observation::ShellObservationRuntime;
use crate::win32_slots::{
    SlotFeatures, create_slots, dispatch_event, handle_broadcast_event, print_monitor_placements,
};
use crate::win32_timer::TimerGuard;
use crate::win32_window::WindowClass;
use crate::win32_windowing::{message_loop, monitor_placement_inputs};

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

    let monitors = monitor_placement_inputs()?;
    if monitors.is_empty() {
        return Err(windows::core::Error::new(
            invalid_arg(),
            "no display monitors were enumerated",
        ));
    }
    print_monitor_placements(&monitors);
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
    let shell_config = crate::win32_config::load_config();
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
    let timer = config
        .qa_exit_ms
        .map(|milliseconds| TimerGuard::start(slots[0].topbar_hwnd(), milliseconds))
        .transpose()?;
    for slot in &mut slots {
        slot.show_shells();
        slot.print_windows();
    }
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
            handle_broadcast_event(
                &class,
                features,
                &shell_config,
                initial_observation,
                &mut slots,
                event,
            )?;
        }
    }
    let result = message_loop(|event| {
        dispatch_event(
            &class,
            features,
            &shell_config,
            &mut observation_runtime,
            &mut slots,
            event,
        )
    });
    drop(observation_runtime);
    drop(slots);
    drop(timer);
    drop(class);
    result
}

const fn solid_material_for_accessibility(safe_mode: bool, high_contrast: bool) -> bool {
    safe_mode || high_contrast
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
    use super::solid_material_for_accessibility;

    #[test]
    fn system_backdrop_is_not_required_for_translucent_composition() {
        assert!(!solid_material_for_accessibility(false, false));
        assert!(solid_material_for_accessibility(true, false));
        assert!(solid_material_for_accessibility(false, true));
    }
}
