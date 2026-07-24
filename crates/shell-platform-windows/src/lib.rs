#![deny(unsafe_code)]

mod diagnostics;

#[allow(
    dead_code,
    reason = "external menu lifecycle is consumed by the native adapter incrementally"
)]
mod external_menu_coordinator;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 window ownership and callbacks are isolated here"
)]
mod win32;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 message dispatch and callbacks are isolated here"
)]
mod win32_windowing;

mod dock_edge_detection;
#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 action dispatch is isolated here")]
mod win32_actions;
mod win32_config;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 AppBar registration is isolated here")]
mod win32_appbar;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 AppBar FFI calls are isolated here")]
mod win32_appbar_ffi;

#[cfg(all(windows, test))]
mod win32_appbar_tests;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 executable identity lookup is isolated here"
)]
mod win32_app_identity;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Documented DWM backdrop calls are isolated here"
)]
mod win32_backdrop;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Windows package identity and installed-path queries are isolated here"
)]
mod win32_package_icon;
#[allow(
    unsafe_code,
    reason = "Hosted Win32 window identity lookup is isolated here"
)]
mod win32_window_identity;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 window discovery is isolated here")]
mod win32_discovery;

#[cfg(windows)]
mod win32_owner;

#[cfg(windows)]
mod win32_slots;

#[cfg(windows)]
mod win32_slot_lifecycle;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the owned observation worker initializes COM and posts wake messages here"
)]
mod win32_shell_observation;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 pointer coordinate helpers are isolated here"
)]
mod win32_pointer;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Windows Notification Facility reads and version-probed quiet-hours interop are isolated here"
)]
mod win32_do_not_disturb;
#[cfg(windows)]
mod win32_dock_render;
mod win32_dock_visibility;
#[cfg(windows)]
mod win32_surface_runtime;
#[cfg(windows)]
mod win32_topbar_render;

#[cfg(windows)]
mod win32_event_queue;

#[cfg(windows)]
mod win32_fullscreen_sync;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 drop decoding is isolated here")]
mod win32_drop;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 non-client hit testing is isolated here")]
mod win32_hit_test;
mod win32_installed_package_icons;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 QA timer ownership is isolated here")]
mod win32_timer;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Core Audio endpoint access is isolated here")]
mod win32_audio_endpoint;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Core Audio session and endpoint access is isolated here"
)]
mod win32_audio_panel;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "documented WMI and DDC/CI brightness adapters are isolated here"
)]
mod win32_brightness;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Windows system commands and input injection are isolated in this Adapter"
)]
mod win32_system_actions;
#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 topbar status adapters are isolated here")]
mod win32_topbar_status;

#[cfg(windows)]
#[allow(
    unsafe_code,
    dead_code,
    reason = "documented Windows capability probes are isolated and wired incrementally"
)]
mod win32_quick_settings_capabilities;

#[cfg(windows)]
#[allow(
    unsafe_code,
    dead_code,
    reason = "documented direct actions are isolated and wired incrementally"
)]
mod win32_quick_settings_actions;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "native quick-settings state adapters are isolated here"
)]
mod win32_quick_settings_system;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 HWND ownership is isolated here")]
mod win32_window;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 work-area lookup is isolated here")]
mod win32_work_area;

#[cfg(windows)]
#[allow(unsafe_code, reason = "DWM thumbnail probing is isolated here")]
mod win32_preview;
#[cfg(windows)]
mod win32_preview_geometry;
#[cfg(windows)]
mod win32_preview_interaction;
#[cfg(windows)]
mod win32_preview_qa;
#[cfg(windows)]
mod win32_preview_render;
#[cfg(windows)]
#[allow(unsafe_code, reason = "DWM source sizing is isolated here")]
mod win32_preview_source;

#[cfg(windows)]
mod win32_sample_state;

#[cfg(windows)]
mod win32_popover_render;

