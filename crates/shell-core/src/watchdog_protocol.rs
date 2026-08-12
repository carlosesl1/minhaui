//! Pure, bounded protocol primitives for a future watchdog arming handshake.
//!
//! This module deliberately performs no I/O, journal persistence, Win32
//! discovery, or mutation. Parsing a frame or matching a fingerprint does not
//! prove Explorer ownership and does not authorize taskbar replacement. Those
//! guarantees belong to the watchdog and platform adapters that will consume
//! this contract in a later change.

use std::fmt;
use std::str::FromStr;

use sha2::{Digest, Sha256};
use thiserror::Error;

const PROTOCOL_NAME: &str = "MINHA_UI_CONTROL";
const PROTOCOL_VERSION: &str = "v1";
const TASKBAR_FINGERPRINT_DOMAIN_V1: &[u8] = b"MINHA_UI_TASKBAR_ARMING_V1\0";
const SUPPORTED_APPBAR_STATE_BITS_V1: u32 = 0x1;

/// Maximum size of one protocol frame, excluding its line terminator.
pub const MAX_WATCHDOG_CONTROL_FRAME_BYTES: usize = 128;

/// Maximum number of taskbar observations accepted by the V1 fingerprint.
pub const MAX_TASKBAR_ARMING_SNAPSHOTS: usize = 64;

/// Maximum UTF-8 byte length of a canonical monitor device identifier.
pub const MAX_TASKBAR_DEVICE_ID_BYTES: usize = 128;

/// Error returned when a fixed-width hexadecimal token is not canonical.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FixedHexError {
    #[error("expected {expected} lowercase hexadecimal characters, found {actual}")]
    InvalidLength { expected: usize, actual: usize },
    #[error("token must contain only lowercase hexadecimal characters")]
    NonCanonical,
    #[error("identifier must not be zero")]
    Zero,
}

/// Correlates one child request with exactly one watchdog response.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArmRequestId(u64);

impl ArmRequestId {
    /// Creates a non-zero request identifier.
    pub const fn try_new(value: u64) -> Result<Self, FixedHexError> {
        if value == 0 {
            Err(FixedHexError::Zero)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the underlying request sequence.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ArmRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

impl FromStr for ArmRequestId {
    type Err = FixedHexError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let decoded = decode_lower_hex::<8>(value)?;
        Self::try_new(u64::from_be_bytes(decoded))
    }
}

/// Watchdog-generated identifier for one durable recovery transaction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecoveryTransactionId([u8; 16]);

impl RecoveryTransactionId {
    /// Creates a transaction identifier from watchdog-owned entropy.
    pub fn try_from_bytes(bytes: [u8; 16]) -> Result<Self, FixedHexError> {
        if bytes.iter().all(|byte| *byte == 0) {
            Err(FixedHexError::Zero)
        } else {
            Ok(Self(bytes))
        }
    }

    /// Returns the stable 128-bit identifier bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 16] {
        self.0
    }
}

impl fmt::Display for RecoveryTransactionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_lower_hex(formatter, &self.0)
    }
}

impl FromStr for RecoveryTransactionId {
    type Err = FixedHexError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from_bytes(decode_lower_hex::<16>(value)?)
    }
}

/// Fixed-width digest of one canonical, read-only taskbar observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskbarStateFingerprint([u8; 32]);

impl TaskbarStateFingerprint {
    /// Wraps a digest produced by a canonical fingerprint implementation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the digest bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Display for TaskbarStateFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_lower_hex(formatter, &self.0)
    }
}

impl FromStr for TaskbarStateFingerprint {
    type Err = FixedHexError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(decode_lower_hex::<32>(value)?))
    }
}

/// Direction enforced by the eventual duplex pipe adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchdogFrameDirection {
    ChildToWatchdog,
    WatchdogToChild,
}

/// Coarse, non-sensitive reason for refusing an arming request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArmDeniedReason {
    Busy,
    PendingRecovery,
    SnapshotMismatch,
    UnsupportedState,
    JournalUnavailable,
    ProtocolViolation,
}

