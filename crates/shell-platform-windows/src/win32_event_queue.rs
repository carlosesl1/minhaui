#![deny(unsafe_code)]

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, OnceLock};

use windows::Win32::Foundation::HWND;

use crate::{
    DockPointerPhase, NativeEventTarget, NativeWindowId, PlatformEvent, TopbarPointerPhase,
};

const MAX_EVENT_QUEUE_LEN: usize = 1_024;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CoalesceKind {
    DockMovement,
    TopbarMovement,
    PreviewMovement,
    PopoverMovement,
    AppMenuMovement,
    AppBarRefresh,
    DisplayRefresh,
    DpiRefresh,
    QuickSettingsRefresh,
    ShellObservationRefresh,
    BackgroundAppsRefresh,
    MediaSessionsRefresh,
    WindowSync,
    DockAnimationFrame,
    DockEdgeProbe,
    PreviewFrame,
    ExternalMenuProbe,
    SettingsResize,
}

#[derive(Debug)]
struct QueuedEvent {
    id: u64,
    routed: RoutedPlatformEvent,
}

#[derive(Debug)]
enum QueueMutation {
    Dropped,
    Inserted {
        id: u64,
        evicted: Option<(usize, QueuedEvent)>,
    },
    Replaced {
        id: u64,
        previous: QueuedEvent,
    },
}

impl QueueMutation {
    const fn was_queued(&self) -> bool {
        !matches!(self, Self::Dropped)
    }
}

#[derive(Debug)]
struct EventQueue {
    events: VecDeque<QueuedEvent>,
    capacity: usize,
    next_id: u64,
}

impl EventQueue {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
            next_id: 1,
        }
    }

    fn enqueue(&mut self, event: RoutedPlatformEvent) -> QueueMutation {
        let id = self.take_id();
        let queued = QueuedEvent { id, routed: event };

        if let Some(index) = self.coalescing_index(&queued.routed) {
            let previous = std::mem::replace(&mut self.events[index], queued);
            return QueueMutation::Replaced { id, previous };
        }

        let evicted = if self.events.len() >= self.capacity {
            let coalescible_index = self
                .events
                .iter()
                .position(|entry| coalesce_kind(entry.routed.event()).is_some());

            if coalesce_kind(queued.routed.event()).is_some() && coalescible_index.is_none() {
                return QueueMutation::Dropped;
            }

            let eviction_index = coalescible_index.unwrap_or(0);
            let removed = self.events.remove(eviction_index);
            removed.map(|entry| (eviction_index, entry))
        } else {
            None
        };

        self.events.push_back(queued);
        QueueMutation::Inserted { id, evicted }
    }

    fn pop_front(&mut self) -> Option<RoutedPlatformEvent> {
        self.events.pop_front().map(|entry| entry.routed)
    }

    fn rollback(&mut self, mutation: QueueMutation) {
        match mutation {
            QueueMutation::Dropped => {}
            QueueMutation::Inserted { id, evicted } => {
                let Some(index) = self.events.iter().position(|entry| entry.id == id) else {
                    return;
                };
                let _ = self.events.remove(index);
                if let Some((original_index, event)) = evicted {
                    self.events
                        .insert(original_index.min(self.events.len()), event);
                }
            }
            QueueMutation::Replaced { id, previous } => {
                let Some(index) = self.events.iter().position(|entry| entry.id == id) else {
                    return;
                };
                self.events[index] = previous;
            }
        }
    }

    fn coalescing_index(&self, incoming: &RoutedPlatformEvent) -> Option<usize> {
        let incoming_kind = coalesce_kind(incoming.event())?;
        for (index, queued) in self.events.iter().enumerate().rev() {
            if queued.routed.target() != incoming.target() {
                continue;
            }

            // The newest queued event for this target is a causal barrier. A
            // move after Pressed, for example, must never replace a move that
            // preceded Pressed and thereby execute on the wrong side of it.
            return (coalesce_kind(queued.routed.event()) == Some(incoming_kind)).then_some(index);
        }
        None
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.events.len()
    }
}

