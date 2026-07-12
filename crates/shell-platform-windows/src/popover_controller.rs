#![deny(unsafe_code)]

use std::error::Error;
use std::fmt;

use shell_core::Popover;
use shell_renderer::{PopoverContentState, PopoverRow, PopoverScene};

use crate::{
    PopoverAction, PopoverDataError, PopoverDataProvider, PopoverItem, PopoverKey,
    PopoverLoadState, QueuedPopoverAction, SessionAction,
};

pub struct PopoverController {
    active: Option<ActivePopover>,
}

impl Default for PopoverController {
    fn default() -> Self {
        Self::new()
    }
}

impl PopoverController {
    #[must_use]
    pub const fn new() -> Self {
        Self { active: None }
    }

    #[must_use]
    pub const fn active_kind(&self) -> Option<Popover> {
        match &self.active {
            Some(active) => Some(active.kind),
            None => None,
        }
    }

    pub fn open<P: PopoverDataProvider>(
        &mut self,
        kind: Popover,
        provider: &P,
    ) -> Result<Vec<QueuedPopoverAction>, PopoverControllerError> {
        self.active = Some(ActivePopover::loading(kind));
        let payload = provider
            .load(kind)
            .unwrap_or_else(|error| PopoverLoadState::Error(error.to_string()).into_payload(kind));
        self.active = Some(ActivePopover::from_payload(payload)?);
        Ok(vec![QueuedPopoverAction::Redraw])
    }

    pub fn handle_key(&mut self, key: PopoverKey) -> Vec<QueuedPopoverAction> {
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        match key {
            PopoverKey::Next => active.focus_delta(1),
            PopoverKey::Previous => active.focus_delta(-1),
            PopoverKey::Activate => active.activate(),
            PopoverKey::Escape => {
                self.active = None;
                vec![QueuedPopoverAction::Dismiss]
            }
        }
    }

    #[must_use]
    pub fn scene(&self) -> Option<PopoverScene> {
        self.active.as_ref().map(ActivePopover::scene)
    }
}

#[derive(Clone, Debug)]
struct ActivePopover {
    kind: Popover,
    state: PopoverContentState,
    items: Vec<PopoverItem>,
    focused: Option<usize>,
    pending_confirmation: Option<SessionAction>,
}

impl ActivePopover {
    const fn loading(kind: Popover) -> Self {
        Self {
            kind,
            state: PopoverContentState::Loading,
            items: Vec::new(),
            focused: None,
            pending_confirmation: None,
        }
    }

    fn from_payload(payload: crate::PopoverPayload) -> Result<Self, PopoverControllerError> {
        let kind = payload.kind();
        let (state, items) = match payload.state() {
            PopoverLoadState::Loading => (PopoverContentState::Loading, Vec::new()),
            PopoverLoadState::Ready(items) => (PopoverContentState::Ready, items.clone()),
            PopoverLoadState::Empty => (PopoverContentState::Empty, Vec::new()),
            PopoverLoadState::Error(_) => (PopoverContentState::Error, Vec::new()),
            PopoverLoadState::Offline(items) => (PopoverContentState::Offline, items.clone()),
        };
        Ok(Self {
            kind,
            state,
            focused: first_enabled(&items),
            items,
            pending_confirmation: None,
        })
    }

    fn focus_delta(&mut self, delta: isize) -> Vec<QueuedPopoverAction> {
        let enabled = enabled_indexes(&self.items);
        if enabled.is_empty() {
            return Vec::new();
        }
        let current = self.focused.unwrap_or(enabled[0]);
        let position = enabled
            .iter()
            .position(|index| *index == current)
            .unwrap_or(0);
        let next = position.saturating_add_signed(delta).min(enabled.len() - 1);
        self.focused = Some(enabled[next]);
        vec![QueuedPopoverAction::Redraw]
    }

    fn activate(&mut self) -> Vec<QueuedPopoverAction> {
        let Some(index) = self.focused else {
            return Vec::new();
        };
        match self.items[index].action() {
            Some(PopoverAction::ConfirmSession(action))
                if self.pending_confirmation != Some(*action) =>
            {
                self.pending_confirmation = Some(*action);
                vec![QueuedPopoverAction::RequestConfirmation(*action)]
            }
            Some(action) => vec![QueuedPopoverAction::TypedIntent(action.clone())],
            None => Vec::new(),
        }
    }

    fn scene(&self) -> PopoverScene {
        let rows = self
            .items
            .iter()
            .map(|item| PopoverRow::new(item.label(), item.detail(), item.enabled()))
            .collect();
        PopoverScene::new(self.kind, title(self.kind), self.state, rows, self.focused)
    }
}

fn first_enabled(items: &[PopoverItem]) -> Option<usize> {
    items.iter().position(PopoverItem::enabled)
}

fn enabled_indexes(items: &[PopoverItem]) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| item.enabled().then_some(index))
        .collect()
}

const fn title(kind: Popover) -> &'static str {
    match kind {
        Popover::SystemMenu => "System",
        Popover::Calendar => "Calendar",
        Popover::Network => "Network",
        Popover::Volume => "Audio",
        Popover::Power => "Power",
        Popover::Notifications => "Control Center",
    }
}

trait IntoPayload {
    fn into_payload(self, kind: Popover) -> crate::PopoverPayload;
}

impl IntoPayload for PopoverLoadState {
    fn into_payload(self, kind: Popover) -> crate::PopoverPayload {
        crate::PopoverPayload::new(kind, self)
    }
}

#[derive(Debug)]
pub enum PopoverControllerError {
    Data(PopoverDataError),
}

impl fmt::Display for PopoverControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for PopoverControllerError {}