mod background_apps;
#[allow(
    unsafe_code,
    reason = "the owned catalog worker only uses PostMessageW to wake the owner window"
)]
mod background_apps_worker;
mod brightness_coordinator;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the brightness worker only posts a wake message to the owner window"
)]
mod brightness_worker;
mod dock_context_menu;
mod dock_controller;
mod dock_controller_interaction;
mod dock_controller_sync;
mod dock_controller_visibility;
mod dock_launch;
mod dock_placement;
mod dock_types;
mod dock_visibility_motion;
mod dock_visuals;
mod dock_window_sync;
mod latest_request_worker;
#[allow(
    dead_code,
    reason = "media controller is wired into the native owner incrementally"
)]
mod media_session_controller;
#[allow(dead_code, reason = "media worker contracts are wired incrementally")]
mod media_session_types;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the media worker only posts a wake message to the owner window"
)]
mod media_session_worker;
mod native_event_route;
mod native_tray;
mod night_light_coordinator;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the night light worker only posts a wake message"
)]
mod night_light_worker;
mod popover_adapters;
mod popover_controller;
mod popover_types;
mod preview_controller;
mod preview_motion;
#[allow(
    dead_code,
    reason = "adaptive controller is wired into the native owner incrementally"
)]
mod quick_settings_controller;
#[allow(
    dead_code,
    reason = "adaptive control intents are consumed by the native owner incrementally"
)]
mod quick_settings_types;
mod runtime;
mod settings_controller;
mod topbar_controller;
mod topbar_types;
#[allow(
    dead_code,
    reason = "pure tray activation coordination is consumed by the native adapter incrementally"
)]
mod tray_activation;
#[allow(
    dead_code,
    reason = "bounded Explorer tray decoding is consumed by the native source incrementally"
)]
mod tray_record_decoder;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "read-only notification registration and process capture is isolated here"
)]
mod win32_background_apps;
#[cfg(windows)]
#[allow(
    unsafe_code,
    dead_code,
    reason = "process-owned WinEvent hook registration and callback forwarding are isolated here"
)]
mod win32_external_menu_events;
#[cfg(windows)]
mod win32_media_sessions;
#[cfg(windows)]
#[allow(
    unsafe_code,
    dead_code,
    reason = "validated Win32 tray callback forwarding is isolated and wired incrementally"
)]
mod win32_tray_activation;
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "bounded Explorer toolbar discovery and remote-memory reads are isolated here"
)]
mod win32_tray_source;
mod window_preview;

#[cfg(test)]
mod integration_tests;

pub(crate) use background_apps::BackgroundAppId;
pub(crate) use dock_context_menu::{
    DockContextMenuController, DockContextMenuItem, QueuedContextMenuAction,
};
pub(crate) use dock_controller::DockController;
#[cfg(test)]
pub(crate) use dock_placement::TaskbarEdge;
pub(crate) use dock_placement::{
    DockEdgeGeometry, DockPhysicalPlacement, FullscreenObservation, FullscreenPolicy,
    MonitorPlacementInput, SlotReconcileAction, plan_monitor_placements, reconcile_monitor_slots,
    resolve_dock_visibility,
};
pub(crate) use dock_types::{
    ContextMenuCommand, DockAnimator, DockControllerError, DockKey, DockPointerPhase,
    DockPointerSample, DockRuntimeConfig, QueuedDockAction,
};
pub(crate) use dock_visibility_motion::DockVisibilityMotion;
pub(crate) use dock_window_sync::ObservedWindow;
#[allow(unused_imports)]
pub(crate) use external_menu_coordinator::{
    EXTERNAL_MENU_OBSERVATION_TIMEOUT, EXTERNAL_MENU_WATCHDOG, ExternalMenuCoordinator,
    ExternalMenuEffect, ExternalMenuPhase,
};
pub(crate) use native_event_route::{NativeEventTarget, NativeWindowId, NativeWindowSlot};
#[cfg(test)]
pub(crate) use native_event_route::{NativeRouteDecision, route_native_event_to_slot};
pub(crate) use popover_adapters::{DefaultPopoverDataProvider, OfflineWeatherProvider};
pub(crate) use popover_controller::PopoverController;
pub(crate) use popover_types::{
    PopoverAction, PopoverDataError, PopoverDataProvider, PopoverItem, PopoverKey,
    PopoverLoadState, PopoverPayload, ProjectionMode, QueuedPopoverAction, SessionAction,
    SystemRoute, WeatherAccess, WeatherItem, WeatherProvider,
};
#[cfg(test)]
pub(crate) use preview_controller::{PREVIEW_BRIDGE_MS, PREVIEW_DWELL_MS};
pub(crate) use preview_controller::{PreviewController, PreviewEffect, PreviewPhase};
pub(crate) use preview_motion::{PreviewEntranceMotion, PreviewMotionSpec};
pub(crate) use quick_settings_controller::QuickSettingsController;
pub(crate) use quick_settings_types::{
    AudioOutputId, AudioOutputSnapshot, AudioPanelSnapshot, AudioSessionId, AudioSessionSnapshot,
    DoNotDisturbMode, QueuedQuickSettingsAction, QuickControlAvailability, QuickControlCapability,
    QuickSettingsCapabilities, QuickSettingsIntent, QuickSettingsKey,
};
pub(crate) use runtime::{
    DockRenderAction, DockRenderChange, RuntimeAction, RuntimeOrchestrator,
    classify_dock_render_action,
};
pub(crate) use settings_controller::{QueuedSettingsAction, SettingsController, SettingsKey};
#[cfg(test)]
pub(crate) use settings_controller::{SettingsEdit, SettingsError, SettingsSection};
pub(crate) use topbar_controller::TopbarController;
pub(crate) use topbar_types::{
    NetworkSnapshot, PowerSnapshot, QueuedTopbarAction, TopbarKey, TopbarOverlayAnchor,
    TopbarPointerPhase, TopbarPointerSample, TopbarSnapshot, foreground_app_label,
};
#[allow(
    unused_imports,
    reason = "tray activation seam is consumed by native adapter incrementally"
)]
pub(crate) use tray_activation::{
    TrayActivationCoordinator, TrayActivationEffect, TrayActivationId, TrayActivationRequest,
    TrayActivationResult, TrayActivationStatus, TrayActivationStrategy, TrayCallbackSink,
    TrayMessage, TrayScreenPoint, WM_CONTEXTMENU, WM_RBUTTONDOWN, WM_RBUTTONUP,
    normalize_executable_identity,
};
#[cfg(windows)]
pub use win32::{ShowcaseRunConfig, run_showcase};
pub(crate) use window_preview::{
    PreviewAction, PreviewCapture, PreviewQueuedAction, PreviewUnavailableReason,
    PreviewWindowState,
};

