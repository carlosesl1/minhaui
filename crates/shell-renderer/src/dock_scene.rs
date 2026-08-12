use std::sync::Arc;

use shell_core::{DockItemId, WindowId};

use crate::dock_item_visual::DockItemVisual;

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
            item_size: 36.0,
            spacing: 9.0,
            padding: 8.0,
            magnified_item_size: 43.92,
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

#[derive(Clone, Debug, PartialEq)]
pub struct DockScene {
    config: DockLayoutConfig,
    items: Arc<[DockItemVisual]>,
    hovered_item: Option<u64>,
    hover_position_x: Option<f32>,
    hover_strength: f32,
    material_motion_strength: f32,
    pressed_item: Option<u64>,
    dragged_item: Option<u64>,
    drag_position_x: Option<f32>,
    drag_insertion_x: Option<f32>,
    drag_target_valid: bool,
    item_offsets: Vec<(u64, f32)>,
    focused_item: Option<u64>,
    window_previews: Vec<WindowPreviewVisual>,
}

impl DockScene {
    #[must_use]
    pub fn new(config: DockLayoutConfig, items: Vec<DockItemVisual>) -> Self {
        Self::from_shared_items(config, items.into())
    }

    #[must_use]
    pub fn from_shared_items(config: DockLayoutConfig, items: Arc<[DockItemVisual]>) -> Self {
        Self {
            config,
            items,
            hovered_item: None,
            hover_position_x: None,
            hover_strength: 0.0,
            material_motion_strength: 0.0,
            pressed_item: None,
            dragged_item: None,
            drag_position_x: None,
            drag_insertion_x: None,
            drag_target_valid: true,
            item_offsets: Vec::new(),
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
    pub const fn hover_strength(&self) -> f32 {
        self.hover_strength
    }

    #[must_use]
    pub const fn hover_position_x(&self) -> Option<f32> {
        self.hover_position_x
    }

    #[must_use]
    pub const fn material_motion_strength(&self) -> f32 {
        self.material_motion_strength
    }

    #[must_use]
    pub const fn focused_item(&self) -> Option<u64> {
        self.focused_item
    }

    #[must_use]
    pub const fn pressed_item(&self) -> Option<u64> {
        self.pressed_item
    }

    #[must_use]
    pub const fn dragged_item(&self) -> Option<u64> {
        self.dragged_item
    }

    #[must_use]
    pub const fn drag_position_x(&self) -> Option<f32> {
        self.drag_position_x
    }

    #[must_use]
    pub const fn drag_insertion_x(&self) -> Option<f32> {
        self.drag_insertion_x
    }

    #[must_use]
    pub const fn drag_target_valid(&self) -> bool {
        self.drag_target_valid
    }

    #[must_use]
    pub fn item_offset(&self, id: u64) -> f32 {
        self.item_offsets
            .iter()
            .find_map(|(item, offset)| (*item == id).then_some(*offset))
            .unwrap_or(0.0)
    }

    #[must_use]
    pub const fn with_hovered_item(mut self, hovered_item: Option<u64>) -> Self {
        self.hovered_item = hovered_item;
        self.hover_strength = if hovered_item.is_some() { 1.0 } else { 0.0 };
        self
    }

    #[must_use]
    pub fn with_hover_strength(mut self, hover_strength: f32) -> Self {
        self.hover_strength = hover_strength.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn with_hover_position_x(mut self, hover_position_x: f32) -> Self {
        self.hover_position_x = hover_position_x.is_finite().then_some(hover_position_x);
        self
    }

    #[must_use]
    pub fn with_material_motion_strength(mut self, strength: f32) -> Self {
        self.material_motion_strength = if strength.is_finite() {
            strength.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }

    #[must_use]
    pub const fn with_focused_item(mut self, focused_item: Option<u64>) -> Self {
        self.focused_item = focused_item;
        self
    }

    #[must_use]
    pub const fn with_pressed_item(mut self, pressed_item: Option<u64>) -> Self {
        self.pressed_item = pressed_item;
        self
    }

    #[must_use]
    pub fn with_dragged_item(mut self, dragged_item: Option<u64>, position_x: f32) -> Self {
        self.dragged_item = dragged_item;
        self.drag_position_x = position_x.is_finite().then_some(position_x);
        self
    }

    #[must_use]
    pub fn with_drag_target(mut self, insertion_x: f32, valid: bool) -> Self {
        self.drag_insertion_x = insertion_x.is_finite().then_some(insertion_x);
        self.drag_target_valid = valid;
        self
    }

    #[must_use]
    pub fn with_item_offsets(mut self, offsets: Vec<(u64, f32)>) -> Self {
        self.item_offsets = offsets
            .into_iter()
            .filter(|(_, offset)| offset.is_finite())
            .collect();
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