impl ArmDeniedReason {
    const fn as_token(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::PendingRecovery => "pending_recovery",
            Self::SnapshotMismatch => "snapshot_mismatch",
            Self::UnsupportedState => "unsupported_state",
            Self::JournalUnavailable => "journal_unavailable",
            Self::ProtocolViolation => "protocol_violation",
        }
    }

    fn from_token(token: &str) -> Result<Self, WatchdogProtocolError> {
        match token {
            "busy" => Ok(Self::Busy),
            "pending_recovery" => Ok(Self::PendingRecovery),
            "snapshot_mismatch" => Ok(Self::SnapshotMismatch),
            "unsupported_state" => Ok(Self::UnsupportedState),
            "journal_unavailable" => Ok(Self::JournalUnavailable),
            "protocol_violation" => Ok(Self::ProtocolViolation),
            _ => Err(WatchdogProtocolError::UnknownDenialReason),
        }
    }
}

/// One complete V1 control frame without its transport line terminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchdogControlFrame {
    Arm {
        request_id: ArmRequestId,
        fingerprint: TaskbarStateFingerprint,
    },
    Applied {
        transaction_id: RecoveryTransactionId,
    },
    Cancel {
        request_id: ArmRequestId,
    },
    Disarm {
        transaction_id: RecoveryTransactionId,
    },
    Armed {
        request_id: ArmRequestId,
        transaction_id: RecoveryTransactionId,
    },
    Denied {
        request_id: ArmRequestId,
        reason: ArmDeniedReason,
    },
    Lease {
        transaction_id: RecoveryTransactionId,
    },
    Disarmed {
        transaction_id: RecoveryTransactionId,
    },
}

impl WatchdogControlFrame {
    /// Returns the only valid transport direction for this frame kind.
    #[must_use]
    pub const fn direction(self) -> WatchdogFrameDirection {
        match self {
            Self::Arm { .. } | Self::Applied { .. } | Self::Cancel { .. } | Self::Disarm { .. } => {
                WatchdogFrameDirection::ChildToWatchdog
            }
            Self::Armed { .. }
            | Self::Denied { .. }
            | Self::Lease { .. }
            | Self::Disarmed { .. } => WatchdogFrameDirection::WatchdogToChild,
        }
    }

    /// Encodes the canonical ASCII representation without a newline.
    #[must_use]
    pub fn encode(self) -> String {
        match self {
            Self::Arm {
                request_id,
                fingerprint,
            } => format!("{PROTOCOL_NAME} {PROTOCOL_VERSION} ARM {request_id} {fingerprint}"),
            Self::Applied { transaction_id } => {
                format!("{PROTOCOL_NAME} {PROTOCOL_VERSION} APPLIED {transaction_id}")
            }
            Self::Cancel { request_id } => {
                format!("{PROTOCOL_NAME} {PROTOCOL_VERSION} CANCEL {request_id}")
            }
            Self::Disarm { transaction_id } => {
                format!("{PROTOCOL_NAME} {PROTOCOL_VERSION} DISARM {transaction_id}")
            }
            Self::Armed {
                request_id,
                transaction_id,
            } => format!("{PROTOCOL_NAME} {PROTOCOL_VERSION} ARMED {request_id} {transaction_id}"),
            Self::Denied { request_id, reason } => format!(
                "{PROTOCOL_NAME} {PROTOCOL_VERSION} DENIED {request_id} {}",
                reason.as_token()
            ),
            Self::Lease { transaction_id } => {
                format!("{PROTOCOL_NAME} {PROTOCOL_VERSION} LEASE {transaction_id}")
            }
            Self::Disarmed { transaction_id } => {
                format!("{PROTOCOL_NAME} {PROTOCOL_VERSION} DISARMED {transaction_id}")
            }
        }
    }