fn coalesce_kind(event: &PlatformEvent) -> Option<CoalesceKind> {
    match event {
        PlatformEvent::DockPointer(sample)
            if matches!(
                sample.phase,
                DockPointerPhase::Moved | DockPointerPhase::Dragged
            ) =>
        {
            Some(CoalesceKind::DockMovement)
        }
        PlatformEvent::TopbarPointer(sample) if sample.phase() == TopbarPointerPhase::Moved => {
            Some(CoalesceKind::TopbarMovement)
        }
        PlatformEvent::PreviewPointerMoved(_) => Some(CoalesceKind::PreviewMovement),
        PlatformEvent::PopoverPointerMoved(_) => Some(CoalesceKind::PopoverMovement),
        PlatformEvent::AppMenuPointerMoved(_) => Some(CoalesceKind::AppMenuMovement),
        PlatformEvent::AppBarPositionChanged => Some(CoalesceKind::AppBarRefresh),
        PlatformEvent::DisplayChanged => Some(CoalesceKind::DisplayRefresh),
        PlatformEvent::DpiChanged(_) => Some(CoalesceKind::DpiRefresh),
        PlatformEvent::QuickSettingsRefresh(_) => Some(CoalesceKind::QuickSettingsRefresh),
        PlatformEvent::ShellObservationLoaded(_) => Some(CoalesceKind::ShellObservationRefresh),
        PlatformEvent::BackgroundAppsLoaded(_) => Some(CoalesceKind::BackgroundAppsRefresh),
        PlatformEvent::MediaSessionsChanged(_) => Some(CoalesceKind::MediaSessionsRefresh),
        PlatformEvent::SyncWindows => Some(CoalesceKind::WindowSync),
        PlatformEvent::DockAnimationFrame => Some(CoalesceKind::DockAnimationFrame),
        PlatformEvent::DockEdgeProbe => Some(CoalesceKind::DockEdgeProbe),
        PlatformEvent::PreviewTimer => Some(CoalesceKind::PreviewFrame),
        PlatformEvent::ExternalMenuTimer => Some(CoalesceKind::ExternalMenuProbe),
        PlatformEvent::SettingsResized => Some(CoalesceKind::SettingsResize),
        _ => None,
    }
}

static EVENT_QUEUE: OnceLock<Mutex<EventQueue>> = OnceLock::new();
static DRAGGING_WINDOWS: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();

pub(super) fn queue_event(event: RoutedPlatformEvent) {
    let queue =
        EVENT_QUEUE.get_or_init(|| Mutex::new(EventQueue::with_capacity(MAX_EVENT_QUEUE_LEN)));
    let _ = lock_recover(queue).enqueue(event);
}

pub(super) fn queue_event_with_wake(
    event: RoutedPlatformEvent,
    wake: impl FnOnce() -> bool,
) -> bool {
    let queue =
        EVENT_QUEUE.get_or_init(|| Mutex::new(EventQueue::with_capacity(MAX_EVENT_QUEUE_LEN)));
    enqueue_and_wake(queue, event, wake)
}

fn enqueue_and_wake(
    queue: &Mutex<EventQueue>,
    event: RoutedPlatformEvent,
    wake: impl FnOnce() -> bool,
) -> bool {
    let mutation = lock_recover(queue).enqueue(event);
    if !mutation.was_queued() {
        return false;
    }

    // PostMessage/PostThreadMessage must run without the queue lock held. Apart
    // from avoiding worker contention, this permits a synchronous test wake and
    // future wake adapters to inspect or drain the queue without deadlocking.
    if wake() {
        return true;
    }

    lock_recover(queue).rollback(mutation);
    false
}

pub(super) fn next_event() -> Option<RoutedPlatformEvent> {
    EVENT_QUEUE
        .get()
        .and_then(|queue| lock_recover(queue).pop_front())
}

