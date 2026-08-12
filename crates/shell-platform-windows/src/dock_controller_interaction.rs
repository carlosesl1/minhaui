#![deny(unsafe_code)]

use shell_core::{
    DockItem, DockItemId, DockLayoutEntry, Effect, PinState, RunningState, ShellEvent, reduce,
};
use shell_renderer::{DipPoint, DockLayout, layout_dock_scene};

use crate::dock_controller::DockDragSession;
use crate::dock_launch::dropped_launch_target;
use crate::{
    ContextMenuCommand, DockContextMenuItem, DockController, DockControllerError, DockKey,
    DockPointerPhase, DockPointerSample, PreviewAction, QueuedDockAction,
};

const DRAG_THRESHOLD_DIP: f32 = 4.0;

impl DockController {
    pub fn handle_pointer(
        &mut self,
        sample: DockPointerSample,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        self.pointer_inside = match sample.phase {
            DockPointerPhase::Exited | DockPointerPhase::Cancelled => false,
            DockPointerPhase::Dragged | DockPointerPhase::Released => {
                self.valid_drag_point(sample.point)
            }
            DockPointerPhase::Pressed | DockPointerPhase::Moved => true,
        };
        match sample.phase {
            DockPointerPhase::Pressed => {
                let entry = self
                    .hit_layout_entry(sample.point)
                    .or_else(|| self.hit_app(sample.point).map(DockLayoutEntry::App));
                self.drag_session = entry.map(|entry| {
                    let layout = match entry {
                        DockLayoutEntry::App(id)
                            if self.dock_item(id).is_some_and(|item| {
                                matches!(item.running(), RunningState::Running { .. })
                            }) =>
                        {
                            crate::dock_visuals::visual_layout_entries(
                                &self.state,
                                self.state.dock_layout(),
                            )
                        }
                        DockLayoutEntry::App(_) | DockLayoutEntry::Separator(_) => {
                            self.state.dock_layout().to_vec()
                        }
                    };
                    DockDragSession {
                        entry,
                        origin: sample.point,
                        pointer_x: sample.point.x,
                        original_layout: layout.clone(),
                        preview_layout: layout,
                        active: false,
                        target_valid: true,
                    }
                });
                self.set_pressed_item(self.hit_app(sample.point));
                Ok(Vec::new())
            }
            DockPointerPhase::Released => self.release_pointer(sample.point),
            DockPointerPhase::Dragged => self.drag(sample.point),
            DockPointerPhase::Cancelled => {
                self.cancel_drag();
                self.hide_after_external_release()
            }
            DockPointerPhase::Exited => {
                if self.drag_session.is_some() {
                    return Ok(Vec::new());
                }
                self.set_hovered_state(None, 0.0);
                self.animator.retarget_strength(0.0);
                if self.autohide_active() && !self.overlay_reveal_hold {
                    self.apply(ShellEvent::HideDock)
                } else {
                    Ok(Vec::new())
                }
            }
            DockPointerPhase::Moved => {
                let hovered_item = self.hit_app(sample.point);
                let hover_strength = self.hover_strength(sample.point);
                self.set_hovered_state(hovered_item, hover_strength);
                self.animator.retarget(sample.point.x, hover_strength);
                self.reveal_if_needed(sample.point)
            }
        }
    }

