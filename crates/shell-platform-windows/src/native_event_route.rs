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

    pub(crate) const fn contains(self, window: NativeWindowId) -> bool {
        self.topbar.value() == window.value()
            || self.dock.value() == window.value()
            || self.popover.value() == window.value()
            || self.settings.value() == window.value()
            || self.preview.value() == window.value()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeEventTarget {
    Broadcast,
    Window(NativeWindowId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub enum NativeRouteDecision {
    Broadcast,
    Slot(MonitorId),
    UnknownWindow,
}

#[must_use]
#[cfg(test)]
pub fn route_native_event_to_slot(
    slots: &[NativeWindowSlot],
    target: NativeEventTarget,
) -> NativeRouteDecision {
    match target {
        NativeEventTarget::Broadcast => NativeRouteDecision::Broadcast,
        NativeEventTarget::Window(window) => slots
            .iter()
            .find(|slot| slot.contains(window))
            .map_or(NativeRouteDecision::UnknownWindow, |slot| {
                NativeRouteDecision::Slot(slot.monitor)
            }),
    }
}
