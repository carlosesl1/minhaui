#![deny(unsafe_code)]

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

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 action dispatch is isolated here")]
mod win32_actions;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 popup menu dispatch is isolated here")]
mod win32_context_menu;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 window discovery is isolated here")]
mod win32_discovery;

#[cfg(windows)]
mod win32_owner;

#[cfg(windows)]
mod win32_slots;

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 pointer coordinate helpers are isolated here"
)]
mod win32_pointer;

#[cfg(windows)]
mod win32_dock_render;

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
mod win32_preview_qa;

#[cfg(windows)]
mod win32_sample_state;

#[cfg(windows)]
mod win32_popover_render;

mod dock_controller;
mod dock_controller_interaction;
mod dock_controller_sync;
mod dock_launch;
mod dock_placement;
mod dock_types;
mod dock_visuals;
mod dock_window_sync;
mod native_event_route;
mod popover_adapters;
mod popover_controller;
mod popover_types;
mod runtime;
mod settings_controller;
mod topbar_controller;
mod topbar_types;
mod window_preview;

pub use dock_controller::DockController;
pub use dock_placement::{
    DockPhysicalPlacement, FullscreenObservation, FullscreenPolicy, MonitorPlacementInput,
    MonitorShellPlacement, SlotReconcileAction, TaskbarEdge, plan_monitor_placements,
    reconcile_monitor_slots, taskbar_edge,
};
pub use dock_types::{
    ContextMenuCommand, DockAnimator, DockControllerError, DockKey, DockPointerPhase,
    DockPointerSample, DockRuntimeConfig, QueuedDockAction,
};
pub use dock_window_sync::ObservedWindow;
pub use native_event_route::{
    NativeEventTarget, NativeRouteDecision, NativeWindowId, NativeWindowSlot,
    route_native_event_to_slot,
};
pub use popover_adapters::{DefaultPopoverDataProvider, OfflineWeatherProvider};
pub use popover_controller::{PopoverController, PopoverControllerError};
pub use popover_types::{
    PopoverAction, PopoverDataError, PopoverDataProvider, PopoverItem, PopoverKey,
    PopoverLoadState, PopoverPayload, QueuedPopoverAction, SessionAction, WeatherAccess,
    WeatherItem, WeatherProvider,
};
pub use runtime::{
    DockRenderAction, DockRenderChange, RuntimeAction, RuntimeOrchestrator,
    classify_dock_render_action,
};
pub use settings_controller::{
    QueuedSettingsAction, SettingsController, SettingsEdit, SettingsError, SettingsKey,
    SettingsSection,
};
pub use topbar_controller::{TopbarController, TopbarControllerError};
pub use topbar_types::{
    NetworkSnapshot, PollBudget, PowerSnapshot, QueuedTopbarAction, ThroughputLabel, TopbarKey,
    TopbarPointerPhase, TopbarPointerSample, TopbarSnapshot,
};
#[cfg(windows)]
pub use win32::{ShowcaseRunConfig, run_showcase};
pub use window_preview::{
    PreviewAction, PreviewCapture, PreviewQueuedAction, PreviewUnavailableReason,
};

use shell_renderer::{DipPoint, DipRect, PhysicalRect, rounded_content_hit};

#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    TaskbarCreated,
    DpiChanged(PhysicalRect),
    DisplayChanged,
    PowerResumed,
    DeviceLost,
    DockPointer(DockPointerSample),
    DockKey(DockKey),
    TopbarPointer(TopbarPointerSample),
    TopbarKey(TopbarKey),
    DockContextMenu {
        point: DipPoint,
        command: ContextMenuCommand,
    },
    DockDrop {
        point: DipPoint,
        path: String,
    },
    PopoverKey(PopoverKey),
    SettingsKey(SettingsKey),
    SyncWindows,
    QaExitRequested,
    CloseRequested,
    Destroyed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleMessage {
    TaskbarCreated(u32),
    DpiChanged(PhysicalRect),
    DisplayChanged,
    PowerResumed,
    Timer,
    Close,
    Destroy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HitRegion {
    Interactive,
    Transparent,
}

#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-platform-windows"
}

#[must_use]
pub const fn translate_lifecycle_message(
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
pub fn classify_hit_test(bounds: DipRect, radius: f32, point: DipPoint) -> HitRegion {
    if rounded_content_hit(bounds, radius, point) {
        HitRegion::Interactive
    } else {
        HitRegion::Transparent
    }
}
