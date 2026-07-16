#![deny(unsafe_code)]

use shell_core::MonitorId;
use windows::core::Result;

use crate::win32_appbar::TopbarWindow;
use crate::win32_event_queue::{RoutedPlatformEvent, native_window_id};
use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows};
use crate::win32_slot_lifecycle::{create_slot, reconcile_slots};
use crate::win32_timer::TimerGuard;
use crate::win32_window::{OwnedWindow, WindowClass, print_window};
use crate::{
    DockRuntimeConfig, MonitorPlacementInput, NativeEventTarget, NativeRouteDecision,
    NativeWindowSlot, PlatformEvent, route_native_event_to_slot,
};

pub(super) struct ShellSlot {
    pub(super) _sync_timer: TimerGuard,
    pub(super) runtime: RuntimeSurfaces,
    pub(super) monitor: MonitorId,
    pub(super) topbar: TopbarWindow,
    pub(super) dock: OwnedWindow,
    pub(super) popover: OwnedWindow,
    pub(super) preview: OwnedWindow,
    pub(super) settings: OwnedWindow,
}

#[derive(Clone, Copy)]
pub(super) struct SlotFeatures {
    pub(super) force_warp: bool,
    pub(super) safe_mode: bool,
    pub(super) backdrop_enabled: bool,
    pub(super) reduced_motion: bool,
}

impl ShellSlot {
    pub(super) fn handle_event(&mut self, event: PlatformEvent) -> Result<bool> {
        if matches!(event, PlatformEvent::AppBarPositionChanged) {
            if let Some(work_area) = self.topbar.reconcile_notification()? {
                self.apply_work_area(work_area)?;
            }
            return Ok(true);
        }
        self.runtime.handle_event(
            event,
            &mut self.topbar,
            &mut self.dock,
            &mut self.popover,
            &mut self.preview,
            &mut self.settings,
        )
    }

    pub(super) fn print_windows(&self) {
        print_window(&self.topbar, self.runtime.device_kind());
        print_window(&self.dock, self.runtime.device_kind());
        print_window(&self.popover, self.runtime.device_kind());
        print_window(&self.preview, self.runtime.device_kind());
        print_window(&self.settings, self.runtime.device_kind());
    }

    pub(super) fn show_shells(&self) {
        self.topbar.show();
        self.dock.show();
    }

    pub(super) fn topbar_hwnd(&self) -> windows::Win32::Foundation::HWND {
        self.topbar.hwnd
    }

    pub(super) fn reregister_topbar(&mut self) -> Result<()> {
        self.topbar.reregister()
    }

    fn route(&self) -> NativeWindowSlot {
        NativeWindowSlot::new(
            self.monitor,
            native_window_id(self.topbar.hwnd),
            native_window_id(self.dock.hwnd),
            native_window_id(self.popover.hwnd),
            native_window_id(self.settings.hwnd),
            native_window_id(self.preview.hwnd),
        )
    }

    pub(super) fn reconcile_monitor(&mut self, monitor: MonitorPlacementInput) -> Result<()> {
        self.monitor = monitor.monitor();
        self.topbar.reconcile(monitor.bounds())?;
        self.apply_work_area(self.topbar.work_area()?)
    }

    fn apply_work_area(&mut self, work_area: shell_renderer::PhysicalRect) -> Result<()> {
        let (config, hidden) = crate::resolve_dock_visibility(
            self.runtime.dock_controller.config(),
            self.runtime.dock_controller.state().dock().is_revealed(),
            self.runtime.fullscreen_suppressed,
        );
        self.runtime
            .snap_dock_visibility(&mut self.dock, work_area, config, hidden)?;
        self.runtime.sync_dock_animation(&self.dock)?;
        if !self.runtime.place_active_overlay(
            work_area,
            &self.topbar,
            &self.dock,
            &mut self.popover,
        )? {
            self.popover.reposition(work_area)?;
        }
        self.settings.reposition(work_area)?;
        self.preview.reposition(work_area)?;
        self.runtime.rebuild_native_surfaces(SurfaceWindows {
            topbar: &self.topbar,
            dock: &self.dock,
            popover: &self.popover,
            preview: &self.preview,
            settings: &self.settings,
        })
    }
}

pub(super) fn create_slots(
    class: &WindowClass,
    monitors: &[MonitorPlacementInput],
    features: SlotFeatures,
) -> Result<Vec<ShellSlot>> {
    let mut slots = Vec::new();
    for monitor in monitors {
        slots.push(create_slot(class, *monitor, features)?);
    }
    Ok(slots)
}

pub(super) fn dispatch_event(
    class: &WindowClass,
    features: SlotFeatures,
    slots: &mut Vec<ShellSlot>,
    event: RoutedPlatformEvent,
) -> Result<bool> {
    match event.target() {
        NativeEventTarget::Broadcast => {
            handle_broadcast_event(class, features, slots, event.into_event())
        }
        NativeEventTarget::Window(_) => dispatch_window_event(class, features, slots, event),
    }
}

pub(super) fn handle_broadcast_event(
    class: &WindowClass,
    features: SlotFeatures,
    slots: &mut Vec<ShellSlot>,
    event: PlatformEvent,
) -> Result<bool> {
    match event {
        PlatformEvent::DisplayChanged => {
            reconcile_slots(class, slots, features)?;
            Ok(true)
        }
        PlatformEvent::TaskbarCreated => {
            for slot in slots.iter_mut() {
                slot.reregister_topbar()?;
            }
            reconcile_slots(class, slots, features)?;
            Ok(true)
        }
        PlatformEvent::QaExitRequested | PlatformEvent::CloseRequested => Ok(false),
        event => {
            for slot in slots {
                if !slot.handle_event(event.clone())? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

pub(super) fn print_monitor_placements(monitors: &[MonitorPlacementInput]) {
    let fullscreen = Vec::new();
    for placement in crate::plan_monitor_placements(
        monitors,
        DockRuntimeConfig::default(),
        fullscreen.as_slice(),
    ) {
        let dock = placement.dock().rect();
        println!(
            "MONITOR_PLACEMENT id={} edge={:?} suppressed={} dock={},{},{},{}",
            placement.monitor().value(),
            placement.taskbar_edge(),
            placement.suppressed(),
            dock.x,
            dock.y,
            dock.width,
            dock.height
        );
    }
}

fn dispatch_window_event(
    class: &WindowClass,
    features: SlotFeatures,
    slots: &mut Vec<ShellSlot>,
    event: RoutedPlatformEvent,
) -> Result<bool> {
    let routes = slots.iter().map(ShellSlot::route).collect::<Vec<_>>();
    match route_native_event_to_slot(&routes, event.target()) {
        NativeRouteDecision::Slot(monitor) => slots
            .iter_mut()
            .find(|slot| slot.monitor == monitor)
            .map_or(Ok(true), |slot| slot.handle_event(event.into_event())),
        NativeRouteDecision::Broadcast => {
            handle_broadcast_event(class, features, slots, event.into_event())
        }
        NativeRouteDecision::UnknownWindow => Ok(true),
    }
}