pub(super) fn set_dragging(hwnd: HWND, dragging: bool) {
    let raw = hwnd.0 as isize;
    let windows = DRAGGING_WINDOWS.get_or_init(|| Mutex::new(Vec::new()));
    let mut values = lock_recover(windows);
    if dragging {
        if !values.contains(&raw) {
            values.push(raw);
        }
    } else {
        values.retain(|value| *value != raw);
    }
}

pub(super) fn is_dragging(hwnd: HWND) -> bool {
    let raw = hwnd.0 as isize;
    DRAGGING_WINDOWS
        .get()
        .is_some_and(|windows| lock_recover(windows).contains(&raw))
}

pub(super) fn native_window_id(hwnd: HWND) -> NativeWindowId {
    NativeWindowId::new(hwnd.0 as isize)
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            mutex.clear_poison();
            poisoned.into_inner()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
    use std::sync::Mutex;

    use shell_renderer::DipPoint;

    use super::{EventQueue, RoutedPlatformEvent, enqueue_and_wake, lock_recover};
    use crate::{DockKey, DockPointerPhase, DockPointerSample, NativeWindowId, PlatformEvent};

    fn window_event(window: isize, event: PlatformEvent) -> RoutedPlatformEvent {
        RoutedPlatformEvent::window_id(NativeWindowId::new(window), event)
    }

    fn moved(window: isize, x: f32) -> RoutedPlatformEvent {
        window_event(
            window,
            PlatformEvent::DockPointer(DockPointerSample::new(
                DockPointerPhase::Moved,
                DipPoint::new(x, 0.0),
            )),
        )
    }

    fn queued_events(queue: &mut EventQueue) -> Vec<RoutedPlatformEvent> {
        std::iter::from_fn(|| queue.pop_front()).collect()
    }

    #[test]
    fn failed_wake_does_not_leave_an_orphaned_event_queued() {
        let queue = Mutex::new(EventQueue::with_capacity(4));

        assert!(!enqueue_and_wake(
            &queue,
            RoutedPlatformEvent::broadcast(PlatformEvent::SyncWindows),
            || false,
        ));
        assert_eq!(lock_recover(&queue).len(), 0);
    }

    #[test]
    fn wake_callback_runs_without_the_queue_lock() {
        let queue = Mutex::new(EventQueue::with_capacity(4));

        assert!(enqueue_and_wake(
            &queue,
            RoutedPlatformEvent::broadcast(PlatformEvent::SyncWindows),
            || lock_recover(&queue).len() == 1,
        ));
    }

    #[test]
    fn failed_wake_restores_the_event_replaced_by_coalescing() {
        let queue = Mutex::new(EventQueue::with_capacity(4));
        let _ = lock_recover(&queue).enqueue(moved(7, 1.0));

        assert!(!enqueue_and_wake(&queue, moved(7, 2.0), || false));

        let event = lock_recover(&queue).pop_front();
        assert_eq!(event, Some(moved(7, 1.0)));
    }

    #[test]
    fn movement_is_latest_value_per_target() {
        let mut queue = EventQueue::with_capacity(8);
        let _ = queue.enqueue(moved(1, 1.0));
        let _ = queue.enqueue(moved(2, 2.0));
        let _ = queue.enqueue(moved(1, 3.0));
        let _ = queue.enqueue(moved(2, 4.0));

        assert_eq!(
            queued_events(&mut queue),
            vec![moved(1, 3.0), moved(2, 4.0)]
        );
    }

    #[test]
    fn discrete_actions_are_ordered_and_form_coalescing_barriers() {
        let mut queue = EventQueue::with_capacity(8);
        let pressed = window_event(
            1,
            PlatformEvent::DockPointer(DockPointerSample::new(
                DockPointerPhase::Pressed,
                DipPoint::new(2.0, 0.0),
            )),
        );
        let released = window_event(
            1,
            PlatformEvent::DockPointer(DockPointerSample::new(
                DockPointerPhase::Released,
                DipPoint::new(5.0, 0.0),
            )),
        );

        let _ = queue.enqueue(moved(1, 1.0));
        let _ = queue.enqueue(pressed.clone());
        let _ = queue.enqueue(moved(1, 3.0));
        let _ = queue.enqueue(moved(1, 4.0));
        let _ = queue.enqueue(released.clone());

        assert_eq!(
            queued_events(&mut queue),
            vec![moved(1, 1.0), pressed, moved(1, 4.0), released]
        );
    }

    #[test]
    fn settings_scroll_deltas_are_discrete_and_preserved() {
        let mut queue = EventQueue::with_capacity(8);
        let down = window_event(1, PlatformEvent::SettingsScroll(1));
        let up = window_event(1, PlatformEvent::SettingsScroll(-1));

        let _ = queue.enqueue(down.clone());
        let _ = queue.enqueue(down.clone());
        let _ = queue.enqueue(up.clone());

        assert_eq!(queued_events(&mut queue), vec![down.clone(), down, up]);
    }

    #[test]
    fn refreshes_and_frames_are_latest_value_without_cross_kind_reordering() {
        let mut queue = EventQueue::with_capacity(8);
        let sync = window_event(1, PlatformEvent::SyncWindows);
        let frame = window_event(1, PlatformEvent::DockAnimationFrame);

        let _ = queue.enqueue(sync.clone());
        let _ = queue.enqueue(sync.clone());
        let _ = queue.enqueue(frame.clone());
        let _ = queue.enqueue(frame.clone());
        let _ = queue.enqueue(sync.clone());

        assert_eq!(queued_events(&mut queue), vec![sync.clone(), frame, sync]);
    }

    #[test]
    fn movement_flood_cannot_evict_discrete_actions_at_the_hard_cap() {
        let mut queue = EventQueue::with_capacity(3);
        let first = window_event(1, PlatformEvent::DockKey(DockKey::Next));
        let second = window_event(1, PlatformEvent::DockKey(DockKey::Previous));
        let last = window_event(1, PlatformEvent::DockKey(DockKey::Activate));

        let _ = queue.enqueue(first.clone());
        let _ = queue.enqueue(second.clone());
        for x in 0..10_000 {
            let _ = queue.enqueue(moved(2, x as f32));
        }
        let _ = queue.enqueue(last.clone());

        assert_eq!(queue.len(), 3);
        assert_eq!(queued_events(&mut queue), vec![first, second, last]);
    }

    #[test]
    fn all_discrete_overflow_remains_bounded_and_keeps_survivor_order() {
        let mut queue = EventQueue::with_capacity(2);
        let first = window_event(1, PlatformEvent::DockKey(DockKey::Next));
        let second = window_event(1, PlatformEvent::DockKey(DockKey::Previous));
        let third = window_event(1, PlatformEvent::DockKey(DockKey::Activate));

        let _ = queue.enqueue(first);
        let _ = queue.enqueue(second.clone());
        let _ = queue.enqueue(third.clone());

        assert_eq!(queue.len(), 2);
        assert_eq!(queued_events(&mut queue), vec![second, third]);
    }

    #[test]
    fn poisoned_mutex_is_cleared_and_the_queue_remains_usable() {
        let queue = Mutex::new(EventQueue::with_capacity(2));
        let poison_result = catch_unwind(AssertUnwindSafe(|| {
            let guard = match queue.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let _keep_guard_alive = guard;
            resume_unwind(Box::new("intentional test poison"));
        }));
        assert!(poison_result.is_err());
        assert!(queue.is_poisoned());

        let _ = lock_recover(&queue).enqueue(moved(1, 1.0));

        assert!(!queue.is_poisoned());
        assert_eq!(lock_recover(&queue).len(), 1);
    }
}
