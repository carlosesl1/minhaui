#![deny(unsafe_code)]

use shell_core::MonitorId;
use windows::core::Result;

use crate::win32_event_queue::{RoutedPlatformEvent, native_window_id};
use crate::win32_owner::{RuntimeSurfaces, SurfaceWindows};
use crate::win32_sample_state::{sample_dock_controller, sample_topbar_controller};
use crate::win32_timer::TimerGuard;
use crate::win32_window::{OwnedWindow, WindowClass, print_window};
use crate::win32_windowing::monitor_placement_inputs;
use crate::{
    DockRuntimeConfig, MonitorPlacementInput, NativeEventTarget, NativeRouteDecision,
    NativeWindowSlot, PlatformEvent, SlotReconcileAction, reconcile_monitor_slots,
    route_native_event_to_slot,
};

pub(super) struct ShellSlot {
    _sync_timer: TimerGuard,
    runtime: RuntimeSurfaces,
    monitor: MonitorId,
    topbar: OwnedWindow,
    dock: OwnedWindow,
    popover: OwnedWindow,
    settings: OwnedWindow,
}

impl ShellSlot {
    pub(super) fn handle_event(&mut self, event: PlatformEvent) -> Result<bool> {
        self.runtime.handle_event(
            event,
            &mut self.topbar,
            &mut self.dock,
            &mut self.popover,
            &mut self.settings,
        )
    }

    pub(super) fn print_windows(&self) {
        print_window(&self.topbar, self.runtime.device_kind());
        print_window(&self.dock, self.runtime.device_kind());
        print_window(&self.popover, self.runtime.device_kind());
        print_window(&self.settings, self.runtime.device_kind());
    }

    pub(super) const fn topbar_hwnd(&self) -> windows::Win32::Foundation::HWND {
        self.topbar.hwnd
    }

    fn route(&self) -> NativeWindowSlot {
        NativeWindowSlot::new(
            self.monitor,
            native_window_id(self.topbar.hwnd),
            native_window_id(self.dock.hwnd),
            native_window_id(self.popover.hwnd),
            native_window_id(self.settings.hwnd),
        )
    }

    fn reconcile_monitor(&mut self, monitor: MonitorPlacementInput) -> Result<()> {
        self.monitor = monitor.monitor();
        self.topbar.reposition(monitor.work_area())?;
        self.dock.apply_dock_visibility(
            monitor.work_area(),
            self.runtime.dock_controller.config(),
            false,
        )?;
        self.popover.reposition(monitor.work_area())?;
        self.settings.reposition(monitor.work_area())?;
        self.runtime
            .rebuild(&self.topbar, &self.dock, &self.popover, &self.settings)
    }
}

pub(super) fn create_slots(
    class: &WindowClass,
    monitors: &[MonitorPlacementInput],
    force_warp: bool,
    safe_mode: bool,
) -> Result<Vec<ShellSlot>> {
    let mut slots = Vec::new();
    for monitor in monitors {
        slots.push(create_slot(class, *monitor, force_warp, safe_mode)?);
    }
    Ok(slots)
}

pub(super) fn dispatch_event(
    class: &WindowClass,
    force_warp: bool,
    safe_mode: bool,
    slots: &mut Vec<ShellSlot>,
    event: RoutedPlatformEvent,
) -> Result<bool> {
    match event.target() {
        NativeEventTarget::Broadcast => {
            handle_broadcast_event(class, force_warp, safe_mode, slots, event.into_event())
        }
        NativeEventTarget::Window(_) => {
            dispatch_window_event(class, force_warp, safe_mode, slots, event)
        }
    }
}

pub(super) fn handle_broadcast_event(
    class: &WindowClass,
    force_warp: bool,
    safe_mode: bool,
    slots: &mut Vec<ShellSlot>,
    event: PlatformEvent,
) -> Result<bool> {
    match event {
        PlatformEvent::DisplayChanged | PlatformEvent::TaskbarCreated => {
            reconcile_slots(class, slots, force_warp, safe_mode)?;
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
    force_warp: bool,
    safe_mode: bool,
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
            handle_broadcast_event(class, force_warp, safe_mode, slots, event.into_event())
        }
        NativeRouteDecision::UnknownWindow => Ok(true),
    }
}

