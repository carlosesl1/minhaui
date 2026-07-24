use std::collections::HashMap;

use crate::native_event_route::NativeWindowId;

/// `WM_CONTEXTMENU`, as defined by Win32's window-message contract.
pub(crate) const WM_CONTEXTMENU: u32 = 0x007b;
/// `WM_RBUTTONDOWN`, as defined by Win32's window-message contract.
pub(crate) const WM_RBUTTONDOWN: u32 = 0x0204;
/// `WM_RBUTTONUP`, as defined by Win32's window-message contract.
pub(crate) const WM_RBUTTONUP: u32 = 0x0205;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TrayScreenPoint {
    x: i32,
    y: i32,
}

impl TrayScreenPoint {
    #[must_use]
    pub(crate) const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub(crate) const fn x(self) -> i32 {
        self.x
    }

    #[must_use]
    pub(crate) const fn y(self) -> i32 {
        self.y
    }
}

impl From<(i32, i32)> for TrayScreenPoint {
    fn from((x, y): (i32, i32)) -> Self {
        Self::new(x, y)
    }
}

/// The three native callback protocols supported by the shell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayActivationStrategy {
    VersionAware,
    Legacy,
    Combined,
}

/// Opaque token identifying one pending user activation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TrayActivationId(u64);

/// A scalar callback envelope suitable for a non-blocking native post.
///
/// The `message` scalar is the application-defined callback message.  The
/// two-argument constructors intentionally leave it at zero so they remain
/// useful for pure strategy tests; callback-aware constructors fill it for the
/// production adapter without introducing a platform type here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TrayMessage {
    message: u32,
    wparam: usize,
    lparam: isize,
}

impl TrayMessage {
    #[must_use]
    pub(crate) fn modern_context<P>(icon_id: u32, point: P) -> Self
    where
        P: Into<TrayScreenPoint>,
    {
        Self::modern_context_with_callback(0, icon_id, point)
    }

    #[must_use]
    pub(crate) fn modern_context_with_callback<P>(
        callback_message: u32,
        icon_id: u32,
        point: P,
    ) -> Self
    where
        P: Into<TrayScreenPoint>,
    {
        let point = point.into();
        let x = pack_signed_word(point.x());
        let y = pack_signed_word(point.y());
        let wparam = (x as usize) | ((y as usize) << 16);
        let lparam = pack_context_lparam(icon_id);
        Self {
            message: callback_message,
            wparam,
            lparam,
        }
    }

    #[must_use]
    pub(crate) const fn legacy(icon_id: u32, event: u32) -> Self {
        Self::legacy_with_callback(0, icon_id, event)
    }

    #[must_use]
    pub(crate) const fn legacy_with_callback(
        callback_message: u32,
        icon_id: u32,
        event: u32,
    ) -> Self {
        Self {
            message: callback_message,
            wparam: icon_id as usize,
            lparam: event as isize,
        }
    }

    #[must_use]
    pub(crate) const fn message(self) -> u32 {
        self.message
    }

    #[must_use]
    pub(crate) const fn wparam(self) -> usize {
        self.wparam
    }

    #[must_use]
    pub(crate) const fn lparam(self) -> isize {
        self.lparam
    }

    #[allow(
        dead_code,
        reason = "callback envelope customization is consumed by native adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn with_callback_message(self, callback_message: u32) -> Self {
        Self {
            message: callback_message,
            ..self
        }
    }
}

/// Posts one callback envelope without waiting for the owner process.
pub(crate) trait TrayCallbackSink {
    fn post(&mut self, target: NativeWindowId, message: TrayMessage) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayActivationRequest {
    executable: String,
    target: NativeWindowId,
    icon_id: u32,
    callback_message: u32,
    version: u32,
    point: TrayScreenPoint,
}

impl TrayActivationRequest {
    #[must_use]
    pub(crate) fn new<P>(
        executable: impl Into<String>,
        target: NativeWindowId,
        icon_id: u32,
        callback_message: u32,
        version: u32,
        point: P,
    ) -> Self
    where
        P: Into<TrayScreenPoint>,
    {
        Self {
            executable: executable.into(),
            target,
            icon_id,
            callback_message,
            version,
            point: point.into(),
        }
    }