    /// Parses one bounded canonical ASCII frame without a newline.
    pub fn parse(frame: &[u8]) -> Result<Self, WatchdogProtocolError> {
        if frame.len() > MAX_WATCHDOG_CONTROL_FRAME_BYTES {
            return Err(WatchdogProtocolError::Oversized {
                actual: frame.len(),
                maximum: MAX_WATCHDOG_CONTROL_FRAME_BYTES,
            });
        }
        if !frame.is_ascii() {
            return Err(WatchdogProtocolError::NonAscii);
        }
        if frame
            .iter()
            .any(|byte| matches!(*byte, b'\r' | b'\n' | b'\t'))
        {
            return Err(WatchdogProtocolError::InvalidSpacing);
        }
        let text = std::str::from_utf8(frame).map_err(|_| WatchdogProtocolError::NonAscii)?;
        let fields = text.split(' ').collect::<Vec<_>>();
        if fields.iter().any(|field| field.is_empty()) {
            return Err(WatchdogProtocolError::InvalidSpacing);
        }
        if fields.first().copied() != Some(PROTOCOL_NAME) {
            return Err(WatchdogProtocolError::InvalidPrefix);
        }
        if fields.get(1).copied() != Some(PROTOCOL_VERSION) {
            return Err(WatchdogProtocolError::UnsupportedVersion);
        }
        let kind = fields
            .get(2)
            .copied()
            .ok_or(WatchdogProtocolError::MissingFrameKind)?;
        match kind {
            "ARM" => {
                require_field_count(&fields, 5)?;
                Ok(Self::Arm {
                    request_id: parse_request_id(fields[3])?,
                    fingerprint: parse_fingerprint(fields[4])?,
                })
            }
            "APPLIED" => {
                require_field_count(&fields, 4)?;
                Ok(Self::Applied {
                    transaction_id: parse_transaction_id(fields[3])?,
                })
            }
            "CANCEL" => {
                require_field_count(&fields, 4)?;
                Ok(Self::Cancel {
                    request_id: parse_request_id(fields[3])?,
                })
            }
            "DISARM" => {
                require_field_count(&fields, 4)?;
                Ok(Self::Disarm {
                    transaction_id: parse_transaction_id(fields[3])?,
                })
            }
            "ARMED" => {
                require_field_count(&fields, 5)?;
                Ok(Self::Armed {
                    request_id: parse_request_id(fields[3])?,
                    transaction_id: parse_transaction_id(fields[4])?,
                })
            }
            "DENIED" => {
                require_field_count(&fields, 5)?;
                Ok(Self::Denied {
                    request_id: parse_request_id(fields[3])?,
                    reason: ArmDeniedReason::from_token(fields[4])?,
                })
            }
            "LEASE" => {
                require_field_count(&fields, 4)?;
                Ok(Self::Lease {
                    transaction_id: parse_transaction_id(fields[3])?,
                })
            }
            "DISARMED" => {
                require_field_count(&fields, 4)?;
                Ok(Self::Disarmed {
                    transaction_id: parse_transaction_id(fields[3])?,
                })
            }
            _ => Err(WatchdogProtocolError::UnknownFrameKind),
        }
    }
}

/// Fail-closed parser error for an untrusted control frame.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WatchdogProtocolError {
    #[error("control frame is {actual} bytes, exceeding the {maximum}-byte limit")]
    Oversized { actual: usize, maximum: usize },
    #[error("control frame must be ASCII")]
    NonAscii,
    #[error("control frame must use one ASCII space between fields and contain no line ending")]
    InvalidSpacing,
    #[error("control frame has an invalid protocol prefix")]
    InvalidPrefix,
    #[error("control frame uses an unsupported protocol version")]
    UnsupportedVersion,
    #[error("control frame has no frame kind")]
    MissingFrameKind,
    #[error("control frame has an unknown frame kind")]
    UnknownFrameKind,
    #[error("control frame has {actual} fields; expected {expected}")]
    InvalidFieldCount { expected: usize, actual: usize },
    #[error("control frame has an invalid request id: {source}")]
    InvalidRequestId {
        #[source]
        source: FixedHexError,
    },
    #[error("control frame has an invalid transaction id: {source}")]
    InvalidTransactionId {
        #[source]
        source: FixedHexError,
    },
    #[error("control frame has an invalid taskbar fingerprint: {source}")]
    InvalidFingerprint {
        #[source]
        source: FixedHexError,
    },
    #[error("control frame has an unknown denial reason")]
    UnknownDenialReason,
}

/// Supported Explorer taskbar class represented without a raw class string.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TaskbarWindowClass {
    Primary,
    Secondary,
}

impl TaskbarWindowClass {
    const fn canonical_tag(self) -> u8 {
        match self {
            Self::Primary => 1,
            Self::Secondary => 2,
        }
    }
}

/// Physical taskbar bounds included in the canonical fingerprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskbarFingerprintBounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl TaskbarFingerprintBounds {
    #[must_use]
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    #[must_use]
    pub const fn left(self) -> i32 {
        self.left
    }

    #[must_use]
    pub const fn top(self) -> i32 {
        self.top
    }

    #[must_use]
    pub const fn right(self) -> i32 {
        self.right
    }

    #[must_use]
    pub const fn bottom(self) -> i32 {
        self.bottom
    }

    const fn is_valid(self) -> bool {
        self.right > self.left && self.bottom > self.top
    }
}

