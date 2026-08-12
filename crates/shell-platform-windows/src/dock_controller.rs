#![deny(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use shell_core::{
    DockItemId, DockLayoutEntry, DockSeparatorId, RunningState, ShellEvent, ShellState, WindowId,
    reduce,
};
use shell_renderer::{DipRect, DockItemVisual, DockLayout, DockScene, WindowPreviewVisual};

use crate::dock_launch::initial_launch_targets;
use crate::dock_visuals::{DockIconSourceCache, visual_items};
use crate::{DockAnimator, DockControllerError, DockRuntimeConfig};

pub struct DockController {
    pub(crate) state: ShellState,
    pub(crate) config: DockRuntimeConfig,
    pub(crate) surface: DipRect,
    pub(crate) pressed_item: Option<DockItemId>,
    pub(crate) hovered_item: Option<DockItemId>,
    pub(crate) hover_strength: f32,
    pub(crate) focused_item: Option<DockItemId>,
    pub(crate) pointer_inside: bool,
    pub(crate) overlay_reveal_hold: bool,
    pub(crate) fullscreen_autohide: bool,
    pub(crate) visual_generation: u64,
    pub(crate) launch_targets: HashMap<DockItemId, String>,
    pub(crate) icon_sources: RefCell<DockIconSourceCache>,
    pub(crate) visual_items: RefCell<Option<Arc<[DockItemVisual]>>>,
    pub(crate) hit_layout: RefCell<Option<DockLayout>>,
    pub(crate) previews: HashMap<WindowId, crate::PreviewWindowState>,
    pub(crate) animator: DockAnimator,
    pub(crate) drag_session: Option<DockDragSession>,
    pub(crate) reorder_offsets: HashMap<u64, f32>,
}

#[derive(Clone, Debug)]
pub(crate) struct DockDragSession {
    pub(crate) entry: DockLayoutEntry,
    pub(crate) origin: shell_renderer::DipPoint,
    pub(crate) pointer_x: f32,
    pub(crate) original_layout: Vec<DockLayoutEntry>,
    pub(crate) preview_layout: Vec<DockLayoutEntry>,
    pub(crate) active: bool,
    pub(crate) target_valid: bool,
}

impl DockController {
    pub fn new(state: ShellState, config: DockRuntimeConfig) -> Result<Self, DockControllerError> {
        let launch_targets = initial_launch_targets(&state);
        let mut controller = Self {
            state,
            config,
            surface: DipRect::new(0.0, 0.0, 1.0, 1.0),
            pressed_item: None,
            hovered_item: None,
            hover_strength: 0.0,
            focused_item: None,
            pointer_inside: false,
            overlay_reveal_hold: false,
            fullscreen_autohide: false,
            visual_generation: 0,
            launch_targets,
            icon_sources: RefCell::new(DockIconSourceCache::default()),
            visual_items: RefCell::new(None),
            hit_layout: RefCell::new(None),
            previews: HashMap::new(),
            animator: DockAnimator::new(),
            drag_session: None,
            reorder_offsets: HashMap::new(),
        };
        if config.autohide() {
            controller.apply(ShellEvent::EnableAutohide(true))?;
        }
        Ok(controller)
    }

    #[must_use]
    pub const fn state(&self) -> &ShellState {
        &self.state
    }

    #[must_use]
    pub const fn config(&self) -> DockRuntimeConfig {
        self.config
    }

    #[must_use]
    #[cfg(test)]
    pub const fn animator(&self) -> DockAnimator {
        self.animator
    }

    #[must_use]
    pub const fn visual_generation(&self) -> u64 {
        self.visual_generation
    }

    #[must_use]
    #[cfg(test)]
    pub fn launch_target_for_item(&self, item: DockItemId) -> Option<&str> {
        self.launch_targets.get(&item).map(String::as_str)
    }

    pub fn update_surface(&mut self, surface: DipRect) {
        if self.surface == surface {
            return;
        }
        self.surface = surface;
        self.invalidate_hit_layout();
    }

