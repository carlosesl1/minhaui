#![deny(unsafe_code)]

use shell_core::MonitorId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeWindowId(isize);

impl NativeWindowId {
    #[must_use]
    pub const fn new(value: isize) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> isize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeWindowSlot {
    monitor: MonitorId,
    topbar: NativeWindowId,
    dock: NativeWindowId,
    popover: NativeWindowId,
    settings: NativeWindowId,
}

impl NativeWindowSlot {
    #[must_use]
    pub const fn new(
        monitor: MonitorId,
        topbar: NativeWindowId,
        dock: NativeWindowId,
        popover: NativeWindowId,
        settings: NativeWindowId,
    ) -> Self {
        Self {
            monitor,
            topbar,
            dock,
            popover,
            settings,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeEventTarget {
    Broadcast,
    Window(NativeWindowId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRouteDecision {
    Broadcast,
    Slot(MonitorId),
    UnknownWindow,
}

#[must_use]
pub fn route_native_event_to_slot(
    slots: &[NativeWindowSlot],
    target: NativeEventTarget,
) -> NativeRouteDecision {
    match target {
        NativeEventTarget::Broadcast => NativeRouteDecision::Broadcast,
        NativeEventTarget::Window(window) => slots
            .iter()
            .find(|slot| {
                slot.topbar == window
                    || slot.dock == window
                    || slot.popover == window
                    || slot.settings == window
            })
            .map_or(NativeRouteDecision::UnknownWindow, |slot| {
                NativeRouteDecision::Slot(slot.monitor)
            }),
    }
}
