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
    #[expect(
        dead_code,
        reason = "retained for native-route diagnostics without exposing the wrapper field"
    )]
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
    preview: NativeWindowId,
}

impl NativeWindowSlot {
    #[must_use]
    pub const fn new(
        monitor: MonitorId,
        topbar: NativeWindowId,
        dock: NativeWindowId,
        popover: NativeWindowId,
        settings: NativeWindowId,
        preview: NativeWindowId,
    ) -> Self {
        Self {
            monitor,
            topbar,
            dock,
            popover,
            settings,
            preview,
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
                    || slot.preview == window
            })
            .map_or(NativeRouteDecision::UnknownWindow, |slot| {
                NativeRouteDecision::Slot(slot.monitor)
            }),
    }
}