    pub fn hold_revealed_for_overlay(
        &mut self,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        self.overlay_reveal_hold = true;
        if self.autohide_active() {
            self.apply(ShellEvent::RevealDock)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn release_overlay_hold(&mut self) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        if !self.overlay_reveal_hold {
            return Ok(Vec::new());
        }
        self.overlay_reveal_hold = false;
        if self.autohide_active() && !self.pointer_inside {
            self.apply(ShellEvent::HideDock)
        } else {
            Ok(Vec::new())
        }
    }

    #[must_use]
    pub const fn overlay_hold_active(&self) -> bool {
        self.overlay_reveal_hold
    }

    pub fn handle_context_menu(
        &mut self,
        point: DipPoint,
        command: ContextMenuCommand,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let id = self.hit_app(point);
        let entry = self.hit_layout_entry(point);
        let movable_entry = entry.or_else(|| id.map(DockLayoutEntry::App));
        match command {
            ContextMenuCommand::Open => id.map_or(Ok(Vec::new()), |item| self.activate(item)),
            ContextMenuCommand::Unpin => {
                id.map_or(Ok(Vec::new()), |item| self.apply(ShellEvent::Unpin(item)))
            }
            ContextMenuCommand::Pin => id.map_or(Ok(Vec::new()), |item| {
                self.dock_item(item)
                    .cloned()
                    .map_or(Ok(Vec::new()), |entry| self.apply(ShellEvent::Pin(entry)))
            }),
            ContextMenuCommand::PreviewFocus => {
                self.handle_preview_action_at(point, PreviewAction::Focus)
            }
            ContextMenuCommand::PreviewClose => {
                self.handle_preview_action_at(point, PreviewAction::Close)
            }
            ContextMenuCommand::AddSeparator => {
                let separator = self.next_separator_id();
                let before = entry.and_then(|entry| self.entry_after(entry));
                self.apply(ShellEvent::AddDockSeparator { separator, before })
            }
            ContextMenuCommand::RemoveSeparator => match entry {
                Some(DockLayoutEntry::Separator(separator)) => {
                    self.apply(ShellEvent::RemoveDockSeparator(separator))
                }
                Some(DockLayoutEntry::App(_)) | None => Ok(Vec::new()),
            },
            ContextMenuCommand::MoveLeft => movable_entry.map_or(Ok(Vec::new()), |entry| {
                self.reorder_entry_by_delta(entry, -1)
            }),
            ContextMenuCommand::MoveRight => movable_entry.map_or(Ok(Vec::new()), |entry| {
                self.reorder_entry_by_delta(entry, 1)
            }),
            ContextMenuCommand::OpenTaskManager => {
                Ok(vec![QueuedDockAction::Launch("taskmgr.exe".to_owned())])
            }
            ContextMenuCommand::Quit => Ok(vec![QueuedDockAction::Quit]),
        }
    }

    #[must_use]
    pub fn context_menu_items(&self, point: DipPoint) -> Vec<DockContextMenuItem> {
        let mut items = Vec::new();
        let separator = matches!(
            self.hit_layout_entry(point),
            Some(DockLayoutEntry::Separator(_))
        );
        if separator {
            items.push(DockContextMenuItem::action(
                "Remove Separator",
                ContextMenuCommand::RemoveSeparator,
                true,
            ));
            items.push(DockContextMenuItem::separator());
        }
        if !separator
            && let Some(id) = self.hit_app(point)
            && let Some(item) = self.dock_item(id)
        {
            items.push(DockContextMenuItem::action(
                "Open",
                ContextMenuCommand::Open,
                true,
            ));
            let (label, command) = match item.pin() {
                PinState::Pinned => ("Remove from Dock", ContextMenuCommand::Unpin),
                PinState::Unpinned => ("Keep in Dock", ContextMenuCommand::Pin),
            };
            items.push(DockContextMenuItem::action(label, command, true));
            if matches!(item.running(), RunningState::Running { .. }) {
                items.push(DockContextMenuItem::separator());
                items.push(DockContextMenuItem::action(
                    "Close Window",
                    ContextMenuCommand::PreviewClose,
                    true,
                ));
            }
            items.push(DockContextMenuItem::separator());
        }
        let movable_entry = self
            .hit_layout_entry(point)
            .or_else(|| self.hit_app(point).map(DockLayoutEntry::App));
        if let Some(entry) = movable_entry
            && let Some((index, len)) = self.entry_group_position(entry)
        {
            items.push(DockContextMenuItem::action(
                "Move Left",
                ContextMenuCommand::MoveLeft,
                index > 0,
            ));
            items.push(DockContextMenuItem::action(
                "Move Right",
                ContextMenuCommand::MoveRight,
                index + 1 < len,
            ));
            items.push(DockContextMenuItem::separator());
        }
        items.push(DockContextMenuItem::action(
            "Add Separator",
            ContextMenuCommand::AddSeparator,
            true,
        ));
        items.push(DockContextMenuItem::separator());
        items.push(DockContextMenuItem::action(
            "Abrir Gerenciador de Tarefas",
            ContextMenuCommand::OpenTaskManager,
            true,
        ));
        items.push(DockContextMenuItem::action(
            "Quit Minha UI",
            ContextMenuCommand::Quit,
            true,
        ));
        items
    }

    pub fn handle_key(
        &mut self,
        key: DockKey,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        match key {
            DockKey::Next => {
                self.focus_delta(1);
                Ok(Vec::new())
            }
            DockKey::Previous => {
                self.focus_delta(-1);
                Ok(Vec::new())
            }
            DockKey::Preview => Ok(Vec::new()),
            DockKey::Activate => self
                .focused_item
                .map_or(Ok(Vec::new()), |item| self.activate(item)),
            DockKey::Escape => {
                if self.drag_session.is_some() {
                    self.cancel_drag();
                } else {
                    self.set_focused_item(None);
                }
                Ok(Vec::new())
            }
        }
    }

    pub fn handle_drop(
        &mut self,
        _point: DipPoint,
        path: &str,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let (app, launch_target) = dropped_launch_target(path)?;
        let id = self.next_item_id();
        let item = DockItem::pinned(id, app).with_launch_target(launch_target.clone());
        let actions = self.apply(ShellEvent::Pin(item))?;
        self.launch_targets.insert(id, launch_target);
        Ok(actions)
    }

    fn release(&mut self, point: DipPoint) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        if let Some(mut drag) = self.drag_session.take()
            && drag.active
        {
            self.set_pressed_item(None);
            self.visual_generation = self.visual_generation.wrapping_add(1);
            if !self.valid_drag_point(point) {
                self.retarget_reorder_offsets(
                    &drag.preview_layout,
                    &drag.original_layout,
                    drag.entry,
                );
                return Ok(Vec::new());
            }
            if matches!(drag.entry, DockLayoutEntry::App(id) if self.dock_item(id).is_none()) {
                self.retarget_reorder_offsets(
                    &drag.preview_layout,
                    &drag.original_layout,
                    drag.entry,
                );
                return Ok(Vec::new());
            }
            self.update_drag_preview(&mut drag, point);
            let dragged_item = match drag.entry {
                DockLayoutEntry::App(id) => self.dock_item(id).cloned(),
                DockLayoutEntry::Separator(_) => None,
            };
            let remains_running = self.drag_remains_in_running_group(&drag);
            if let Some(item) = dragged_item.as_ref()
                && remains_running
                && matches!(item.running(), RunningState::Running { .. })
            {
                if drag.preview_layout == drag.original_layout {
                    return Ok(Vec::new());
                }
                let mut next = self.state.clone();
                let mut effects = Vec::new();
                if item.pin() == PinState::Pinned {
                    let transition = reduce(&next, ShellEvent::Unpin(item.id()))?;
                    next = transition.state;
                    effects.extend(transition.effects);
                }
                let transition = reduce(
                    &next,
                    ShellEvent::ReorderDockItem {
                        item: item.id(),
                        before: self.unpinned_app_after(&drag),
                    },
                )?;
                self.state = transition.state;
                self.invalidate_model_caches();
                effects.extend(transition.effects);
                return Ok(effects.into_iter().map(QueuedDockAction::Effect).collect());
            }
            if let Some(item) = dragged_item
                && item.pin() == PinState::Unpinned
            {
                let before = self.persisted_entry_after(&drag);
                let mut next = self.state.clone();
                let mut effects = Vec::new();
                let transition = reduce(&next, ShellEvent::Pin(item))?;
                next = transition.state;
                effects.extend(transition.effects);
                let transition = reduce(
                    &next,
                    ShellEvent::ReorderDockEntry {
                        entry: drag.entry,
                        before,
                    },
                )?;
                self.state = transition.state;
                self.invalidate_model_caches();
                effects.extend(transition.effects);
                return Ok(effects.into_iter().map(QueuedDockAction::Effect).collect());
            }
            if drag.preview_layout == drag.original_layout {
                return Ok(Vec::new());
            }
            let before = self.persisted_entry_after(&drag);
            let transition = reduce(
                &self.state,
                ShellEvent::ReorderDockEntry {
                    entry: drag.entry,
                    before,
                },
            )?;
            self.state = transition.state;
            self.invalidate_model_caches();
            return Ok(transition
                .effects
                .into_iter()
                .map(QueuedDockAction::Effect)
                .collect());
        }
        self.drag_session = None;
        let released = self.hit_app(point);
        let pressed = self.pressed_item;
        self.set_pressed_item(None);
        if let Some(released_item) = released
            && pressed == Some(released_item)
        {
            self.activate(released_item)
        } else {
            Ok(Vec::new())
        }
    }

