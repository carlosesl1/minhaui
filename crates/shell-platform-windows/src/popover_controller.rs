#![deny(unsafe_code)]

use std::error::Error;
use std::fmt;

use shell_core::Popover;
use shell_renderer::{
    DipPoint, DipRect, PopoverContentState, PopoverLayoutStyle, PopoverRow, PopoverScene,
    layout_popover_scene,
};

use crate::{
    BackgroundAppId, PopoverAction, PopoverDataError, PopoverDataProvider, PopoverItem, PopoverKey,
    PopoverLoadState, QueuedPopoverAction, SessionAction,
};

pub struct PopoverController {
    active: Option<ActivePopover>,
    calendar_offset: i16,
    load_generation: u64,
    external_active: Option<BackgroundAppId>,
}

const COMPACT_MAX_VISIBLE_ROWS: usize = 17;
const BACKGROUND_APPS_MAX_VISIBLE_ROWS: usize = 8;

const fn max_visible_rows(kind: Popover) -> usize {
    match kind {
        Popover::BackgroundApps => BACKGROUND_APPS_MAX_VISIBLE_ROWS,
        _ => COMPACT_MAX_VISIBLE_ROWS,
    }
}

impl Default for PopoverController {
    fn default() -> Self {
        Self::new()
    }
}

