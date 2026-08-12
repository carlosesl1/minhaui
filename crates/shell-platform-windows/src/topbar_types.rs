#![deny(unsafe_code)]

use shell_core::{CalendarDate, Popover, TopbarModuleKind};
use shell_renderer::DipPoint;

use crate::ObservedWindow;

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
    OpenPopover {
        popover: Popover,
        anchor: TopbarOverlayAnchor,
    },
    OpenOverflow {
        items: Vec<TopbarOverflowItem>,
    },
    OpenSearch,
    RedrawTopbar,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TopbarOverflowItem {
    kind: TopbarModuleKind,
    label: String,
}

impl TopbarOverflowItem {
    #[must_use]
    pub(crate) fn new(kind: TopbarModuleKind, label: String) -> Self {
        Self { kind, label }
    }

    #[must_use]
    pub(crate) const fn kind(&self) -> TopbarModuleKind {
        self.kind
    }

    #[must_use]
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TopbarOverlayAnchor {
    Module(TopbarModuleKind),
    Overflow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TopbarSnapshot {
    pub(crate) app_label: String,
    pub(crate) local_date: CalendarDate,
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
            app_label: "Desktop".to_owned(),
            local_date: CalendarDate::default(),
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
    pub fn with_app_label(mut self, label: &str) -> Self {
        self.app_label = if label.trim().is_empty() {
            "Desktop".to_owned()
        } else {
            label.trim().to_owned()
        };
        self
    }

    #[must_use]
    pub const fn with_local_date(mut self, date: CalendarDate) -> Self {
        self.local_date = date;
        self
    }

    #[must_use]
    pub fn app_label(&self) -> &str {
        &self.app_label
    }

    #[must_use]
    pub const fn local_date(&self) -> CalendarDate {
        self.local_date
    }

    #[must_use]
    pub const fn network(&self) -> &NetworkSnapshot {
        &self.network
    }

    #[must_use]
    pub const fn volume_percent(&self) -> u8 {
        self.volume_percent
    }

    #[must_use]
    pub const fn battery(&self) -> PowerSnapshot {
        self.battery
    }

    #[must_use]
    #[expect(
        dead_code,
        reason = "retained as a privacy-safe snapshot accessor for diagnostics"
    )]
    pub fn clock(&self) -> &str {
        &self.clock
    }
}

impl Default for TopbarSnapshot {
    fn default() -> Self {
        Self::privacy_safe_fixture()
    }
}

#[must_use]
pub(crate) fn foreground_app_label(windows: &[ObservedWindow]) -> String {
    windows
        .iter()
        .find(|window| window.foreground())
        .map(|window| display_app_id(window.app().as_str()))
        .unwrap_or_else(|| "Desktop".to_owned())
}

fn display_app_id(app_id: &str) -> String {
    let unpackaged = app_id
        .strip_suffix(".exe")
        .or_else(|| app_id.strip_suffix(".EXE"))
        .unwrap_or(app_id);
    let packaged = unpackaged.split('_').next().unwrap_or(unpackaged);
    let segment = packaged.rsplit('.').next().unwrap_or(packaged);
    let label = segment
        .strip_suffix("App")
        .filter(|value| !value.is_empty())
        .unwrap_or(segment);
    if label.is_empty() {
        "Desktop".to_owned()
    } else {
        label.to_owned()
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

    #[must_use]
    pub const fn received_kib_s(&self) -> u32 {
        self.received_kib_s
    }

    #[must_use]
    pub const fn sent_kib_s(&self) -> u32 {
        self.sent_kib_s
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
        format!("↓ {}K  ↑ {}K", self.received_kib_s, self.sent_kib_s)
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