    #[allow(
        dead_code,
        reason = "tuple convenience is consumed by native adapter incrementally"
    )]
    #[must_use]
    pub(crate) fn from_tuple(
        executable: impl Into<String>,
        target: NativeWindowId,
        icon_id: u32,
        callback_message: u32,
        version: u32,
        point: (i32, i32),
    ) -> Self {
        Self::new(
            executable,
            target,
            icon_id,
            callback_message,
            version,
            point,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayActivationStatus {
    Posted,
    Pending,
    NoEligibleStrategy,
    PostFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TrayActivationResult {
    status: TrayActivationStatus,
    strategy: Option<TrayActivationStrategy>,
    id: Option<TrayActivationId>,
    delivered_count: usize,
    expected_count: usize,
    pending: bool,
}

impl TrayActivationResult {
    #[must_use]
    pub(crate) const fn status(self) -> TrayActivationStatus {
        self.status
    }

    #[must_use]
    pub(crate) const fn strategy(self) -> Option<TrayActivationStrategy> {
        self.strategy
    }

    #[must_use]
    pub(crate) const fn id(self) -> Option<TrayActivationId> {
        self.id
    }

    #[must_use]
    pub(crate) const fn delivered_count(self) -> usize {
        self.delivered_count
    }

    #[must_use]
    pub(crate) const fn expected_count(self) -> usize {
        self.expected_count
    }

    #[must_use]
    pub(crate) const fn is_pending(self) -> bool {
        self.pending
    }

    #[allow(
        dead_code,
        reason = "result detail accessors are consumed by native adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn posted(self) -> usize {
        self.delivered_count
    }

    #[allow(
        dead_code,
        reason = "result detail accessors are consumed by native adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn is_post_failure(self) -> bool {
        matches!(self.status, TrayActivationStatus::PostFailed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayActivationEffect {
    NoPending,
    StaleToken(TrayActivationId),
    ObservedSuccess {
        id: TrayActivationId,
        strategy: TrayActivationStrategy,
    },
    TimedOut {
        id: TrayActivationId,
        strategy: TrayActivationStrategy,
        protocol_failed: bool,
    },
    Cancelled {
        id: TrayActivationId,
        strategy: TrayActivationStrategy,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingActivation {
    id: TrayActivationId,
    key: String,
    strategy: TrayActivationStrategy,
    delivered_count: usize,
    expected_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AppCompatibility {
    succeeded: Option<TrayActivationStrategy>,
    failed: [bool; 3],
}

impl Default for AppCompatibility {
    fn default() -> Self {
        Self {
            succeeded: None,
            failed: [false; 3],
        }
    }
}

impl AppCompatibility {
    fn mark_success(&mut self, strategy: TrayActivationStrategy) {
        self.succeeded = Some(strategy);
        self.failed[strategy_index(strategy)] = false;
    }

    fn mark_failed(&mut self, strategy: TrayActivationStrategy) {
        self.failed[strategy_index(strategy)] = true;
        if self.succeeded == Some(strategy) {
            self.succeeded = None;
        }
    }

    fn select(&self, version: u32) -> Option<TrayActivationStrategy> {
        if let Some(strategy) = self.succeeded {
            return (!self.failed[strategy_index(strategy)]).then_some(strategy);
        }
        strategy_order(version)
            .into_iter()
            .find(|strategy| !self.failed[strategy_index(*strategy)])
    }
}

/// Pure, in-memory compatibility state for native tray activation.
#[derive(Clone, Debug, Default)]
pub(crate) struct TrayActivationCoordinator {
    compatibility: HashMap<String, AppCompatibility>,
    pending: Option<PendingActivation>,
    next_id: u64,
}

impl TrayActivationCoordinator {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Starts at most one protocol for one user action.
    pub(crate) fn begin(
        &mut self,
        request: TrayActivationRequest,
        sink: &mut dyn TrayCallbackSink,
    ) -> TrayActivationResult {
        if let Some(pending) = self.pending.as_ref() {
            return TrayActivationResult {
                status: TrayActivationStatus::Pending,
                strategy: None,
                id: None,
                delivered_count: pending.delivered_count,
                expected_count: pending.expected_count,
                pending: true,
            };
        }

        let key = normalize_executable_identity(&request.executable);
        let compatibility = self.compatibility.entry(key.clone()).or_default();
        let Some(strategy) = compatibility.select(request.version) else {
            return TrayActivationResult {
                status: TrayActivationStatus::NoEligibleStrategy,
                strategy: None,
                id: None,
                delivered_count: 0,
                expected_count: 0,
                pending: false,
            };
        };

        let messages = strategy_messages(strategy, &request);
        let expected_count = messages.len();
        let id = self.next_activation_id();
        self.pending = Some(PendingActivation {
            id,
            key: key.clone(),
            strategy,
            delivered_count: 0,
            expected_count,
        });
        let mut delivered_count = 0;
        for message in messages {
            if !sink.post(request.target, message) {
                let keep_pending = delivered_count > 0;
                if !keep_pending {
                    self.pending = None;
                }
                return TrayActivationResult {
                    status: TrayActivationStatus::PostFailed,
                    strategy: Some(strategy),
                    id: Some(id),
                    delivered_count,
                    expected_count,
                    pending: keep_pending,
                };
            }
            delivered_count += 1;
            if let Some(pending) = self.pending.as_mut() {
                pending.delivered_count = delivered_count;
            }
        }

        TrayActivationResult {
            status: TrayActivationStatus::Posted,
            strategy: Some(strategy),
            id: Some(id),
            delivered_count,
            expected_count,
            pending: true,
        }
    }

    /// Records the observed menu only for the matching pending action.
    pub(crate) fn mark_observed_success(&mut self, id: TrayActivationId) -> TrayActivationEffect {
        let Some(pending) = self.pending.as_ref() else {
            return TrayActivationEffect::NoPending;
        };
        if pending.id != id {
            return TrayActivationEffect::StaleToken(id);
        }
        let Some(pending) = self.pending.take() else {
            return TrayActivationEffect::NoPending;
        };
        if let Some(compatibility) = self.compatibility.get_mut(&pending.key) {
            compatibility.mark_success(pending.strategy);
        }
        TrayActivationEffect::ObservedSuccess {
            id: pending.id,
            strategy: pending.strategy,
        }
    }

    /// Marks the currently pending strategy failed.  No alternate protocol is
    /// sent; the caller must make an explicit later `begin` call.  Partial
    /// delivery is transport failure, not protocol incompatibility.
    pub(crate) fn mark_timeout(&mut self, id: TrayActivationId) -> TrayActivationEffect {
        let Some(pending) = self.pending.as_ref() else {
            return TrayActivationEffect::NoPending;
        };
        if pending.id != id {
            return TrayActivationEffect::StaleToken(id);
        }
        let Some(pending) = self.pending.take() else {
            return TrayActivationEffect::NoPending;
        };
        let protocol_failed = pending.delivered_count == pending.expected_count;
        if protocol_failed {
            if let Some(compatibility) = self.compatibility.get_mut(&pending.key) {
                compatibility.mark_failed(pending.strategy);
            }
        }
        TrayActivationEffect::TimedOut {
            id: pending.id,
            strategy: pending.strategy,
            protocol_failed,
        }
    }

    pub(crate) fn cancel(&mut self, id: TrayActivationId) -> TrayActivationEffect {
        let Some(pending) = self.pending.as_ref() else {
            return TrayActivationEffect::NoPending;
        };
        if pending.id != id {
            return TrayActivationEffect::StaleToken(id);
        }
        let Some(pending) = self.pending.take() else {
            return TrayActivationEffect::NoPending;
        };
        TrayActivationEffect::Cancelled {
            id: pending.id,
            strategy: pending.strategy,
        }
    }

    fn next_activation_id(&mut self) -> TrayActivationId {
        self.next_id = self.next_id.saturating_add(1).max(1);
        TrayActivationId(self.next_id)
    }

    #[must_use]
    pub(crate) fn pending_strategy(&self) -> Option<TrayActivationStrategy> {
        self.pending.as_ref().map(|pending| pending.strategy)
    }

    #[must_use]
    pub(crate) fn pending_id(&self) -> Option<TrayActivationId> {
        self.pending.as_ref().map(|pending| pending.id)
    }
}

fn pack_signed_word(value: i32) -> u16 {
    value as i16 as u16
}

const fn pack_context_lparam(icon_id: u32) -> isize {
    let packed = ((icon_id as u16 as u32) << 16) | WM_CONTEXTMENU;
    packed as isize
}

fn strategy_messages(
    strategy: TrayActivationStrategy,
    request: &TrayActivationRequest,
) -> Vec<TrayMessage> {
    match strategy {
        TrayActivationStrategy::VersionAware => vec![TrayMessage::modern_context_with_callback(
            request.callback_message,
            request.icon_id,
            request.point,
        )],
        TrayActivationStrategy::Legacy => vec![
            TrayMessage::legacy_with_callback(
                request.callback_message,
                request.icon_id,
                WM_RBUTTONDOWN,
            ),
            TrayMessage::legacy_with_callback(
                request.callback_message,
                request.icon_id,
                WM_RBUTTONUP,
            ),
        ],
        TrayActivationStrategy::Combined => vec![
            TrayMessage::legacy_with_callback(
                request.callback_message,
                request.icon_id,
                WM_RBUTTONDOWN,
            ),
            TrayMessage::legacy_with_callback(
                request.callback_message,
                request.icon_id,
                WM_RBUTTONUP,
            ),
            TrayMessage::legacy_with_callback(
                request.callback_message,
                request.icon_id,
                WM_CONTEXTMENU,
            ),
        ],
    }
}

fn strategy_order(version: u32) -> [TrayActivationStrategy; 3] {
    if version >= 4 {
        [
            TrayActivationStrategy::VersionAware,
            TrayActivationStrategy::Legacy,
            TrayActivationStrategy::Combined,
        ]
    } else {
        [
            TrayActivationStrategy::Legacy,
            TrayActivationStrategy::Combined,
            TrayActivationStrategy::VersionAware,
        ]
    }
}

const fn strategy_index(strategy: TrayActivationStrategy) -> usize {
    match strategy {
        TrayActivationStrategy::VersionAware => 0,
        TrayActivationStrategy::Legacy => 1,
        TrayActivationStrategy::Combined => 2,
    }
}

pub(crate) fn normalize_executable_identity(executable: &str) -> String {
    let trimmed = executable.trim().trim_matches('"');
    let without_device_prefix = trimmed
        .strip_prefix(r"\\?\")
        .or_else(|| trimmed.strip_prefix(r"\??\"))
        .unwrap_or(trimmed);
    let normalized = without_device_prefix
        .replace('/', "\\")
        .to_ascii_lowercase();
    if let Some(unc_path) = normalized.strip_prefix("unc\\") {
        format!(r"\\{unc_path}")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingSink {
        posted: Vec<(NativeWindowId, TrayMessage)>,
        accept: bool,
        fail_at: Option<usize>,
    }

    impl TrayCallbackSink for RecordingSink {
        fn post(&mut self, target: NativeWindowId, message: TrayMessage) -> bool {
            self.posted.push((target, message));
            self.accept
                && self
                    .fail_at
                    .map_or(true, |fail_at| self.posted.len() - 1 < fail_at)
        }
    }

    fn request(executable: &str, version: u32) -> TrayActivationRequest {
        TrayActivationRequest::new(
            executable,
            NativeWindowId::new(7),
            0x1234,
            0x8001,
            version,
            TrayScreenPoint::new(-32769, 32768),
        )
    }

    #[test]
    fn modern_context_packs_signed_coordinates_and_context_icon_words() {
        let message = TrayMessage::modern_context(0x1234_5678, TrayScreenPoint::new(-2, 3));
        assert_eq!(message.message(), 0);
        assert_eq!(message.wparam(), 0x0003_fffe);
        assert_eq!(message.lparam(), 0x5678_007b);
    }

    #[test]
    fn modern_context_keeps_the_signed_16_bit_coordinate_boundaries_exact() {
        assert_eq!(
            TrayMessage::modern_context(1, TrayScreenPoint::new(-32_768, -32_768)).wparam(),
            0x8000_8000
        );
        assert_eq!(
            TrayMessage::modern_context(1, TrayScreenPoint::new(32_767, 32_767)).wparam(),
            0x7fff_7fff
        );
    }

    #[test]
    fn legacy_messages_keep_icon_in_wparam_and_event_in_lparam() {
        assert_eq!(
            TrayMessage::legacy(9, WM_RBUTTONDOWN),
            TrayMessage {
                message: 0,
                wparam: 9,
                lparam: WM_RBUTTONDOWN as isize,
            }
        );
        assert_eq!(
            TrayMessage::legacy(9, WM_RBUTTONUP).lparam(),
            WM_RBUTTONUP as isize
        );
        assert_eq!(
            TrayMessage::legacy(u32::MAX, WM_RBUTTONDOWN).wparam(),
            u32::MAX as usize
        );
    }

    #[test]
    fn one_begin_posts_only_the_initial_modern_strategy() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink {
            accept: true,
            ..RecordingSink::default()
        };
        let result = coordinator.begin(request("Vendor\\App.EXE", 4), &mut sink);
        assert_eq!(result.status(), TrayActivationStatus::Posted);
        assert_eq!(
            result.strategy(),
            Some(TrayActivationStrategy::VersionAware)
        );
        assert!(result.id().is_some());
        assert_eq!(sink.posted.len(), 1);
        assert_eq!(
            sink.posted[0].1,
            TrayMessage::modern_context_with_callback(
                0x8001,
                0x1234,
                TrayScreenPoint::new(-32769, 32768)
            )
        );
    }

    #[test]
    fn legacy_versions_start_with_legacy_and_combined_is_three_messages() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink {
            accept: true,
            ..RecordingSink::default()
        };
        let result = coordinator.begin(request("legacy.exe", 3), &mut sink);
        assert_eq!(result.strategy(), Some(TrayActivationStrategy::Legacy));
        let legacy_id = result.id().expect("legacy activation id");
        assert_eq!(
            sink.posted
                .iter()
                .map(|(_, message)| *message)
                .collect::<Vec<_>>(),
            vec![
                TrayMessage::legacy_with_callback(0x8001, 0x1234, WM_RBUTTONDOWN),
                TrayMessage::legacy_with_callback(0x8001, 0x1234, WM_RBUTTONUP),
            ]
        );

        assert!(matches!(
            coordinator.mark_timeout(legacy_id),
            TrayActivationEffect::TimedOut {
                protocol_failed: true,
                ..
            }
        ));
        let result = coordinator.begin(request("legacy.exe", 3), &mut sink);
        assert_eq!(result.strategy(), Some(TrayActivationStrategy::Combined));
        assert_eq!(sink.posted.len(), 5);
        assert_eq!(
            sink.posted[2..]
                .iter()
                .map(|(_, message)| *message)
                .collect::<Vec<_>>(),
            vec![
                TrayMessage::legacy_with_callback(0x8001, 0x1234, WM_RBUTTONDOWN),
                TrayMessage::legacy_with_callback(0x8001, 0x1234, WM_RBUTTONUP),
                TrayMessage::legacy_with_callback(0x8001, 0x1234, WM_CONTEXTMENU),
            ]
        );
    }

    #[test]
    fn success_is_reused_for_normalized_executable_identity() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink {
            accept: true,
            ..RecordingSink::default()
        };
        let first = coordinator.begin(request(r"C:/Vendor/App.EXE", 4), &mut sink);
        assert_eq!(first.strategy(), Some(TrayActivationStrategy::VersionAware));
        let first_id = first.id().expect("first activation id");
        assert!(matches!(
            coordinator.mark_observed_success(first_id),
            TrayActivationEffect::ObservedSuccess {
                strategy: TrayActivationStrategy::VersionAware,
                ..
            }
        ));
        let second = coordinator.begin(request(r" c:\vendor\app.exe ", 1), &mut sink);
        assert_eq!(
            second.strategy(),
            Some(TrayActivationStrategy::VersionAware)
        );
    }

    #[test]
    fn pending_and_cancel_prevent_duplicate_click_posts() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink {
            accept: true,
            ..RecordingSink::default()
        };
        let first = coordinator.begin(request("app.exe", 4), &mut sink);
        assert_eq!(first.status(), TrayActivationStatus::Posted);
        let first_id = first.id().expect("first activation id");
        let duplicate = coordinator.begin(request("other.exe", 4), &mut sink);
        assert_eq!(duplicate.status(), TrayActivationStatus::Pending);
        assert_eq!(sink.posted.len(), 1);
        assert!(matches!(
            coordinator.cancel(first_id),
            TrayActivationEffect::Cancelled {
                strategy: TrayActivationStrategy::VersionAware,
                ..
            }
        ));
        assert_eq!(coordinator.pending_strategy(), None);
    }

    #[test]
    fn post_failure_is_reported_without_auto_posting_an_alternate_strategy() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink::default();
        let result = coordinator.begin(request("app.exe", 4), &mut sink);
        assert_eq!(result.status(), TrayActivationStatus::PostFailed);
        assert_eq!(
            result.strategy(),
            Some(TrayActivationStrategy::VersionAware)
        );
        assert_eq!(sink.posted.len(), 1);
        assert_eq!(coordinator.pending_strategy(), None);
        assert!(!result.is_pending());
    }

    #[test]
    fn app_compatibility_failures_and_successes_are_isolated() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink {
            accept: true,
            ..RecordingSink::default()
        };
        let first = coordinator.begin(request("first.exe", 4), &mut sink);
        coordinator.mark_timeout(first.id().expect("first activation id"));
        let second = coordinator.begin(request("second.exe", 4), &mut sink);
        coordinator.mark_observed_success(second.id().expect("second activation id"));
        let first_next = coordinator.begin(request("FIRST.EXE", 4), &mut sink);
        assert_eq!(first_next.strategy(), Some(TrayActivationStrategy::Legacy));
        coordinator.cancel(first_next.id().expect("first retry activation id"));
        let second_next = coordinator.begin(request("second.exe", 4), &mut sink);
        assert_eq!(
            second_next.strategy(),
            Some(TrayActivationStrategy::VersionAware)
        );
    }

    #[test]
    fn stale_tokens_cannot_consume_or_mutate_a_new_pending_activation() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink {
            accept: true,
            ..RecordingSink::default()
        };
        let activation_a = coordinator.begin(request("a.exe", 4), &mut sink);
        let id_a = activation_a.id().expect("activation A id");
        assert!(matches!(
            coordinator.cancel(id_a),
            TrayActivationEffect::Cancelled { .. }
        ));

        let activation_b = coordinator.begin(request("b.exe", 4), &mut sink);
        let id_b = activation_b.id().expect("activation B id");
        assert_eq!(coordinator.pending_id(), Some(id_b));
        assert!(matches!(
            coordinator.mark_observed_success(id_a),
            TrayActivationEffect::StaleToken(stale) if stale == id_a
        ));
        assert!(matches!(
            coordinator.mark_timeout(id_a),
            TrayActivationEffect::StaleToken(stale) if stale == id_a
        ));
        assert!(matches!(
            coordinator.cancel(id_a),
            TrayActivationEffect::StaleToken(stale) if stale == id_a
        ));
        assert_eq!(coordinator.pending_id(), Some(id_b));
        assert!(matches!(
            coordinator.cancel(id_b),
            TrayActivationEffect::Cancelled { id, .. } if id == id_b
        ));
    }

    #[test]
    fn partial_combined_post_can_observe_success_and_cache_combined() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink {
            accept: true,
            ..RecordingSink::default()
        };
        let legacy = coordinator.begin(request("combined.exe", 3), &mut sink);
        let legacy_id = legacy.id().expect("legacy activation id");
        assert!(matches!(
            coordinator.mark_timeout(legacy_id),
            TrayActivationEffect::TimedOut {
                protocol_failed: true,
                ..
            }
        ));
        sink.fail_at = Some(sink.posted.len() + 2);

        let combined = coordinator.begin(request("combined.exe", 3), &mut sink);
        assert_eq!(combined.strategy(), Some(TrayActivationStrategy::Combined));
        assert_eq!(combined.delivered_count(), 2);
        assert_eq!(combined.expected_count(), 3);
        assert!(combined.is_pending());
        let combined_id = combined.id().expect("combined activation id");
        assert!(matches!(
            coordinator.mark_observed_success(combined_id),
            TrayActivationEffect::ObservedSuccess {
                strategy: TrayActivationStrategy::Combined,
                ..
            }
        ));

        sink.fail_at = None;
        let reused = coordinator.begin(request("COMBINED.EXE", 4), &mut sink);
        assert_eq!(reused.strategy(), Some(TrayActivationStrategy::Combined));
        coordinator.cancel(reused.id().expect("reused activation id"));
    }

    #[test]
    fn partial_transport_timeout_retries_the_same_strategy_without_poisoning_it() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink {
            accept: true,
            fail_at: Some(1),
            ..RecordingSink::default()
        };
        let failed = coordinator.begin(request("retry.exe", 3), &mut sink);
        assert_eq!(failed.strategy(), Some(TrayActivationStrategy::Legacy));
        assert_eq!(failed.delivered_count(), 1);
        assert!(failed.is_pending());
        let id = failed.id().expect("partial activation id");
        assert!(matches!(
            coordinator.mark_timeout(id),
            TrayActivationEffect::TimedOut {
                protocol_failed: false,
                ..
            }
        ));

        sink.fail_at = None;
        let retry = coordinator.begin(request("retry.exe", 3), &mut sink);
        assert_eq!(retry.strategy(), Some(TrayActivationStrategy::Legacy));
        coordinator.cancel(retry.id().expect("retry activation id"));
    }

    #[test]
    fn zero_delivery_failures_do_not_exhaust_the_initial_strategy() {
        let mut coordinator = TrayActivationCoordinator::new();
        let mut sink = RecordingSink::default();
        let first = coordinator.begin(request("zero.exe", 4), &mut sink);
        assert_eq!(first.strategy(), Some(TrayActivationStrategy::VersionAware));
        assert_eq!(first.delivered_count(), 0);
        assert!(!first.is_pending());
        let second = coordinator.begin(request("zero.exe", 4), &mut sink);
        assert_eq!(
            second.strategy(),
            Some(TrayActivationStrategy::VersionAware)
        );
        assert_eq!(second.delivered_count(), 0);
        assert!(!second.is_pending());
    }

    #[test]
    fn device_prefixed_unc_paths_share_the_plain_unc_identity() {
        assert_eq!(
            normalize_executable_identity(r"\\?\UNC\Server\Share\App.EXE"),
            normalize_executable_identity(r"\\server\share\app.exe")
        );
    }
}