    #[must_use]
    pub fn preferred_width_dip(&self) -> f32 {
        shell_renderer::dock_scene_max_width(&self.scene())
    }

    #[must_use]
    pub const fn hovered_item(&self) -> Option<DockItemId> {
        self.hovered_item
    }

    #[must_use]
    pub const fn focused_item(&self) -> Option<DockItemId> {
        self.focused_item
    }

    #[must_use]
    pub fn scene(&self) -> DockScene {
        let layout = self
            .drag_session
            .as_ref()
            .map_or(self.state.dock_layout(), |drag| {
                drag.preview_layout.as_slice()
            });
        let drag_presentation = self
            .drag_session
            .as_ref()
            .filter(|drag| drag.active)
            .map(|drag| {
                (
                    drag.entry,
                    drag.pointer_x,
                    self.drag_insertion_x(&drag.preview_layout, drag.entry)
                        .unwrap_or(drag.pointer_x),
                    drag.target_valid,
                )
            });
        let mut scene = DockScene::from_shared_items(
            self.config.layout(),
            self.visual_items_for_layout(layout),
        )
        .with_hovered_item(self.hovered_item.map(DockItemId::value))
        .with_hover_position_x(self.animator.position_x())
        .with_hover_strength(self.animator.strength())
        .with_material_motion_strength(self.animator.material_strength())
        .with_pressed_item(self.pressed_item.map(DockItemId::value))
        .with_focused_item(self.focused_item.map(DockItemId::value))
        .with_item_offsets(self.sorted_reorder_offsets())
        .with_window_previews(self.hovered_window_preview());
        if let Some((entry, pointer_x, insertion_x, target_valid)) = drag_presentation {
            scene = scene
                .with_dragged_item(Some(entry.visual_id()), pointer_x)
                .with_drag_target(insertion_x, target_valid);
        }
        scene
    }

    fn drag_insertion_x(&self, layout: &[DockLayoutEntry], entry: DockLayoutEntry) -> Option<f32> {
        let scene = self.scene_for_layout(layout);
        let item = shell_renderer::layout_dock_scene(&scene, self.surface)
            .items()
            .iter()
            .find(|item| item.id() == entry.visual_id())?
            .bounds();
        Some(item.x - self.config.layout().spacing() / 2.0)
    }

    pub(crate) fn next_separator_id(&self) -> DockSeparatorId {
        let mut value = u64::MAX - 1;
        loop {
            let used_by_app = self
                .state
                .dock_items()
                .iter()
                .any(|item| item.id().value() == value);
            let used_by_separator = self.state.dock_layout().iter().any(
                |entry| matches!(entry, DockLayoutEntry::Separator(id) if id.value() == value),
            );
            if !used_by_app && !used_by_separator {
                return DockSeparatorId::new(value);
            }
            value = value.saturating_sub(1);
        }
    }

    pub(crate) fn separator_visual_id_exists(&self, value: u64) -> bool {
        self.state
            .dock_layout()
            .iter()
            .any(|entry| matches!(entry, DockLayoutEntry::Separator(id) if id.value() == value))
    }

    pub fn advance_animation(&mut self, delta_seconds: f32) {
        let mut changed = self.animator.advance(delta_seconds);
        if delta_seconds.is_finite() && delta_seconds > 0.0 && !self.reorder_offsets.is_empty() {
            let retention = (-18.0 * delta_seconds.min(0.05)).exp();
            self.reorder_offsets.retain(|_, offset| {
                *offset *= retention;
                offset.abs() >= 0.1
            });
            changed = true;
        }
        if changed {
            self.visual_generation = self.visual_generation.wrapping_add(1);
        }
    }

    pub fn snap_animation_to_target(&mut self) {
        let had_reorder_motion = !self.reorder_offsets.is_empty();
        self.reorder_offsets.clear();
        if self.animator.snap_to_target() || had_reorder_motion {
            self.visual_generation = self.visual_generation.wrapping_add(1);
        }
    }

    #[must_use]
    pub fn animations_idle(&self) -> bool {
        self.animator.is_idle() && self.reorder_offsets.is_empty()
    }

