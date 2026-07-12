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
mod win32_dock_render;

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
#[allow(unsafe_code, reason = "Win32 HWND ownership is isolated here")]
mod win32_window;

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 work-area lookup is isolated here")]
mod win32_work_area;

mod dock_controller;
mod dock_controller_interaction;
mod dock_controller_sync;
mod dock_launch;
mod dock_placement;
mod dock_types;
mod dock_visuals;
mod dock_window_sync;
mod runtime;

pub use dock_controller::DockController;
pub use dock_placement::DockPhysicalPlacement;
pub use dock_types::{
    ContextMenuCommand, DockAnimator, DockControllerError, DockPointerPhase, DockPointerSample,
    DockRuntimeConfig, QueuedDockAction,
};
pub use dock_window_sync::ObservedWindow;
pub use runtime::{
    DockRenderAction, DockRenderChange, RuntimeAction, RuntimeOrchestrator,
    classify_dock_render_action,
};
#[cfg(windows)]
pub use win32::run_showcase;

use shell_renderer::{DipPoint, DipRect, PhysicalRect, rounded_content_hit};

#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    TaskbarCreated,
    DpiChanged(PhysicalRect),
    DisplayChanged,
    PowerResumed,
    DeviceLost,
    DockPointer(DockPointerSample),
    DockContextMenu {
        point: DipPoint,
        command: ContextMenuCommand,
    },
    DockDrop {
        point: DipPoint,
        path: String,
    },
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
