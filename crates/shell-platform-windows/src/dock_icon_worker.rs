#![cfg_attr(not(windows), deny(unsafe_code))]

use shell_core::{DockItemId, WindowId};

#[cfg(any(windows, test))]
use std::collections::HashMap;
#[cfg(windows)]
use std::collections::VecDeque;
#[cfg(windows)]
use std::sync::{Arc, Condvar, Mutex, OnceLock};
#[cfg(windows)]
use std::thread::{self, JoinHandle};
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
use windows::Win32::Foundation::{E_FAIL, HWND, LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

#[cfg(windows)]
use crate::win32_event_queue::{RoutedPlatformEvent, queue_event_with_wake};
#[cfg(windows)]
use crate::{NativeWindowId, PlatformEvent};

#[cfg(windows)]
pub(super) const DOCK_ICON_WAKE_MESSAGE: u32 = WM_APP + 0x6B;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DockIconCandidate {
    item: DockItemId,
    fingerprint: String,
    window: Option<WindowId>,
    launch_target: Option<String>,
}

impl DockIconCandidate {
    pub(crate) fn new(
        item: DockItemId,
        fingerprint: String,
        window: Option<WindowId>,
        launch_target: Option<String>,
    ) -> Self {
        Self {
            item,
            fingerprint,
            window,
            launch_target,
        }
    }

    #[must_use]
    pub(crate) const fn item(&self) -> DockItemId {
        self.item
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DockIconResolutionRequest {
    generation: u64,
    candidates: Vec<DockIconCandidate>,
}

impl DockIconResolutionRequest {
    pub(crate) fn new(generation: u64, candidates: Vec<DockIconCandidate>) -> Self {
        Self {
            generation,
            candidates,
        }
    }

    #[must_use]
    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    fn fallback_result(&self) -> DockIconResolutionResult {
        DockIconResolutionResult::new(
            self.generation,
            self.candidates
                .iter()
                .map(|candidate| {
                    ResolvedDockIcon::new(candidate.item, candidate.fingerprint.clone(), None)
                })
                .collect(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedDockIcon {
    item: DockItemId,
    fingerprint: String,
    source: Option<String>,
}

impl ResolvedDockIcon {
    pub(crate) fn new(item: DockItemId, fingerprint: String, source: Option<String>) -> Self {
        Self {
            item,
            fingerprint,
            source,
        }
    }

    #[must_use]
    pub(crate) const fn item(&self) -> DockItemId {
        self.item
    }

    #[must_use]
    pub(crate) fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    #[must_use]
    pub(crate) fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DockIconResolutionResult {
    generation: u64,
    icons: Vec<ResolvedDockIcon>,
}

impl DockIconResolutionResult {
    pub(crate) fn new(generation: u64, icons: Vec<ResolvedDockIcon>) -> Self {
        Self { generation, icons }
    }

    #[must_use]
    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub(crate) fn icons(&self) -> &[ResolvedDockIcon] {
        &self.icons
    }
}

#[cfg(any(windows, test))]
trait DockIconQuery {
    fn window_icon_source(&self, window: WindowId) -> Option<String>;
    fn resolve_icon_source(&self, source: &str) -> Option<String>;
}

#[cfg(any(windows, test))]
fn resolve_candidates(
    query: &impl DockIconQuery,
    request: &DockIconResolutionRequest,
    is_current: &dyn Fn() -> bool,
) -> Option<DockIconResolutionResult> {
    let mut windows = HashMap::<WindowId, Option<String>>::new();
    let mut sources = HashMap::<String, Option<String>>::new();
    let mut icons = Vec::with_capacity(request.candidates.len());

    for candidate in &request.candidates {
        if !is_current() {
            return None;
        }
        let window_source = candidate.window.and_then(|window| {
            if let Some(source) = windows.get(&window) {
                return source.clone();
            }
            let source = query.window_icon_source(window);
            windows.insert(window, source.clone());
            source
        });
        if !is_current() {
            return None;
        }
        let preferred = window_source.or_else(|| candidate.launch_target.clone());
        let source = preferred.map(|preferred| {
            if let Some(resolved) = sources.get(&preferred) {
                return resolved.clone().unwrap_or(preferred);
            }
            let resolved = query.resolve_icon_source(&preferred);
            sources.insert(preferred.clone(), resolved.clone());
            resolved.unwrap_or(preferred)
        });
        if !is_current() {
            return None;
        }
        icons.push(ResolvedDockIcon::new(
            candidate.item,
            candidate.fingerprint.clone(),
            source,
        ));
    }

    Some(DockIconResolutionResult::new(request.generation, icons))
}

#[cfg(windows)]
trait DockIconSource {
    fn resolve(
        &self,
        request: &DockIconResolutionRequest,
        is_current: &dyn Fn() -> bool,
    ) -> Option<DockIconResolutionResult>;
}

#[cfg(windows)]
struct NativeDockIconSource;

#[cfg(windows)]
impl DockIconSource for NativeDockIconSource {
    fn resolve(
        &self,
        request: &DockIconResolutionRequest,
        is_current: &dyn Fn() -> bool,
    ) -> Option<DockIconResolutionResult> {
        let _com = ComThreadGuard::initialize().ok()?;
        resolve_candidates(&WindowsDockIconQuery, request, is_current)
    }
}

#[cfg(windows)]
struct WindowsDockIconQuery;

#[cfg(windows)]
impl DockIconQuery for WindowsDockIconQuery {
    fn window_icon_source(&self, window: WindowId) -> Option<String> {
        crate::win32_app_identity::window_icon_source(window)
    }

    fn resolve_icon_source(&self, source: &str) -> Option<String> {
        crate::win32_app_identity::resolve_icon_source(source)
    }
}

#[cfg(windows)]
struct ComThreadGuard;

#[cfg(windows)]
impl ComThreadGuard {
    fn initialize() -> windows::core::Result<Self> {
        // SAFETY: the Dock icon worker exclusively owns this thread and balances
        // a successful apartment initialization in this guard's Drop.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok()?;
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for ComThreadGuard {
    fn drop(&mut self) {
        // SAFETY: balances the successful CoInitializeEx call on this worker.
        unsafe { CoUninitialize() };
    }
}

#[cfg(windows)]
#[derive(Clone)]
struct DockIconWorkItem {
    request: DockIconResolutionRequest,
    wake_window: NativeWindowId,
    request_id: u64,
}

#[cfg(windows)]
const MAX_DOCK_ICON_TARGETS: usize = 32;
#[cfg(windows)]
const DELIVERY_RETRY_INITIAL: Duration = Duration::from_millis(25);
#[cfg(windows)]
const DELIVERY_RETRY_MAX: Duration = Duration::from_millis(800);

#[cfg(windows)]
struct DockIconDelivery {
    result: DockIconResolutionResult,
    wake_window: NativeWindowId,
    request_id: u64,
    failures: u8,
    ready_at: Instant,
}

#[cfg(windows)]
impl DockIconDelivery {
    fn new(
        result: DockIconResolutionResult,
        wake_window: NativeWindowId,
        request_id: u64,
        now: Instant,
    ) -> Self {
        Self {
            result,
            wake_window,
            request_id,
            failures: 0,
            ready_at: now,
        }
    }
}

#[cfg(windows)]
struct DockIconQueueState {
    pending: HashMap<isize, DockIconWorkItem>,
    order: VecDeque<isize>,
    ready: HashMap<isize, DockIconDelivery>,
    current: HashMap<isize, u64>,
    next_request_id: u64,
    stopping: bool,
}

#[cfg(windows)]
impl DockIconQueueState {
    fn new() -> Self {
        Self {
            pending: HashMap::new(),
            order: VecDeque::new(),
            ready: HashMap::new(),
            current: HashMap::new(),
            next_request_id: 1,
            stopping: false,
        }
    }

    fn enqueue(&mut self, mut work: DockIconWorkItem) -> bool {
        let target = work.wake_window.value();
        if !self.current.contains_key(&target) && self.current.len() >= MAX_DOCK_ICON_TARGETS {
            return false;
        }
        work.request_id = self.take_request_id();
        self.current.insert(target, work.request_id);
        self.ready.remove(&target);
        if self.pending.insert(target, work).is_none() {
            self.order.push_back(target);
        }
        true
    }

    fn pop_next(&mut self) -> Option<DockIconWorkItem> {
        while let Some(target) = self.order.pop_front() {
            if let Some(work) = self.pending.remove(&target) {
                return Some(work);
            }
        }
        None
    }

    fn is_current(&self, target: isize, request_id: u64) -> bool {
        self.current.get(&target) == Some(&request_id)
    }

    fn complete(&mut self, target: isize, request_id: u64) {
        if self.is_current(target, request_id)
            && !self.pending.contains_key(&target)
            && !self.ready.contains_key(&target)
        {
            self.current.remove(&target);
        }
    }

    fn finish_delivery(&mut self, mut delivery: DockIconDelivery, delivered: bool, now: Instant) {
        let target = delivery.wake_window.value();
        if !self.is_current(target, delivery.request_id) {
            return;
        }
        if delivered {
            self.complete(target, delivery.request_id);
            return;
        }
        delivery.failures = delivery.failures.saturating_add(1);
        delivery.ready_at = now + delivery_retry_delay(delivery.failures);
        self.ready.insert(target, delivery);
    }

    fn pop_ready(&mut self, now: Instant) -> Option<DockIconDelivery> {
        let target = self
            .ready
            .iter()
            .filter(|(_, delivery)| delivery.ready_at <= now)
            .min_by(|(left_target, left), (right_target, right)| {
                left.ready_at
                    .cmp(&right.ready_at)
                    .then_with(|| left_target.cmp(right_target))
            })
            .map(|(target, _)| *target)?;
        self.ready.remove(&target)
    }

    fn next_retry_delay(&self, now: Instant) -> Option<Duration> {
        self.ready
            .values()
            .map(|delivery| delivery.ready_at.saturating_duration_since(now))
            .min()
    }

    fn cancel(&mut self, target: isize) {
        self.pending.remove(&target);
        self.order.retain(|queued| *queued != target);
        self.ready.remove(&target);
        self.current.remove(&target);
    }

    fn take_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        request_id
    }
}

#[cfg(windows)]
fn delivery_retry_delay(failures: u8) -> Duration {
    let exponent = failures.saturating_sub(1).min(5);
    DELIVERY_RETRY_INITIAL
        .saturating_mul(1_u32 << exponent)
        .min(DELIVERY_RETRY_MAX)
}

#[cfg(windows)]
struct DockIconWorkerShared {
    state: Mutex<DockIconQueueState>,
    wake: Condvar,
}

#[cfg(windows)]
pub(crate) struct DockIconWorker {
    shared: Arc<DockIconWorkerShared>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(windows)]
impl DockIconWorker {
    fn with_source<S, F>(source: S, deliver: F) -> std::io::Result<Self>
    where
        S: DockIconSource + Send + 'static,
        F: FnMut(DockIconResolutionResult, NativeWindowId) -> bool + Send + 'static,
    {
        let shared = Arc::new(DockIconWorkerShared {
            state: Mutex::new(DockIconQueueState::new()),
            wake: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("dock-icon-source".to_owned())
            .spawn(move || dock_icon_worker_loop(&worker_shared, source, deliver))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    pub(crate) fn request(
        &self,
        request: DockIconResolutionRequest,
        wake_window: NativeWindowId,
    ) -> bool {
        let mut state = lock_recover(&self.shared.state);
        if state.stopping {
            return false;
        }
        let accepted = state.enqueue(DockIconWorkItem {
            request,
            wake_window,
            request_id: 0,
        });
        if accepted {
            self.shared.wake.notify_one();
        }
        accepted
    }

    pub(crate) fn cancel_target(&self, wake_window: NativeWindowId) {
        lock_recover(&self.shared.state).cancel(wake_window.value());
        self.shared.wake.notify_one();
    }

    pub(super) fn new() -> windows::core::Result<Self> {
        Self::with_source(NativeDockIconSource, |result, wake_window| {
            let event = RoutedPlatformEvent::window_id(
                wake_window,
                PlatformEvent::DockIconSourcesLoaded(result),
            );
            queue_event_with_wake(event, || wake_owner_window(wake_window))
        })
        .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))
    }

    pub(super) fn shared() -> Option<Arc<Self>> {
        static INSTANCE: OnceLock<Option<Arc<DockIconWorker>>> = OnceLock::new();
        INSTANCE
            .get_or_init(|| Self::new().ok().map(Arc::new))
            .as_ref()
            .map(Arc::clone)
    }
}

#[cfg(windows)]
impl Drop for DockIconWorker {
    fn drop(&mut self) {
        {
            let mut state = lock_recover(&self.shared.state);
            state.stopping = true;
            state.pending.clear();
            state.order.clear();
            state.ready.clear();
            state.current.clear();
            self.shared.wake.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(windows)]
fn dock_icon_worker_loop<S, F>(shared: &DockIconWorkerShared, source: S, mut deliver: F)
where
    S: DockIconSource,
    F: FnMut(DockIconResolutionResult, NativeWindowId) -> bool,
{
    loop {
        let action = {
            let mut state = lock_recover(&shared.state);
            loop {
                if state.stopping {
                    return;
                }
                let now = Instant::now();
                if let Some(delivery) = state.pop_ready(now) {
                    break DockIconWorkerAction::Deliver(delivery);
                }
                if let Some(work) = state.pop_next() {
                    break DockIconWorkerAction::Resolve(work);
                }
                state = if let Some(delay) = state.next_retry_delay(now) {
                    shared
                        .wake
                        .wait_timeout(state, delay)
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0
                } else {
                    shared
                        .wake
                        .wait(state)
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                };
            }
        };
        match action {
            DockIconWorkerAction::Resolve(work) => {
                let target = work.wake_window.value();
                let request_id = work.request_id;
                let is_current = || lock_recover(&shared.state).is_current(target, request_id);
                let resolved = source.resolve(&work.request, &is_current);
                if is_current() {
                    let result = resolved.unwrap_or_else(|| work.request.fallback_result());
                    attempt_delivery(
                        shared,
                        DockIconDelivery::new(result, work.wake_window, request_id, Instant::now()),
                        &mut deliver,
                    );
                } else {
                    lock_recover(&shared.state).complete(target, request_id);
                }
            }
            DockIconWorkerAction::Deliver(delivery) => {
                attempt_delivery(shared, delivery, &mut deliver);
            }
        }
    }
}

#[cfg(windows)]
enum DockIconWorkerAction {
    Resolve(DockIconWorkItem),
    Deliver(DockIconDelivery),
}

#[cfg(windows)]
fn attempt_delivery(
    shared: &DockIconWorkerShared,
    delivery: DockIconDelivery,
    deliver: &mut impl FnMut(DockIconResolutionResult, NativeWindowId) -> bool,
) {
    let target = delivery.wake_window.value();
    if !lock_recover(&shared.state).is_current(target, delivery.request_id) {
        return;
    }
    let delivered = deliver(delivery.result.clone(), delivery.wake_window);
    lock_recover(&shared.state).finish_delivery(delivery, delivered, Instant::now());
}

#[cfg(windows)]
fn lock_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(windows)]
fn wake_owner_window(window: NativeWindowId) -> bool {
    let hwnd = HWND(window.value() as *mut core::ffi::c_void);
    // SAFETY: the copied owner HWND is used only for a pointer-free asynchronous
    // wake. The routed queue owns all result data.
    unsafe { PostMessageW(Some(hwnd), DOCK_ICON_WAKE_MESSAGE, WPARAM(0), LPARAM(0)).is_ok() }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    #[cfg(windows)]
    use std::sync::{Condvar, Mutex};
    #[cfg(windows)]
    use std::time::Instant;

    use shell_core::{DockItemId, WindowId};

    #[cfg(windows)]
    use super::{
        DELIVERY_RETRY_INITIAL, DockIconDelivery, DockIconQueueState, DockIconWorkItem,
        DockIconWorkerShared, MAX_DOCK_ICON_TARGETS, attempt_delivery, lock_recover,
    };
    use super::{DockIconCandidate, DockIconQuery, DockIconResolutionRequest, resolve_candidates};
    #[cfg(windows)]
    use crate::NativeWindowId;

    struct CountingQuery {
        window_calls: Cell<usize>,
        source_calls: Cell<usize>,
    }

    impl DockIconQuery for CountingQuery {
        fn window_icon_source(&self, _window: WindowId) -> Option<String> {
            self.window_calls.set(self.window_calls.get() + 1);
            Some(r"C:\Apps\Browser\browser.exe".to_owned())
        }

        fn resolve_icon_source(&self, source: &str) -> Option<String> {
            self.source_calls.set(self.source_calls.get() + 1);
            Some(source.to_owned())
        }
    }

    #[test]
    fn batch_deduplicates_window_and_canonical_source_queries() {
        let query = CountingQuery {
            window_calls: Cell::new(0),
            source_calls: Cell::new(0),
        };
        let window = Some(WindowId::new(42));
        let request = DockIconResolutionRequest::new(
            7,
            vec![
                DockIconCandidate::new(
                    DockItemId::new(1),
                    "one".to_owned(),
                    window,
                    Some("browser.exe".to_owned()),
                ),
                DockIconCandidate::new(
                    DockItemId::new(2),
                    "two".to_owned(),
                    window,
                    Some("browser.exe".to_owned()),
                ),
            ],
        );

        let result = resolve_candidates(&query, &request, &|| true)
            .unwrap_or_else(|| panic!("current request was unexpectedly cancelled"));

        assert_eq!(query.window_calls.get(), 1);
        assert_eq!(query.source_calls.get(), 1);
        assert_eq!(result.icons().len(), 2);
        assert!(
            result
                .icons()
                .iter()
                .all(|icon| { icon.source() == Some(r"C:\Apps\Browser\browser.exe") })
        );
    }

    #[test]
    fn superseded_batch_stops_before_more_blocking_queries() {
        let query = CountingQuery {
            window_calls: Cell::new(0),
            source_calls: Cell::new(0),
        };
        let request = DockIconResolutionRequest::new(
            9,
            vec![DockIconCandidate::new(
                DockItemId::new(1),
                "one".to_owned(),
                Some(WindowId::new(42)),
                None,
            )],
        );
        let checks = Cell::new(0);

        let result = resolve_candidates(&query, &request, &|| {
            let check = checks.get();
            checks.set(check + 1);
            check == 0
        });

        assert!(result.is_none());
        assert_eq!(query.window_calls.get(), 1);
        assert_eq!(query.source_calls.get(), 0);
    }

    #[test]
    fn source_failure_has_a_complete_fallback_result_for_the_current_request() {
        let request = DockIconResolutionRequest::new(
            11,
            vec![DockIconCandidate::new(
                DockItemId::new(7),
                "fingerprint".to_owned(),
                None,
                Some("browser.exe".to_owned()),
            )],
        );

        let fallback = request.fallback_result();

        assert_eq!(fallback.generation(), 11);
        assert_eq!(fallback.icons().len(), 1);
        assert_eq!(fallback.icons()[0].item(), DockItemId::new(7));
        assert_eq!(fallback.icons()[0].fingerprint(), "fingerprint");
        assert_eq!(fallback.icons()[0].source(), None);
    }

    #[test]
    #[cfg(windows)]
    fn keyed_queue_coalesces_per_target_without_overwriting_other_monitors() {
        let mut queue = DockIconQueueState::new();

        assert!(queue.enqueue(work(101, 1)));
        assert!(queue.enqueue(work(202, 1)));
        assert!(queue.enqueue(work(101, 2)));

        let first = queue
            .pop_next()
            .unwrap_or_else(|| panic!("first monitor request missing"));
        let second = queue
            .pop_next()
            .unwrap_or_else(|| panic!("second monitor request missing"));
        assert_eq!(first.wake_window, NativeWindowId::new(101));
        assert_eq!(first.request.generation(), 2);
        assert_eq!(second.wake_window, NativeWindowId::new(202));
        assert_eq!(second.request.generation(), 1);
        assert!(queue.pop_next().is_none());

        queue.complete(first.wake_window.value(), first.request_id);
        queue.complete(second.wake_window.value(), second.request_id);
        assert!(queue.current.is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn failed_delivery_retries_the_ready_result_then_completes_without_new_resolution() {
        let mut queue = DockIconQueueState::new();
        assert!(queue.enqueue(work(101, 7)));
        let work = queue
            .pop_next()
            .unwrap_or_else(|| panic!("resolution work missing"));
        let started = Instant::now();
        let delivery = DockIconDelivery::new(
            work.request.fallback_result(),
            work.wake_window,
            work.request_id,
            started,
        );
        let shared = DockIconWorkerShared {
            state: Mutex::new(queue),
            wake: Condvar::new(),
        };
        let attempts = Cell::new(0);
        let mut deliver = |_result, _window| {
            let attempt = attempts.get();
            attempts.set(attempt + 1);
            attempt > 0
        };

        attempt_delivery(&shared, delivery, &mut deliver);

        let retry_at = {
            let mut state = lock_recover(&shared.state);
            assert_eq!(state.ready.len(), 1);
            assert!(state.next_retry_delay(started) >= Some(DELIVERY_RETRY_INITIAL));
            assert!(state.pop_ready(started).is_none());
            state
                .ready
                .get(&101)
                .map(|retry| retry.ready_at)
                .unwrap_or_else(|| panic!("ready retry missing"))
        };
        let retry = lock_recover(&shared.state)
            .pop_ready(retry_at)
            .unwrap_or_else(|| panic!("ready result was not retried"));
        assert_eq!(retry.failures, 1);
        assert_eq!(retry.result.generation(), 7);

        attempt_delivery(&shared, retry, &mut deliver);

        let state = lock_recover(&shared.state);
        assert_eq!(attempts.get(), 2);
        assert!(state.ready.is_empty());
        assert!(state.current.is_empty());
        assert!(state.pending.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn cancelling_a_target_bounds_hotplug_state_without_affecting_other_monitors() {
        let mut queue = DockIconQueueState::new();
        assert!(queue.enqueue(work(101, 1)));
        assert!(queue.enqueue(work(202, 1)));
        let first = queue
            .pop_next()
            .unwrap_or_else(|| panic!("first monitor request missing"));
        let now = Instant::now();
        let delivery = DockIconDelivery::new(
            first.request.fallback_result(),
            first.wake_window,
            first.request_id,
            now,
        );
        queue.finish_delivery(delivery, false, now);

        queue.cancel(101);

        assert!(!queue.current.contains_key(&101));
        assert!(!queue.pending.contains_key(&101));
        assert!(!queue.ready.contains_key(&101));
        let remaining = queue
            .pop_next()
            .unwrap_or_else(|| panic!("other monitor was removed"));
        assert_eq!(remaining.wake_window, NativeWindowId::new(202));
    }

    #[cfg(windows)]
    #[test]
    fn keyed_queue_rejects_new_targets_at_its_bound_but_keeps_latest_existing_request() {
        let mut queue = DockIconQueueState::new();
        for target in 1..=MAX_DOCK_ICON_TARGETS {
            assert!(queue.enqueue(work(target as isize, 1)));
        }

        assert!(!queue.enqueue(work((MAX_DOCK_ICON_TARGETS + 1) as isize, 1)));
        assert!(queue.enqueue(work(1, 2)));
        assert_eq!(queue.current.len(), MAX_DOCK_ICON_TARGETS);
        assert_eq!(queue.pending.len(), MAX_DOCK_ICON_TARGETS);
        assert_eq!(queue.order.len(), MAX_DOCK_ICON_TARGETS);
    }

    #[cfg(windows)]
    fn work(target: isize, generation: u64) -> DockIconWorkItem {
        DockIconWorkItem {
            request: DockIconResolutionRequest::new(generation, Vec::new()),
            wake_window: NativeWindowId::new(target),
            request_id: 0,
        }
    }
}
