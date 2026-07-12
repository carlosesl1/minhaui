use shell_core::{DockItemId, WindowId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DockAlignment {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockLayoutConfig {
    alignment: DockAlignment,
    item_size: f32,
    spacing: f32,
    padding: f32,
    magnified_item_size: f32,
}

impl DockLayoutConfig {
    #[must_use]
    pub const fn new(alignment: DockAlignment) -> Self {
        Self {
            alignment,
            item_size: 52.0,
            spacing: 8.0,
            padding: 14.0,
            magnified_item_size: 76.0,
        }
    }

    #[must_use]
    pub const fn alignment(self) -> DockAlignment {
        self.alignment
    }

    #[must_use]
    pub const fn item_size(self) -> f32 {
        self.item_size
    }

    #[must_use]
    pub const fn spacing(self) -> f32 {
        self.spacing
    }

    #[must_use]
    pub const fn padding(self) -> f32 {
        self.padding
    }

    #[must_use]
    pub const fn magnified_item_size(self) -> f32 {
        self.magnified_item_size
    }

    #[must_use]
    pub const fn with_item_size(mut self, value: f32) -> Self {
        self.item_size = value;
        self
    }

    #[must_use]
    pub const fn with_spacing(mut self, value: f32) -> Self {
        self.spacing = value;
        self
    }

    #[must_use]
    pub const fn with_padding(mut self, value: f32) -> Self {
        self.padding = value;
        self
    }

    #[must_use]
    pub const fn with_magnified_item_size(mut self, value: f32) -> Self {
        self.magnified_item_size = value;
        self
    }
}

impl Default for DockLayoutConfig {
    fn default() -> Self {
        Self::new(DockAlignment::Center)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DockItemVisualKind {
    App,
    Separator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunningIndicator {
    Stopped,
    Running,
    Focused,
    Minimized,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockItemVisual {
    id: u64,
    label: String,
    kind: DockItemVisualKind,
    indicator: RunningIndicator,
}

impl DockItemVisual {
    #[must_use]
    pub fn app(id: u64, label: &str, indicator: RunningIndicator) -> Self {
        Self {
            id,
            label: label.to_owned(),
            kind: DockItemVisualKind::App,
            indicator,
        }
    }

    #[must_use]
    pub fn separator(id: u64) -> Self {
        Self {
            id,
            label: String::new(),
            kind: DockItemVisualKind::Separator,
            indicator: RunningIndicator::Stopped,
        }
    }

    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub const fn kind(&self) -> DockItemVisualKind {
        self.kind
    }

    #[must_use]
    pub const fn indicator(&self) -> RunningIndicator {
        self.indicator
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockScene {
    config: DockLayoutConfig,
    items: Vec<DockItemVisual>,
    hovered_item: Option<u64>,
    focused_item: Option<u64>,
    window_previews: Vec<WindowPreviewVisual>,
}

impl DockScene {
    #[must_use]
    pub const fn new(config: DockLayoutConfig, items: Vec<DockItemVisual>) -> Self {
        Self {
            config,
            items,
            hovered_item: None,
            focused_item: None,
            window_previews: Vec::new(),
        }
    }

    #[must_use]
    pub const fn config(&self) -> DockLayoutConfig {
        self.config
    }

    #[must_use]
    pub fn items(&self) -> &[DockItemVisual] {
        &self.items
    }

    #[must_use]
    pub const fn hovered_item(&self) -> Option<u64> {
        self.hovered_item
    }

    #[must_use]
    pub const fn focused_item(&self) -> Option<u64> {
        self.focused_item
    }

    #[must_use]
    pub const fn with_hovered_item(mut self, hovered_item: Option<u64>) -> Self {
        self.hovered_item = hovered_item;
        self
    }

    #[must_use]
    pub const fn with_focused_item(mut self, focused_item: Option<u64>) -> Self {
        self.focused_item = focused_item;
        self
    }

    #[must_use]
    pub fn window_previews(&self) -> &[WindowPreviewVisual] {
        &self.window_previews
    }

    #[must_use]
    pub fn with_window_previews(mut self, previews: Vec<WindowPreviewVisual>) -> Self {
        self.window_previews = previews;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewUnavailableReason {
    CaptureRestricted,
    SourceUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowPreviewCapture {
    DwmThumbnail,
    Restricted(PreviewUnavailableReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowPreviewVisual {
    window: WindowId,
    item: DockItemId,
    capture: WindowPreviewCapture,
}

impl WindowPreviewVisual {
    #[must_use]
    pub const fn available(window: WindowId, item: DockItemId) -> Self {
        Self {
            window,
            item,
            capture: WindowPreviewCapture::DwmThumbnail,
        }
    }

    #[must_use]
    pub const fn restricted(
        window: WindowId,
        item: DockItemId,
        reason: PreviewUnavailableReason,
    ) -> Self {
        Self {
            window,
            item,
            capture: WindowPreviewCapture::Restricted(reason),
        }
    }

    #[must_use]
    pub const fn window(self) -> WindowId {
        self.window
    }

    #[must_use]
    pub const fn item(self) -> DockItemId {
        self.item
    }

    #[must_use]
    pub const fn capture(self) -> WindowPreviewCapture {
        self.capture
    }
}