    fn release_pointer(
        &mut self,
        point: DipPoint,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let mut actions = self.release(point)?;
        actions.extend(self.hide_after_external_release()?);
        Ok(actions)
    }

    fn hide_after_external_release(
        &mut self,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        if self.config.autohide() && !self.pointer_inside && !self.overlay_reveal_hold {
            self.apply(ShellEvent::HideDock)
        } else {
            Ok(Vec::new())
        }
    }

    fn drag(&mut self, point: DipPoint) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let Some(mut drag) = self.drag_session.take() else {
            return Ok(Vec::new());
        };
        drag.pointer_x = point.x;
        let distance =
            ((point.x - drag.origin.x).powi(2) + (point.y - drag.origin.y).powi(2)).sqrt();
        if !drag.active && distance < DRAG_THRESHOLD_DIP {
            self.drag_session = Some(drag);
            return Ok(Vec::new());
        }
        drag.active = true;
        drag.target_valid = self.valid_drag_point(point);
        self.set_hovered_state(None, 0.0);
        self.animator.retarget_strength(0.0);
        if !drag.target_valid {
            self.drag_session = Some(drag);
            self.visual_generation = self.visual_generation.wrapping_add(1);
            return Ok(Vec::new());
        }
        self.update_drag_preview(&mut drag, point);
        self.drag_session = Some(drag);
        self.visual_generation = self.visual_generation.wrapping_add(1);
        Ok(Vec::new())
    }

    fn reveal_if_needed(
        &mut self,
        point: DipPoint,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let reveal_height = self.config.reveal_zone_height();
        let hidden_strip_hit =
            !self.state.dock().is_revealed() && point.y <= self.surface.y + reveal_height;
        let revealed_edge_hit = self.state.dock().is_revealed()
            && point.y >= self.surface.y + self.surface.height - reveal_height;
        if self.autohide_active() && (hidden_strip_hit || revealed_edge_hit) {
            self.apply(ShellEvent::RevealDock)
        } else {
            Ok(Vec::new())
        }
    }

    pub(crate) fn layout(&self) -> DockLayout {
        layout_dock_scene(&self.scene(), self.surface)
    }

    fn before_layout_entry(
        &self,
        point: DipPoint,
        moving: DockLayoutEntry,
        preview_layout: &[DockLayoutEntry],
    ) -> Option<DockLayoutEntry> {
        let scene = self.scene_for_layout(preview_layout);
        layout_dock_scene(&scene, self.surface)
            .items()
            .iter()
            .filter(|item| item.id() != moving.visual_id())
            .filter_map(|item| {
                preview_layout
                    .iter()
                    .find(|entry| entry.visual_id() == item.id())
                    .map(|entry| (*entry, item.bounds()))
            })
            .find(|(_, bounds)| point.x < bounds.x + bounds.width / 2.0)
            .map(|(entry, _)| entry)
    }

    fn update_drag_preview(&mut self, drag: &mut DockDragSession, point: DipPoint) {
        drag.pointer_x = point.x;
        let previous_layout = drag.preview_layout.clone();
        if !drag.preview_layout.contains(&drag.entry) {
            drag.preview_layout.push(drag.entry);
        }
        let before = self.before_layout_entry(point, drag.entry, &drag.preview_layout);
        move_preview_entry(&mut drag.preview_layout, drag.entry, before);
        if drag.preview_layout != previous_layout {
            self.retarget_reorder_offsets(&previous_layout, &drag.preview_layout, drag.entry);
        }
    }

    fn drag_remains_in_running_group(&self, drag: &DockDragSession) -> bool {
        let Some(entry_index) = drag
            .preview_layout
            .iter()
            .position(|entry| *entry == drag.entry)
        else {
            return false;
        };
        let synthetic_separator = drag.preview_layout.iter().position(|entry| {
            matches!(entry, DockLayoutEntry::Separator(_))
                && !self.state.dock_layout().contains(entry)
        });
        synthetic_separator.map_or(self.state.dock_layout().is_empty(), |separator_index| {
            entry_index > separator_index
        })
    }

    fn unpinned_app_after(&self, drag: &DockDragSession) -> Option<DockItemId> {
        let entry_index = drag
            .preview_layout
            .iter()
            .position(|entry| *entry == drag.entry)?;
        drag.preview_layout
            .iter()
            .skip(entry_index + 1)
            .find_map(|entry| match entry {
                DockLayoutEntry::App(id)
                    if self
                        .dock_item(*id)
                        .is_some_and(|item| item.pin() == PinState::Unpinned) =>
                {
                    Some(*id)
                }
                DockLayoutEntry::App(_) | DockLayoutEntry::Separator(_) => None,
            })
    }

    fn persisted_entry_after(&self, drag: &DockDragSession) -> Option<DockLayoutEntry> {
        let persisted_preview = drag
            .preview_layout
            .iter()
            .filter(|entry| **entry == drag.entry || self.state.dock_layout().contains(*entry))
            .copied()
            .collect::<Vec<_>>();
        persisted_preview
            .iter()
            .position(|entry| *entry == drag.entry)
            .and_then(|index| persisted_preview.get(index + 1))
            .copied()
    }

    pub(crate) fn scene_for_layout(&self, layout: &[DockLayoutEntry]) -> shell_renderer::DockScene {
        shell_renderer::DockScene::from_shared_items(
            self.config.layout(),
            self.visual_items_for_layout(layout),
        )
    }

    fn hit_layout_entry(&self, point: DipPoint) -> Option<DockLayoutEntry> {
        let id = self.hit_visual(point)?;
        self.state
            .dock_layout()
            .iter()
            .find(|entry| entry.visual_id() == id)
            .copied()
    }

    fn entry_after(&self, entry: DockLayoutEntry) -> Option<DockLayoutEntry> {
        self.state
            .dock_layout()
            .iter()
            .position(|current| *current == entry)
            .and_then(|index| self.state.dock_layout().get(index + 1))
            .copied()
    }

    fn cancel_drag(&mut self) {
        if let Some(drag) = self.drag_session.take() {
            if drag.active && drag.preview_layout != drag.original_layout {
                self.retarget_reorder_offsets(
                    &drag.preview_layout,
                    &drag.original_layout,
                    drag.entry,
                );
            }
            self.set_pressed_item(None);
            self.visual_generation = self.visual_generation.wrapping_add(1);
        }
    }

    fn retarget_reorder_offsets(
        &mut self,
        previous_layout: &[DockLayoutEntry],
        next_layout: &[DockLayoutEntry],
        moving: DockLayoutEntry,
    ) {
        let previous = layout_dock_scene(&self.scene_for_layout(previous_layout), self.surface);
        let next = layout_dock_scene(&self.scene_for_layout(next_layout), self.surface);
        for target in next
            .items()
            .iter()
            .filter(|item| item.id() != moving.visual_id())
        {
            let Some(source) = previous
                .items()
                .iter()
                .find(|item| item.id() == target.id())
            else {
                continue;
            };
            let current_x = source.bounds().x
                + self
                    .reorder_offsets
                    .get(&target.id())
                    .copied()
                    .unwrap_or(0.0);
            let offset = current_x - target.bounds().x;
            if offset.abs() >= 0.1 {
                self.reorder_offsets.insert(target.id(), offset);
            } else {
                self.reorder_offsets.remove(&target.id());
            }
        }
    }

    fn valid_drag_point(&self, point: DipPoint) -> bool {
        point.x >= self.surface.x
            && point.x <= self.surface.x + self.surface.width
            && point.y >= self.surface.y - 24.0
            && point.y <= self.surface.y + self.surface.height
    }

    fn entry_group_position(&self, entry: DockLayoutEntry) -> Option<(usize, usize)> {
        if let DockLayoutEntry::App(id) = entry
            && self
                .dock_item(id)
                .is_some_and(|item| item.pin() == PinState::Unpinned)
        {
            let group = self
                .state
                .dock_items()
                .iter()
                .filter(|item| item.pin() == PinState::Unpinned)
                .map(DockItem::id)
                .collect::<Vec<_>>();
            return group
                .iter()
                .position(|current| *current == id)
                .map(|index| (index, group.len()));
        }
        self.state
            .dock_layout()
            .iter()
            .position(|current| *current == entry)
            .map(|index| (index, self.state.dock_layout().len()))
    }

    fn reorder_entry_by_delta(
        &mut self,
        entry: DockLayoutEntry,
        delta: isize,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        if let DockLayoutEntry::App(id) = entry
            && self
                .dock_item(id)
                .is_some_and(|item| item.pin() == PinState::Unpinned)
        {
            let group = self
                .state
                .dock_items()
                .iter()
                .filter(|item| item.pin() == PinState::Unpinned)
                .map(DockItem::id)
                .collect::<Vec<_>>();
            let Some(index) = group.iter().position(|current| *current == id) else {
                return Ok(Vec::new());
            };
            let target = index
                .saturating_add_signed(delta)
                .min(group.len().saturating_sub(1));
            if target == index {
                return Ok(Vec::new());
            }
            let before = if target < index {
                group.get(target).copied()
            } else {
                group.get(target + 1).copied()
            };
            return self.apply(ShellEvent::ReorderDockItem { item: id, before });
        }
        let layout = self.state.dock_layout();
        let Some(index) = layout.iter().position(|current| *current == entry) else {
            return Ok(Vec::new());
        };
        let target = index
            .saturating_add_signed(delta)
            .min(layout.len().saturating_sub(1));
        if target == index {
            return Ok(Vec::new());
        }
        let before = if target < index {
            layout.get(target).copied()
        } else {
            layout.get(target + 1).copied()
        };
        self.apply(ShellEvent::ReorderDockEntry { entry, before })
    }

    pub(crate) fn next_item_id(&self) -> DockItemId {
        let mut value = self
            .state
            .dock_items()
            .iter()
            .map(|item| item.id().value())
            .max()
            .map_or(1, |value| value + 1);
        while self.separator_visual_id_exists(value) {
            value = value.wrapping_add(1);
        }
        DockItemId::new(value)
    }

    fn window_for_item(&self, id: DockItemId) -> Option<shell_core::WindowId> {
        self.state
            .dock_items()
            .iter()
            .find(|item| item.id() == id)
            .and_then(|item| match item.running() {
                shell_core::RunningState::Running { window, .. } => Some(*window),
                shell_core::RunningState::Stopped => None,
            })
    }

    fn handle_preview_action_at(
        &self,
        point: DipPoint,
        action: PreviewAction,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let Some(item) = self.hit_app(point) else {
            return Ok(Vec::new());
        };
        self.window_for_item(item).map_or(Ok(Vec::new()), |window| {
            self.handle_preview_action(window, action)
        })
    }

    pub(crate) fn dock_item(&self, id: DockItemId) -> Option<&DockItem> {
        self.state.dock_items().iter().find(|item| item.id() == id)
    }

    pub(crate) fn is_dock_item(&self, id: DockItemId) -> bool {
        self.dock_item(id).is_some()
    }

    fn hit_app(&self, point: DipPoint) -> Option<DockItemId> {
        self.hit_visual(point)
            .map(DockItemId::new)
            .filter(|id| self.is_dock_item(*id))
    }

    fn hit_visual(&self, point: DipPoint) -> Option<u64> {
        let cache_missing = self.hit_layout.borrow().is_none();
        if cache_missing {
            self.hit_layout.replace(Some(self.layout()));
        }
        self.hit_layout
            .borrow()
            .as_ref()
            .and_then(|layout| layout.hit_test(point))
    }

    fn set_hovered_state(&mut self, hovered_item: Option<DockItemId>, hover_strength: f32) {
        if self.hovered_item != hovered_item || (self.hover_strength - hover_strength).abs() >= 0.01
        {
            self.hovered_item = hovered_item;
            self.hover_strength = hover_strength;
            self.visual_generation = self.visual_generation.wrapping_add(1);
        }
    }

    fn hover_strength(&self, point: DipPoint) -> f32 {
        let layout = self.config.layout();
        let icon_top = self.surface.y + layout.padding();
        let icon_bottom = icon_top + layout.item_size();
        let icon_center = icon_top + layout.item_size() / 2.0;
        ((icon_bottom - point.y) / (icon_bottom - icon_center).max(1.0)).clamp(0.0, 1.0)
    }

    fn set_pressed_item(&mut self, pressed_item: Option<DockItemId>) {
        if self.pressed_item != pressed_item {
            self.pressed_item = pressed_item;
            self.visual_generation = self.visual_generation.wrapping_add(1);
        }
    }

    fn set_focused_item(&mut self, focused_item: Option<DockItemId>) {
        if self.focused_item != focused_item {
            self.focused_item = focused_item;
            self.visual_generation = self.visual_generation.wrapping_add(1);
        }
    }

    fn focus_delta(&mut self, delta: isize) {
        let items = self
            .state
            .dock_items()
            .iter()
            .map(DockItem::id)
            .collect::<Vec<_>>();
        if items.is_empty() {
            self.set_focused_item(None);
            return;
        }
        let Some(current) = self.focused_item else {
            if delta < 0 {
                self.set_focused_item(items.last().copied());
            } else {
                self.set_focused_item(Some(items[0]));
            }
            return;
        };
        let position = items.iter().position(|item| *item == current).unwrap_or(0);
        let next = position.saturating_add_signed(delta).min(items.len() - 1);
        self.set_focused_item(Some(items[next]));
    }

    fn activate(&mut self, item: DockItemId) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        let transition = reduce(&self.state, ShellEvent::ActivateDockItem(item))?;
        self.state = transition.state;
        self.invalidate_model_caches();
        Ok(transition
            .effects
            .into_iter()
            .map(|effect| self.map_effect(effect, Some(item)))
            .collect())
    }

    fn map_effect(&self, effect: Effect, launch_item: Option<DockItemId>) -> QueuedDockAction {
        match effect {
            Effect::Launch(app) => {
                let target = launch_item
                    .and_then(|item| self.launch_targets.get(&item).cloned())
                    .unwrap_or_else(|| app.as_str().to_owned());
                QueuedDockAction::Launch(target)
            }
            effect => QueuedDockAction::Effect(effect),
        }
    }
}

fn move_preview_entry(
    layout: &mut Vec<DockLayoutEntry>,
    entry: DockLayoutEntry,
    before: Option<DockLayoutEntry>,
) {
    let Some(source) = layout.iter().position(|current| *current == entry) else {
        return;
    };
    let entry = layout.remove(source);
    let target = before
        .and_then(|target| layout.iter().position(|current| *current == target))
        .unwrap_or(layout.len());
    layout.insert(target, entry);
}