use shell_renderer::{DipPoint, PhysicalRect};
#[cfg(test)]
use shell_renderer::{DipRect, rounded_content_hit};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PlatformEvent {
    TaskbarCreated,
    AppBarPositionChanged,
    DpiChanged(PhysicalRect),
    DisplayChanged,
    PowerResumed,
    QuickSettingsRefresh(RefreshScope),
    MediaSessionsChanged(
        Result<media_session_types::MediaSessionSnapshot, media_session_types::MediaSessionError>,
    ),
    MediaTransportCompleted(media_session_types::MediaTransportResult),
    NightLightCompleted(night_light_worker::NightLightWorkerResult),
    BrightnessCompleted(brightness_worker::BrightnessWorkerResult),
    DeviceLost,
    DockPointer(DockPointerSample),
    DockEdgeProbe,
    DockAnimationFrame,
    PreviewTimer,
    PreviewPointerMoved(DipPoint),
    PreviewPointerPressed(DipPoint),
    PreviewPointerReleased(DipPoint),
    PreviewDismissed,
    DockKey(DockKey),
    TopbarPointer(TopbarPointerSample),
    TopbarKey(TopbarKey),
    #[expect(
        dead_code,
        reason = "native menu command dispatch remains an internal compatibility seam"
    )]
    DockContextMenu {
        point: DipPoint,
        command: ContextMenuCommand,
    },
    DockContextMenuRequested {
        point: DipPoint,
    },
    DockDrop {
        point: DipPoint,
        path: String,
    },
    PopoverKey(PopoverKey),
    PopoverPointerPressed(DipPoint),
    PopoverContextPressed(DipPoint),
    PopoverPointer(DipPoint),
    PopoverPointerMoved(DipPoint),
    PopoverContextRequested(DipPoint),
    PopoverScroll(isize),
    DismissTransientOverlays,
    SettingsKey(SettingsKey),
    SyncWindows,
    ShellObservationLoaded(win32_shell_observation::ShellObservationLoadResult),
    BackgroundAppsLoaded(background_apps_worker::BackgroundAppsLoadResult),
    ExternalMenuPopupStarted {
        window: NativeWindowId,
        owner_process_id: u32,
    },
    ExternalMenuPopupEnded {
        window: NativeWindowId,
        owner_process_id: u32,
    },
    QaExitRequested,
    CloseRequested,
    Destroyed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RefreshScope {
    Devices,
}

#[must_use]
pub(crate) const fn popover_activation_event(active: bool) -> Option<PlatformEvent> {
    if active {
        None
    } else {
        Some(PlatformEvent::DismissTransientOverlays)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum LifecycleMessage {
    TaskbarCreated(u32),
    DpiChanged(PhysicalRect),
    DisplayChanged,
    PowerResumed,
    Timer,
    Close,
    Destroy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum HitRegion {
    Interactive,
    Transparent,
}

#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-platform-windows"
}

#[must_use]
#[cfg(test)]
pub(crate) const fn translate_lifecycle_message(
    message_id: u32,
    lifecycle_message: LifecycleMessage,
) -> Option<PlatformEvent> {
    match lifecycle_message {
        LifecycleMessage::TaskbarCreated(taskbar_created) if message_id == taskbar_created => {
            Some(PlatformEvent::TaskbarCreated)
        }
        LifecycleMessage::TaskbarCreated(_) => None,
        LifecycleMessage::DpiChanged(rect) => Some(PlatformEvent::DpiChanged(rect)),
        LifecycleMessage::DisplayChanged => Some(PlatformEvent::DisplayChanged),
        LifecycleMessage::PowerResumed => Some(PlatformEvent::PowerResumed),
        LifecycleMessage::Timer => Some(PlatformEvent::QaExitRequested),
        LifecycleMessage::Close => Some(PlatformEvent::CloseRequested),
        LifecycleMessage::Destroy => Some(PlatformEvent::Destroyed),
    }
}

#[must_use]
#[cfg(test)]
pub(crate) fn classify_hit_test(bounds: DipRect, radius: f32, point: DipPoint) -> HitRegion {
    if rounded_content_hit(bounds, radius, point) {
        HitRegion::Interactive
    } else {
        HitRegion::Transparent
    }
}