impl PopoverController {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: None,
            calendar_offset: 0,
            load_generation: 0,
            external_active: None,
        }
    }

    #[must_use]
    pub const fn active_kind(&self) -> Option<Popover> {
        match &self.active {
            Some(active) => Some(active.kind),
            None => None,
        }
    }

    pub fn dismiss(&mut self) -> Vec<QueuedPopoverAction> {
        self.load_generation = self.load_generation.wrapping_add(1);
        self.external_active = None;
        if self.active.take().is_some() {
            vec![QueuedPopoverAction::Dismiss]
        } else {
            Vec::new()
        }
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "privacy-safe convenience path is retained for tests"
        )
    )]
    pub fn open<P: PopoverDataProvider>(
        &mut self,
        kind: Popover,
        provider: &P,
    ) -> Result<Vec<QueuedPopoverAction>, PopoverControllerError> {
        self.open_with_snapshot(kind, provider, &crate::TopbarSnapshot::default())
    }

    pub fn open_with_snapshot<P: PopoverDataProvider>(
        &mut self,
        kind: Popover,
        provider: &P,
        snapshot: &crate::TopbarSnapshot,
    ) -> Result<Vec<QueuedPopoverAction>, PopoverControllerError> {
        if kind == Popover::Calendar && self.active_kind() != Some(Popover::Calendar) {
            self.calendar_offset = 0;
        }
        self.begin_loading(kind);
        let payload = provider
            .load(kind, snapshot, self.calendar_offset)
            .unwrap_or_else(|error| PopoverLoadState::Error(error.to_string()).into_payload(kind));
        self.active = Some(ActivePopover::from_payload(payload)?);
        Ok(vec![QueuedPopoverAction::Redraw])
    }

    pub fn begin_loading(&mut self, kind: Popover) -> u64 {
        self.load_generation = self.load_generation.wrapping_add(1);
        self.external_active = None;
        self.active = Some(ActivePopover::loading(kind));
        self.load_generation
    }

    /// Marks a stable background-app row as externally active while its
    /// application-owned menu is open.  The ID is resolved to an index only
    /// when a BackgroundApps popover is active; no row geometry is changed.
    #[allow(
        dead_code,
        reason = "external menu lifecycle is consumed by the native adapter incrementally"
    )]
    pub fn mark_external_active(&mut self, app: BackgroundAppId) -> Vec<QueuedPopoverAction> {
        if !self.has_background_app(app) {
            return Vec::new();
        }
        if self.external_active == Some(app) {
            return Vec::new();
        }
        self.external_active = Some(app);
        vec![QueuedPopoverAction::Redraw]
    }

    #[allow(
        dead_code,
        reason = "external menu lifecycle is consumed by the native adapter incrementally"
    )]
    pub fn clear_external_active(&mut self) -> Vec<QueuedPopoverAction> {
        if self.external_active.take().is_some() {
            vec![QueuedPopoverAction::Redraw]
        } else {
            Vec::new()
        }
    }

    #[must_use]
    #[allow(
        dead_code,
        reason = "external menu lifecycle is consumed by the native adapter incrementally"
    )]
    pub const fn external_active(&self) -> Option<BackgroundAppId> {
        self.external_active
    }

    /// Restores focus by the stable application ID.  An index captured before
    /// a reload is intentionally not accepted, so stale worker results cannot
    /// move focus to another row.
    #[allow(
        dead_code,
        reason = "external menu lifecycle is consumed by the native adapter incrementally"
    )]
    pub fn restore_focus(&mut self, app: BackgroundAppId) -> Vec<QueuedPopoverAction> {
        let Some(active) = self.active.as_mut() else {
            return Vec::new();
        };
        if active.kind != Popover::BackgroundApps {
            return Vec::new();
        }
        let Some(index) = active
            .items
            .iter()
            .position(|item| background_app_id(item) == Some(app))
        else {
            return Vec::new();
        };
        if !active.items[index].enabled() || active.focused == Some(index) {
            return Vec::new();
        }
        active.focused = Some(index);
        active.pending_confirmation = None;
        vec![QueuedPopoverAction::Redraw]
    }

    #[must_use]
    #[allow(
        dead_code,
        reason = "external menu lifecycle is consumed by the native adapter incrementally"
    )]
    pub const fn load_generation(&self) -> u64 {
        self.load_generation
    }

    pub fn complete_background_apps(
        &mut self,
        generation: u64,
        result: Result<Vec<PopoverItem>, PopoverDataError>,
    ) -> bool {
        if generation != self.load_generation || self.active_kind() != Some(Popover::BackgroundApps)
        {
            return false;
        }

        self.external_active = None;

        let state = match result {
            Ok(items) if items.is_empty() => PopoverLoadState::Empty,
            Ok(items) => PopoverLoadState::Ready(items),
            Err(_) => {
                PopoverLoadState::Error("Aplicativos em segundo plano indisponíveis".to_owned())
            }
        };
        let payload = crate::PopoverPayload::new(Popover::BackgroundApps, state);
        self.active = ActivePopover::from_payload(payload).ok();
        self.active.is_some()
    }

    pub fn reload<P: PopoverDataProvider>(
        &mut self,
        provider: &P,
        snapshot: &crate::TopbarSnapshot,
    ) -> Result<Vec<QueuedPopoverAction>, PopoverControllerError> {
        let Some(kind) = self.active_kind() else {
            return Ok(Vec::new());
        };
        self.open_with_snapshot(kind, provider, snapshot)
    }

    pub fn handle_key(&mut self, key: PopoverKey) -> Vec<QueuedPopoverAction> {
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        match key {
            PopoverKey::Next => active.focus_delta(1),
            PopoverKey::Previous => active.focus_delta(-1),
            PopoverKey::Activate => {
                let actions = active.activate();
                self.apply_internal_actions(&actions);
                actions
            }
            PopoverKey::ContextMenu => active.context_activate(),
            PopoverKey::Escape => self.dismiss(),
        }
    }

    pub fn handle_scroll(&mut self, rows: isize) -> Vec<QueuedPopoverAction> {
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        let maximum = active
            .items
            .len()
            .saturating_sub(max_visible_rows(active.kind));
        active.scroll_offset = active
            .scroll_offset
            .saturating_add_signed(rows)
            .min(maximum);
        vec![QueuedPopoverAction::Redraw]
    }

    pub fn handle_pointer(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedPopoverAction> {
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        let scene = active.scene(self.external_active);
        let Some(index) = layout_popover_scene(&scene, surface).hit_test(point) else {
            return Vec::new();
        };
        if !active.items[index].enabled() {
            return Vec::new();
        }
        if active.focused != Some(index) {
            active.pending_confirmation = None;
        }
        active.focused = Some(index);
        let actions = active.activate();
        self.apply_internal_actions(&actions);
        actions
    }

    pub fn handle_pointer_context(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedPopoverAction> {
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        if active.kind != Popover::BackgroundApps {
            return Vec::new();
        }
        let scene = active.scene(self.external_active);
        let Some(index) = layout_popover_scene(&scene, surface).hit_test(point) else {
            return Vec::new();
        };
        if !active.items[index].enabled() {
            return Vec::new();
        }
        if active.focused != Some(index) {
            active.pending_confirmation = None;
        }
        active.focused = Some(index);
        active.context_activate()
    }

    pub fn handle_pointer_context_pressed(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedPopoverAction> {
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        if active.kind != Popover::BackgroundApps {
            return Vec::new();
        }
        let scene = active.scene(self.external_active);
        let Some(index) = layout_popover_scene(&scene, surface).hit_test(point) else {
            return Vec::new();
        };
        if !active.items[index].enabled() {
            return Vec::new();
        }
        if active.focused != Some(index) {
            active.pending_confirmation = None;
        }
        active.focused = Some(index);
        vec![QueuedPopoverAction::Redraw]
    }

    pub fn handle_pointer_move(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedPopoverAction> {
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        let scene = active.scene(self.external_active);
        let hovered = layout_popover_scene(&scene, surface)
            .hit_test(point)
            .filter(|index| active.items[*index].enabled());
        if active.focused == hovered {
            return Vec::new();
        }
        active.focused = hovered;
        active.pending_confirmation = None;
        vec![QueuedPopoverAction::Redraw]
    }

    #[must_use]
    pub fn scene(&self) -> Option<PopoverScene> {
        self.active
            .as_ref()
            .map(|active| active.scene(self.external_active))
    }

    #[allow(
        dead_code,
        reason = "external menu lifecycle is consumed by the native adapter incrementally"
    )]
    fn has_background_app(&self, app: BackgroundAppId) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.kind == Popover::BackgroundApps
                && active
                    .items
                    .iter()
                    .any(|item| background_app_id(item) == Some(app))
        })
    }

    fn apply_internal_actions(&mut self, actions: &[QueuedPopoverAction]) {
        for action in actions {
            if let QueuedPopoverAction::TypedIntent(intent) = action {
                match intent {
                    PopoverAction::CalendarPrevious => {
                        self.calendar_offset = self.calendar_offset.saturating_sub(1);
                    }
                    PopoverAction::CalendarToday => self.calendar_offset = 0,
                    PopoverAction::CalendarNext => {
                        self.calendar_offset = self.calendar_offset.saturating_add(1);
                    }
                    _ => {}
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ActivePopover {
    kind: Popover,
    state: PopoverContentState,
    items: Vec<PopoverItem>,
    focused: Option<usize>,
    scroll_offset: usize,
    status_text: String,
    pending_confirmation: Option<SessionAction>,
}

impl ActivePopover {
    fn loading(kind: Popover) -> Self {
        Self {
            kind,
            state: PopoverContentState::Loading,
            items: Vec::new(),
            focused: None,
            scroll_offset: 0,
            status_text: loading_text(kind).to_owned(),
            pending_confirmation: None,
        }
    }

    fn from_payload(payload: crate::PopoverPayload) -> Result<Self, PopoverControllerError> {
        let kind = payload.kind();
        let (state, items, status_text) = match payload.state() {
            PopoverLoadState::Loading => (
                PopoverContentState::Loading,
                Vec::new(),
                loading_text(kind).to_owned(),
            ),
            PopoverLoadState::Ready(items) => {
                (PopoverContentState::Ready, items.clone(), String::new())
            }
            PopoverLoadState::Empty => (
                PopoverContentState::Empty,
                Vec::new(),
                empty_text(kind).to_owned(),
            ),
            PopoverLoadState::Error(message) => {
                (PopoverContentState::Error, Vec::new(), message.clone())
            }
            PopoverLoadState::Offline(items) => (
                PopoverContentState::Offline,
                items.clone(),
                "Offline".to_owned(),
            ),
        };
        Ok(Self {
            kind,
            state,
            focused: first_enabled(&items),
            items,
            scroll_offset: 0,
            status_text,
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
        let next = enabled[next];
        if self.focused != Some(next) {
            self.pending_confirmation = None;
        }
        self.focused = Some(next);
        let max_visible_rows = max_visible_rows(self.kind);
        if next < self.scroll_offset {
            self.scroll_offset = next;
        } else if next >= self.scroll_offset + max_visible_rows {
            self.scroll_offset = next + 1 - max_visible_rows;
        }
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
            Some(
                action @ (PopoverAction::CalendarPrevious
                | PopoverAction::CalendarToday
                | PopoverAction::CalendarNext),
            ) => vec![
                QueuedPopoverAction::TypedIntent(action.clone()),
                QueuedPopoverAction::Reload,
            ],
            Some(action) => vec![QueuedPopoverAction::TypedIntent(action.clone())],
            None => Vec::new(),
        }
    }

    fn context_activate(&self) -> Vec<QueuedPopoverAction> {
        if self.kind != Popover::BackgroundApps {
            return Vec::new();
        }
        let Some(index) = self.focused else {
            return Vec::new();
        };
        match self.items[index].action() {
            Some(PopoverAction::OpenBackgroundApp(id)) => vec![QueuedPopoverAction::TypedIntent(
                PopoverAction::OpenBackgroundAppContextMenu(*id),
            )],
            _ => Vec::new(),
        }
    }

    fn scene(&self, external_active: Option<BackgroundAppId>) -> PopoverScene {
        let rows = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let detail = if self.pending_confirmation.is_some() && self.focused == Some(index) {
                    "Activate again to confirm"
                } else {
                    item.detail()
                };
                let mut row = PopoverRow::new(item.label(), detail, item.enabled())
                    .with_icon_source(item.icon_source().map(str::to_owned));
                if let Some(glyph) = item.icon_glyph() {
                    row = row.with_icon_glyph(glyph);
                }
                if item.section_start() {
                    row = row.with_section_start();
                }
                row
            })
            .collect();
        let layout_style = match self.kind {
            Popover::SystemMenu => PopoverLayoutStyle::SystemPanel,
            Popover::BackgroundApps => PopoverLayoutStyle::BalancedApps,
            _ => PopoverLayoutStyle::Compact,
        };
        let external_active_row = (self.kind == Popover::BackgroundApps)
            .then(|| {
                external_active.and_then(|app| {
                    self.items
                        .iter()
                        .position(|item| background_app_id(item) == Some(app))
                })
            })
            .flatten();
        let mut scene =
            PopoverScene::new(self.kind, title(self.kind), self.state, rows, self.focused)
                .with_scroll_offset(self.scroll_offset)
                .with_status_text(&self.status_text)
                .with_layout_style(layout_style)
                .with_external_active_row(external_active_row);
        if self.kind == Popover::BackgroundApps {
            scene = scene.with_header_detail(&self.items.len().to_string());
        }
        scene
    }
}

fn first_enabled(items: &[PopoverItem]) -> Option<usize> {
    items.iter().position(PopoverItem::enabled)
}

fn background_app_id(item: &PopoverItem) -> Option<BackgroundAppId> {
    match item.action() {
        Some(PopoverAction::OpenBackgroundApp(app))
        | Some(PopoverAction::OpenBackgroundAppContextMenu(app)) => Some(*app),
        _ => None,
    }
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
        Popover::SystemMenu => "Minha UI",
        Popover::Calendar => "Calendar",
        Popover::Network => "Network",
        Popover::Volume => "Audio",
        Popover::Power => "Power",
        Popover::Notifications => "Control Center",
        Popover::QuickSettings => "Controls",
        Popover::BackgroundApps => "Aplicativos em segundo plano",
    }
}

const fn loading_text(kind: Popover) -> &'static str {
    if matches!(kind, Popover::BackgroundApps) {
        "Carregando aplicativos…"
    } else {
        "Loading"
    }
}

const fn empty_text(kind: Popover) -> &'static str {
    if matches!(kind, Popover::BackgroundApps) {
        "Nenhum aplicativo em segundo plano"
    } else {
        "Empty"
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
    #[expect(
        dead_code,
        reason = "reserved for typed popover adapter failures at the controller boundary"
    )]
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
