#![deny(unsafe_code)]

use shell_core::{DockItemId, WindowId};

use crate::{DockIcon, WindowPreviewCapture};

pub const WINDOW_PREVIEW_PAGE_SIZE: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewSourceSize {
    width: u32,
    height: u32,
}

impl PreviewSourceSize {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewCardVisual {
    window: WindowId,
    title: String,
    capture: WindowPreviewCapture,
    focused: bool,
    minimized: bool,
    source_size: Option<PreviewSourceSize>,
}

impl PreviewCardVisual {
    #[must_use]
    pub fn new(window: WindowId, title: &str, capture: WindowPreviewCapture) -> Self {
        Self {
            window,
            title: title.to_owned(),
            capture,
            focused: false,
            minimized: false,
            source_size: None,
        }
    }

    #[must_use]
    pub const fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    #[must_use]
    pub const fn with_minimized(mut self, minimized: bool) -> Self {
        self.minimized = minimized;
        self
    }

    #[must_use]
    pub const fn with_source_size(mut self, source_size: PreviewSourceSize) -> Self {
        self.source_size = Some(source_size);
        self
    }

    #[must_use]
    pub const fn window(&self) -> WindowId {
        self.window
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub const fn capture(&self) -> WindowPreviewCapture {
        self.capture
    }

    #[must_use]
    pub const fn focused(&self) -> bool {
        self.focused
    }

    #[must_use]
    pub const fn minimized(&self) -> bool {
        self.minimized
    }

    #[must_use]
    pub const fn source_size(&self) -> Option<PreviewSourceSize> {
        self.source_size
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowPreviewScene {
    item: DockItemId,
    app_label: String,
    icon: DockIcon,
    cards: Vec<PreviewCardVisual>,
    page: usize,
    hovered_card: Option<WindowId>,
    hovered_close: Option<WindowId>,
    focused_card: Option<WindowId>,
    reduced_motion: bool,
}

impl WindowPreviewScene {
    #[must_use]
    pub fn new(item: DockItemId, app_label: &str, cards: Vec<PreviewCardVisual>) -> Self {
        Self {
            item,
            app_label: app_label.to_owned(),
            icon: DockIcon::SystemFallback,
            cards,
            page: 0,
            hovered_card: None,
            hovered_close: None,
            focused_card: None,
            reduced_motion: false,
        }
    }

    #[must_use]
    pub fn with_icon(mut self, icon: DockIcon) -> Self {
        self.icon = icon;
        self
    }

    #[must_use]
    pub const fn with_page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    #[must_use]
    pub const fn with_hovered_card(mut self, window: Option<WindowId>) -> Self {
        self.hovered_card = window;
        self
    }

    #[must_use]
    pub const fn with_hovered_close(mut self, window: Option<WindowId>) -> Self {
        self.hovered_close = window;
        self
    }

    #[must_use]
    pub const fn with_focused_card(mut self, window: Option<WindowId>) -> Self {
        self.focused_card = window;
        self
    }

    #[must_use]
    pub const fn with_reduced_motion(mut self, reduced_motion: bool) -> Self {
        self.reduced_motion = reduced_motion;
        self
    }

    #[must_use]
    pub fn with_card_capture(mut self, window: WindowId, capture: WindowPreviewCapture) -> Self {
        if let Some(card) = self.cards.iter_mut().find(|card| card.window == window) {
            card.capture = capture;
        }
        self
    }

    #[must_use]
    pub const fn item(&self) -> DockItemId {
        self.item
    }

    #[must_use]
    pub fn app_label(&self) -> &str {
        &self.app_label
    }

    #[must_use]
    pub const fn icon(&self) -> &DockIcon {
        &self.icon
    }

    #[must_use]
    pub fn cards(&self) -> &[PreviewCardVisual] {
        &self.cards
    }

    #[must_use]
    pub const fn page(&self) -> usize {
        self.page
    }

    #[must_use]
    pub const fn hovered_card(&self) -> Option<WindowId> {
        self.hovered_card
    }

    #[must_use]
    pub const fn hovered_close(&self) -> Option<WindowId> {
        self.hovered_close
    }

    #[must_use]
    pub const fn focused_card(&self) -> Option<WindowId> {
        self.focused_card
    }

    #[must_use]
    pub const fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    #[must_use]
    pub fn page_count(&self) -> usize {
        self.cards.len().div_ceil(WINDOW_PREVIEW_PAGE_SIZE)
    }

    #[must_use]
    pub fn corrected_page(&self) -> usize {
        self.page_count().saturating_sub(1).min(self.page)
    }

    #[must_use]
    pub fn visible_cards(&self) -> &[PreviewCardVisual] {
        let start = self.corrected_page() * WINDOW_PREVIEW_PAGE_SIZE;
        let end = (start + WINDOW_PREVIEW_PAGE_SIZE).min(self.cards.len());
        &self.cards[start..end]
    }
}
