#![deny(unsafe_code)]

mod diagnostics;

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
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Hosted Win32 window identity lookup is isolated here"
)]
mod win32_window_identity;

#[cfg(windows)]
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
    reason = "Win32 pointer coordinate helpers are isolated here"
)]
mod win32_pointer;

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
#[allow(unsafe_code, reason = "Win32 topbar status adapters are isolated here")]
mod win32_topbar_status;

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
mod native_event_route;
mod popover_adapters;
mod popover_controller;
mod popover_types;
mod preview_controller;
mod preview_motion;
mod runtime;
mod settings_controller;
mod topbar_controller;
mod topbar_types;
mod window_preview;

#[cfg(test)]
mod integration_tests;

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
pub(crate) use native_event_route::{
    NativeEventTarget, NativeRouteDecision, NativeWindowId, NativeWindowSlot,
    route_native_event_to_slot,
};
pub(crate) use popover_adapters::{DefaultPopoverDataProvider, OfflineWeatherProvider};
pub(crate) use popover_controller::PopoverController;
pub(crate) use popover_types::{
    PopoverAction, PopoverDataError, PopoverDataProvider, PopoverItem, PopoverKey,
    PopoverLoadState, PopoverPayload, QueuedPopoverAction, SessionAction, WeatherAccess,
    WeatherItem, WeatherProvider,
};
#[cfg(test)]
pub(crate) use preview_controller::{PREVIEW_BRIDGE_MS, PREVIEW_DWELL_MS};
pub(crate) use preview_controller::{PreviewController, PreviewEffect, PreviewPhase};
pub(crate) use preview_motion::{PreviewEntranceMotion, PreviewMotionSpec};
pub(crate) use runtime::{
    DockRenderAction, DockRenderChange, RuntimeAction, RuntimeOrchestrator,
    classify_dock_render_action,
};
pub(crate) use settings_controller::{QueuedSettingsAction, SettingsController, SettingsKey};
#[cfg(test)]
pub(crate) use settings_controller::{SettingsEdit, SettingsError, SettingsSection};
pub(crate) use topbar_controller::TopbarController;
pub(crate) use topbar_types::{
    NetworkSnapshot, PollBudget, PowerSnapshot, QueuedTopbarAction, TopbarKey, TopbarPointerPhase,
    TopbarPointerSample, TopbarSnapshot,
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
    PopoverPointer(DipPoint),
    PopoverPointerMoved(DipPoint),
    DismissTransientOverlays,
    SettingsKey(SettingsKey),
    SyncWindows,
    QaExitRequested,
    CloseRequested,
    Destroyed,
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
