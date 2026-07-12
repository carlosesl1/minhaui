#![deny(unsafe_code)]

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use shell_renderer::DipPoint;
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

    pub(super) const fn target(&self) -> NativeEventTarget {
        self.target
    }

    pub(super) fn into_event(self) -> PlatformEvent {
        self.event
    }
}

static EVENT_QUEUE: OnceLock<Mutex<VecDeque<RoutedPlatformEvent>>> = OnceLock::new();
static LAST_CONTEXT_POINT: OnceLock<Mutex<Option<DipPoint>>> = OnceLock::new();
static DRAGGING_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();

pub(super) fn queue_event(event: RoutedPlatformEvent) {
    let queue = EVENT_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Ok(mut events) = queue.lock() {
        events.push_back(event);
    }
}

pub(super) fn next_event() -> Option<RoutedPlatformEvent> {
    EVENT_QUEUE
        .get()
        .and_then(|queue| queue.lock().ok()?.pop_front())
}

pub(super) fn set_last_context_point(point: DipPoint) {
    let state = LAST_CONTEXT_POINT.get_or_init(|| Mutex::new(None));
    if let Ok(mut value) = state.lock() {
        *value = Some(point);
    }
}

pub(super) fn last_context_point() -> Option<DipPoint> {
    LAST_CONTEXT_POINT
        .get()
        .and_then(|state| state.lock().ok().and_then(|value| *value))
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