/// Read-only logical target used to compare independent taskbar discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskbarFingerprintSnapshot {
    device: String,
    class: TaskbarWindowClass,
    visible: bool,
    bounds: TaskbarFingerprintBounds,
}

impl TaskbarFingerprintSnapshot {
    #[must_use]
    pub fn new(
        device: impl Into<String>,
        class: TaskbarWindowClass,
        visible: bool,
        bounds: TaskbarFingerprintBounds,
    ) -> Self {
        Self {
            device: device.into(),
            class,
            visible,
            bounds,
        }
    }

    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }

    #[must_use]
    pub const fn class(&self) -> TaskbarWindowClass {
        self.class
    }

    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }

    #[must_use]
    pub const fn bounds(&self) -> TaskbarFingerprintBounds {
        self.bounds
    }
}

/// Validation error raised before hashing a read-only taskbar observation.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TaskbarArmingFingerprintError {
    #[error("Explorer process id must not be zero")]
    InvalidExplorerProcessId,
    #[error("V1 fingerprint contains unsupported appbar state 0x{state:08x}")]
    UnsupportedAppbarState { state: u32 },
    #[error("V1 fingerprint contains no taskbar snapshots")]
    EmptySnapshots,
    #[error(
        "V1 fingerprint contains {actual} taskbar snapshots, exceeding the {maximum}-snapshot limit"
    )]
    TooManySnapshots { actual: usize, maximum: usize },
    #[error("taskbar snapshot {index} has an empty monitor device")]
    EmptyDevice { index: usize },
    #[error(
        "taskbar snapshot {index} device is {actual} bytes, exceeding the {maximum}-byte limit"
    )]
    DeviceTooLong {
        index: usize,
        actual: usize,
        maximum: usize,
    },
    #[error("taskbar snapshot {index} device must contain only visible ASCII characters")]
    InvalidDevice { index: usize },
    #[error("taskbar snapshot {index} has invalid bounds")]
    InvalidBounds { index: usize },
    #[error("taskbar snapshots {first} and {second} have the same class and monitor device")]
    DuplicateSnapshot { first: usize, second: usize },
    #[error("V1 fingerprint requires exactly one primary taskbar snapshot; found {actual}")]
    InvalidPrimaryCount { actual: usize },
}

/// Computes the deterministic V1 digest used to compare two independent,
/// already-validated read-only taskbar observations.
///
/// Device identifiers are compared and hashed case-insensitively using ASCII,
/// and snapshot input order does not affect the result. The digest is only an
/// equality token; it is not evidence that either observer trusted the HWNDs.
pub fn taskbar_arming_fingerprint_v1(
    session_id: u32,
    explorer_process_id: u32,
    appbar_state: u32,
    snapshots: &[TaskbarFingerprintSnapshot],
) -> Result<TaskbarStateFingerprint, TaskbarArmingFingerprintError> {
    validate_fingerprint_input(explorer_process_id, appbar_state, snapshots)?;

    let mut ordered = snapshots.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.class
            .cmp(&right.class)
            .then_with(|| compare_device_ascii_case_insensitive(&left.device, &right.device))
            .then_with(|| left.bounds.left.cmp(&right.bounds.left))
            .then_with(|| left.bounds.top.cmp(&right.bounds.top))
            .then_with(|| left.bounds.right.cmp(&right.bounds.right))
            .then_with(|| left.bounds.bottom.cmp(&right.bounds.bottom))
            .then_with(|| left.visible.cmp(&right.visible))
    });

    let mut hasher = Sha256::new();
    hasher.update(TASKBAR_FINGERPRINT_DOMAIN_V1);
    hasher.update(session_id.to_be_bytes());
    hasher.update(explorer_process_id.to_be_bytes());
    hasher.update(appbar_state.to_be_bytes());
    hasher.update((ordered.len() as u16).to_be_bytes());
    for snapshot in ordered {
        let canonical_device = snapshot.device.to_ascii_uppercase();
        hasher.update([snapshot.class.canonical_tag()]);
        hasher.update([u8::from(snapshot.visible)]);
        hasher.update(snapshot.bounds.left.to_be_bytes());
        hasher.update(snapshot.bounds.top.to_be_bytes());
        hasher.update(snapshot.bounds.right.to_be_bytes());
        hasher.update(snapshot.bounds.bottom.to_be_bytes());
        hasher.update((canonical_device.len() as u16).to_be_bytes());
        hasher.update(canonical_device.as_bytes());
    }
    Ok(TaskbarStateFingerprint::from_bytes(
        hasher.finalize().into(),
    ))
}

