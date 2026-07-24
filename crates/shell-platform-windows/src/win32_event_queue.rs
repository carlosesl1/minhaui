#![deny(unsafe_code)]

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::HWND;

use crate::{NativeEventTarget, NativeWindowId, PlatformEvent};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RoutedPlatformEvent {
    target: NativeEventTarget,
    event: PlatformEvent,
}

impl RoutedPlatformEvent {
    pub(super) const fn broadcast(event: PlatformEvent) -> Self {
        Self {
            target: NativeEventTarget::Broadcast,
            event,
        }
    }

    pub(super) fn window(hwnd: HWND, event: PlatformEvent) -> Self {
        Self {
            target: NativeEventTarget::Window(native_window_id(hwnd)),
            event,
        }
    }

    pub(super) const fn window_id(window: NativeWindowId, event: PlatformEvent) -> Self {
        Self {
            target: NativeEventTarget::Window(window),
            event,
        }
    }

    pub(super) const fn target(&self) -> NativeEventTarget {
        self.target
    }

    pub(super) const fn event(&self) -> &PlatformEvent {
        &self.event
    }

    pub(super) fn into_event(self) -> PlatformEvent {
        self.event
    }
}

static EVENT_QUEUE: OnceLock<Mutex<VecDeque<RoutedPlatformEvent>>> = OnceLock::new();
static DRAGGING_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();

pub(super) fn queue_event(event: RoutedPlatformEvent) {
    let queue = EVENT_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Ok(mut events) = queue.lock() {
        events.push_back(event);
    }
}

pub(super) fn queue_event_with_wake(
    event: RoutedPlatformEvent,
    wake: impl FnOnce() -> bool,
) -> bool {
    let queue = EVENT_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()));
    let mut events = queue
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    enqueue_and_wake(&mut events, event, wake)
}

fn enqueue_and_wake(
    events: &mut VecDeque<RoutedPlatformEvent>,
    event: RoutedPlatformEvent,
    wake: impl FnOnce() -> bool,
) -> bool {
    events.push_back(event);
    if wake() {
        return true;
    }
    events.pop_back();
    false
}

pub(super) fn next_event() -> Option<RoutedPlatformEvent> {
    EVENT_QUEUE
        .get()
        .and_then(|queue| queue.lock().ok()?.pop_front())
}

pub(super) fn set_dragging(hwnd: HWND, dragging: bool) {
    let raw = hwnd.0 as isize;
    let windows = DRAGGING_WINDOWS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut values) = windows.lock() {
        if dragging {
            if !values.contains(&raw) {
                values.push(raw);
            }
        } else {
            values.retain(|value| *value != raw);
        }
    }
}

pub(super) fn is_dragging(hwnd: HWND) -> bool {
    let raw = hwnd.0 as isize;
    DRAGGING_WINDOWS
        .get()
        .and_then(|windows| windows.lock().ok().map(|values| values.contains(&raw)))
        .unwrap_or(false)
}

pub(super) fn native_window_id(hwnd: HWND) -> NativeWindowId {
    NativeWindowId::new(hwnd.0 as isize)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{RoutedPlatformEvent, enqueue_and_wake};
    use crate::PlatformEvent;

    #[test]
    fn failed_wake_does_not_leave_an_orphaned_event_queued() {
        let mut events = VecDeque::new();

        assert!(!enqueue_and_wake(
            &mut events,
            RoutedPlatformEvent::broadcast(PlatformEvent::SyncWindows),
            || false,
        ));
        assert!(events.is_empty());
    }
}