fn create_slot(
    class: &WindowClass,
    monitor: MonitorPlacementInput,
    force_warp: bool,
    safe_mode: bool,
) -> Result<ShellSlot> {
    let topbar = OwnedWindow::create(
        class,
        shell_renderer::native::ShowcaseRole::Topbar,
        monitor.work_area(),
    )?;
    let dock = OwnedWindow::create(
        class,
        shell_renderer::native::ShowcaseRole::Dock,
        monitor.work_area(),
    )?;
    let popover = OwnedWindow::create(
        class,
        shell_renderer::native::ShowcaseRole::Popover,
        monitor.work_area(),
    )?;
    let settings = OwnedWindow::create(
        class,
        shell_renderer::native::ShowcaseRole::Settings,
        monitor.work_area(),
    )?;
    let dock_controller = sample_dock_controller()?;
    let topbar_controller = sample_topbar_controller(topbar.rect.width)?;
    let runtime = RuntimeSurfaces::new(
        force_warp,
        SurfaceWindows {
            topbar: &topbar,
            dock: &dock,
            popover: &popover,
            settings: &settings,
        },
        dock_controller,
        topbar_controller,
        safe_mode,
    )?;
    let sync_timer = TimerGuard::start_sync(dock.hwnd)?;
    Ok(ShellSlot {
        _sync_timer: sync_timer,
        runtime,
        monitor: monitor.monitor(),
        topbar,
        dock,
        popover,
        settings,
    })
}

fn reconcile_slots(
    class: &WindowClass,
    slots: &mut Vec<ShellSlot>,
    force_warp: bool,
    safe_mode: bool,
) -> Result<()> {
    let monitors = monitor_placement_inputs()?;
    if monitors.is_empty() {
        return Err(windows::core::Error::new(
            invalid_arg(),
            "no display monitors were enumerated",
        ));
    }
    print_monitor_placements(&monitors);
    let current = slots.iter().map(|slot| slot.monitor).collect::<Vec<_>>();
    let actions = reconcile_monitor_slots(&current, &monitors);
    let mut old = std::mem::take(slots);
    let mut next = Vec::new();
    for action in actions {
        match action {
            SlotReconcileAction::Remove(monitor) => remove_slot(&mut old, monitor),
            SlotReconcileAction::Create(monitor) => {
                next.push(create_slot(
                    class,
                    monitor_input(&monitors, monitor)?,
                    force_warp,
                    safe_mode,
                )?);
            }
            SlotReconcileAction::Reuse(monitor) => {
                if let Some(mut slot) = take_slot(&mut old, monitor) {
                    slot.reconcile_monitor(monitor_input(&monitors, monitor)?)?;
                    next.push(slot);
                }
            }
        }
    }
    drop(old);
    for slot in &next {
        slot.print_windows();
    }
    *slots = next;
    Ok(())
}

fn remove_slot(slots: &mut Vec<ShellSlot>, monitor: MonitorId) {
    if let Some(index) = slots.iter().position(|slot| slot.monitor == monitor) {
        drop(slots.remove(index));
        println!("SLOT_REMOVED monitor={}", monitor.value());
    }
}

fn take_slot(slots: &mut Vec<ShellSlot>, monitor: MonitorId) -> Option<ShellSlot> {
    slots
        .iter()
        .position(|slot| slot.monitor == monitor)
        .map(|index| slots.remove(index))
}

fn monitor_input(
    monitors: &[MonitorPlacementInput],
    monitor: MonitorId,
) -> Result<MonitorPlacementInput> {
    monitors
        .iter()
        .copied()
        .find(|input| input.monitor() == monitor)
        .ok_or_else(|| windows::core::Error::new(invalid_arg(), "monitor missing during reconcile"))
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}