fn validate_fingerprint_input(
    explorer_process_id: u32,
    appbar_state: u32,
    snapshots: &[TaskbarFingerprintSnapshot],
) -> Result<(), TaskbarArmingFingerprintError> {
    if explorer_process_id == 0 {
        return Err(TaskbarArmingFingerprintError::InvalidExplorerProcessId);
    }
    if appbar_state & !SUPPORTED_APPBAR_STATE_BITS_V1 != 0 {
        return Err(TaskbarArmingFingerprintError::UnsupportedAppbarState {
            state: appbar_state,
        });
    }
    if snapshots.is_empty() {
        return Err(TaskbarArmingFingerprintError::EmptySnapshots);
    }
    if snapshots.len() > MAX_TASKBAR_ARMING_SNAPSHOTS {
        return Err(TaskbarArmingFingerprintError::TooManySnapshots {
            actual: snapshots.len(),
            maximum: MAX_TASKBAR_ARMING_SNAPSHOTS,
        });
    }

    let mut primary_count = 0;
    for (index, snapshot) in snapshots.iter().enumerate() {
        validate_snapshot(index, snapshot)?;
        primary_count += usize::from(snapshot.class == TaskbarWindowClass::Primary);
        for (previous_index, previous) in snapshots[..index].iter().enumerate() {
            if previous.class == snapshot.class
                && previous.device.eq_ignore_ascii_case(&snapshot.device)
            {
                return Err(TaskbarArmingFingerprintError::DuplicateSnapshot {
                    first: previous_index,
                    second: index,
                });
            }
        }
    }
    if primary_count != 1 {
        return Err(TaskbarArmingFingerprintError::InvalidPrimaryCount {
            actual: primary_count,
        });
    }
    Ok(())
}

fn validate_snapshot(
    index: usize,
    snapshot: &TaskbarFingerprintSnapshot,
) -> Result<(), TaskbarArmingFingerprintError> {
    if snapshot.device.is_empty() {
        return Err(TaskbarArmingFingerprintError::EmptyDevice { index });
    }
    if snapshot.device.len() > MAX_TASKBAR_DEVICE_ID_BYTES {
        return Err(TaskbarArmingFingerprintError::DeviceTooLong {
            index,
            actual: snapshot.device.len(),
            maximum: MAX_TASKBAR_DEVICE_ID_BYTES,
        });
    }
    if !snapshot.device.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(TaskbarArmingFingerprintError::InvalidDevice { index });
    }
    if !snapshot.bounds.is_valid() {
        return Err(TaskbarArmingFingerprintError::InvalidBounds { index });
    }
    Ok(())
}

fn compare_device_ascii_case_insensitive(left: &str, right: &str) -> std::cmp::Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_uppercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_uppercase()))
}

fn require_field_count(fields: &[&str], expected: usize) -> Result<(), WatchdogProtocolError> {
    if fields.len() == expected {
        Ok(())
    } else {
        Err(WatchdogProtocolError::InvalidFieldCount {
            expected,
            actual: fields.len(),
        })
    }
}

fn parse_request_id(value: &str) -> Result<ArmRequestId, WatchdogProtocolError> {
    value
        .parse()
        .map_err(|source| WatchdogProtocolError::InvalidRequestId { source })
}

fn parse_transaction_id(value: &str) -> Result<RecoveryTransactionId, WatchdogProtocolError> {
    value
        .parse()
        .map_err(|source| WatchdogProtocolError::InvalidTransactionId { source })
}

fn parse_fingerprint(value: &str) -> Result<TaskbarStateFingerprint, WatchdogProtocolError> {
    value
        .parse()
        .map_err(|source| WatchdogProtocolError::InvalidFingerprint { source })
}

fn decode_lower_hex<const N: usize>(value: &str) -> Result<[u8; N], FixedHexError> {
    let expected = N.saturating_mul(2);
    if value.len() != expected {
        return Err(FixedHexError::InvalidLength {
            expected,
            actual: value.len(),
        });
    }
    let mut decoded = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_lower_hex_nibble(pair[0]).ok_or(FixedHexError::NonCanonical)?;
        let low = decode_lower_hex_nibble(pair[1]).ok_or(FixedHexError::NonCanonical)?;
        decoded[index] = (high << 4) | low;
    }
    Ok(decoded)
}

const fn decode_lower_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn write_lower_hex(formatter: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}
