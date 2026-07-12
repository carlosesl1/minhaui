#![deny(unsafe_code)]

use shell_core::Popover;
use shell_renderer::DipPoint;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopbarPointerPhase {
    Moved,
    Pressed,
    Released,
    Exited,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopbarPointerSample {
    phase: TopbarPointerPhase,
    point: DipPoint,
}

impl TopbarPointerSample {
    #[must_use]
    pub const fn new(phase: TopbarPointerPhase, point: DipPoint) -> Self {
        Self { phase, point }
    }

    #[must_use]
    pub const fn phase(self) -> TopbarPointerPhase {
        self.phase
    }

    #[must_use]
    pub const fn point(self) -> DipPoint {
        self.point
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopbarKey {
    Next,
    Previous,
    Activate,
    Escape,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueuedTopbarAction {
    OpenPopover(Popover),
    PollDeferred,
    RedrawTopbar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PollBudget {
    last_polled_ms: u64,
    minimum_interval_ms: u64,
}

impl PollBudget {
    #[must_use]
    pub const fn new(last_polled_ms: u64, minimum_interval_ms: u64) -> Self {
        Self {
            last_polled_ms,
            minimum_interval_ms,
        }
    }

    #[must_use]
    pub const fn permits(self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_polled_ms) >= self.minimum_interval_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TopbarSnapshot {
    pub(crate) clock: String,
    pub(crate) network: NetworkSnapshot,
    pub(crate) volume_percent: u8,
    pub(crate) battery: PowerSnapshot,
    pub(crate) notifications: u16,
}

impl TopbarSnapshot {
    #[must_use]
    pub fn new(
        clock: String,
        network: NetworkSnapshot,
        volume_percent: u8,
        battery: PowerSnapshot,
        notifications: u16,
    ) -> Self {
        Self {
            clock,
            network,
            volume_percent: volume_percent.min(100),
            battery,
            notifications,
        }
    }

    #[must_use]
    pub fn privacy_safe_fixture() -> Self {
        Self::new(
            "09:41 Sun 12".to_owned(),
            NetworkSnapshot::new("Online", 12, 3),
            42,
            PowerSnapshot::new(Some(86), true),
            0,
        )
    }

    #[must_use]
    pub fn clock(&self) -> &str {
        &self.clock
    }
}

impl Default for TopbarSnapshot {
    fn default() -> Self {
        Self::privacy_safe_fixture()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkSnapshot {
    label: String,
    received_kib_s: u32,
    sent_kib_s: u32,
}

impl NetworkSnapshot {
    #[must_use]
    pub fn new(label: &str, received_kib_s: u32, sent_kib_s: u32) -> Self {
        Self {
            label: label.to_owned(),
            received_kib_s,
            sent_kib_s,
        }
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub fn is_online(&self) -> bool {
        self.label == "Online"
    }

    #[must_use]
    pub const fn throughput_label(&self) -> ThroughputLabel {
        ThroughputLabel {
            received_kib_s: self.received_kib_s,
            sent_kib_s: self.sent_kib_s,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThroughputLabel {
    received_kib_s: u32,
    sent_kib_s: u32,
}

impl ThroughputLabel {
    #[must_use]
    pub fn text(self) -> String {
        format!(
            "{} KiB/s down {} KiB/s up",
            self.received_kib_s, self.sent_kib_s
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PowerSnapshot {
    percent: Option<u8>,
    plugged_in: bool,
}

impl PowerSnapshot {
    #[must_use]
    pub const fn new(percent: Option<u8>, plugged_in: bool) -> Self {
        Self {
            percent,
            plugged_in,
        }
    }

    #[must_use]
    pub fn label(self) -> String {
        match (self.percent, self.plugged_in) {
            (Some(percent), true) => format!("{percent}% AC"),
            (Some(percent), false) => format!("{percent}%"),
            (None, true) => "AC".to_owned(),
            (None, false) => "Power".to_owned(),
        }
    }

    #[must_use]
    pub const fn percent(self) -> Option<u8> {
        self.percent
    }

    #[must_use]
    pub const fn plugged_in(self) -> bool {
        self.plugged_in
    }
}