    fn sorted_reorder_offsets(&self) -> Vec<(u64, f32)> {
        let mut offsets = self
            .reorder_offsets
            .iter()
            .map(|(id, offset)| (*id, *offset))
            .collect::<Vec<_>>();
        offsets.sort_by_key(|(id, _)| *id);
        offsets
    }

    pub(crate) fn qa_trace_line(&self) -> String {
        let items = self
            .state
            .dock_items()
            .iter()
            .enumerate()
            .map(|(index, item)| {
                format!(
                    "{}:{}:{}:{}:{}",
                    index,
                    item.id().value(),
                    item.app().as_str(),
                    format!("{:?}", item.pin()).to_ascii_lowercase(),
                    running_state(item.running()),
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        format!(
            "DOCK_STATE hover={:?} items={}",
            self.hovered_item.map(DockItemId::value),
            items
        )
    }

    pub(crate) fn apply(
        &mut self,
        event: ShellEvent,
    ) -> Result<Vec<crate::QueuedDockAction>, DockControllerError> {
        let transition = reduce(&self.state, event)?;
        self.state = transition.state;
        self.invalidate_model_caches();
        Ok(transition
            .effects
            .into_iter()
            .map(crate::QueuedDockAction::Effect)
            .collect())
    }

    pub(crate) fn invalidate_hit_layout(&self) {
        self.hit_layout.replace(None);
    }

    pub(crate) fn invalidate_model_caches(&self) {
        self.visual_items.replace(None);
        self.invalidate_hit_layout();
    }

    pub(crate) fn visual_items_for_layout(
        &self,
        layout: &[DockLayoutEntry],
    ) -> Arc<[DockItemVisual]> {
        let cacheable = self.drag_session.is_none() && layout == self.state.dock_layout();
        if cacheable {
            let mut cached_items = self.visual_items.borrow_mut();
            if let Some(items) = cached_items.as_ref() {
                return Arc::clone(items);
            }
            let mut icon_sources = self.icon_sources.borrow_mut();
            let items: Arc<[DockItemVisual]> =
                visual_items(&self.state, layout, &self.launch_targets, &mut icon_sources).into();
            *cached_items = Some(Arc::clone(&items));
            return items;
        }
        let mut icon_sources = self.icon_sources.borrow_mut();
        visual_items(&self.state, layout, &self.launch_targets, &mut icon_sources).into()
    }

    #[cfg(test)]
    pub(crate) fn hit_layout_cache_populated(&self) -> bool {
        self.hit_layout.borrow().is_some()
    }

    #[cfg(test)]
    pub(crate) fn visual_items_cache_strong_count(&self) -> usize {
        self.visual_items
            .borrow()
            .as_ref()
            .map_or(0, Arc::strong_count)
    }
}

impl DockController {
    fn hovered_window_preview(&self) -> Vec<WindowPreviewVisual> {
        let Some(hovered) = self.hovered_item else {
            return Vec::new();
        };
        let Some(window) = self
            .state
            .dock_items()
            .iter()
            .find(|item| item.id() == hovered)
            .and_then(|item| match item.running() {
                RunningState::Running { window, .. } => Some(*window),
                RunningState::Stopped => None,
            })
        else {
            return Vec::new();
        };
        match self
            .previews
            .get(&window)
            .map(crate::PreviewWindowState::capture)
        {
            Some(crate::PreviewCapture::DwmThumbnail) => {
                vec![WindowPreviewVisual::available(window, hovered)]
            }
            Some(crate::PreviewCapture::Restricted(reason)) => {
                vec![WindowPreviewVisual::restricted(window, hovered, reason)]
            }
            None => Vec::new(),
        }
    }
}

fn running_state(running: &shell_core::RunningState) -> String {
    match running {
        shell_core::RunningState::Stopped => "stopped".to_owned(),
        shell_core::RunningState::Running {
            window,
            focused,
            minimized,
        } => format!(
            "running:{:?}:focused={}:minimized={}",
            window, focused, minimized
        ),
    }
}
