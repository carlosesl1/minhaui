#![deny(unsafe_code)]

use shell_renderer::{
    ContextMenuEntry, ContextMenuScene, DipPoint, DipRect, layout_context_menu_scene,
};

use crate::{ContextMenuCommand, PopoverKey};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DockContextMenuItem {
    Action {
        label: String,
        command: ContextMenuCommand,
        enabled: bool,
    },
    Separator,
}

impl DockContextMenuItem {
    #[must_use]
    pub fn action(label: &str, command: ContextMenuCommand, enabled: bool) -> Self {
        Self::Action {
            label: label.to_owned(),
            command,
            enabled,
        }
    }

    #[must_use]
    pub const fn separator() -> Self {
        Self::Separator
    }

    #[must_use]
    const fn enabled(&self) -> bool {
        matches!(self, Self::Action { enabled: true, .. })
    }

    #[must_use]
    pub const fn command(&self) -> Option<ContextMenuCommand> {
        match self {
            Self::Action { command, .. } => Some(*command),
            Self::Separator => None,
        }
    }

    fn render_entry(&self) -> ContextMenuEntry {
        match self {
            Self::Action { label, enabled, .. } => ContextMenuEntry::action(label, *enabled),
            Self::Separator => ContextMenuEntry::separator(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QueuedContextMenuAction {
    Redraw,
    Dismiss,
    Execute {
        point: DipPoint,
        command: ContextMenuCommand,
    },
}

pub struct DockContextMenuController {
    active: Option<ActiveContextMenu>,
}

impl DockContextMenuController {
    #[must_use]
    pub const fn new() -> Self {
        Self { active: None }
    }

    pub fn open(&mut self, point: DipPoint, items: Vec<DockContextMenuItem>) {
        self.active = Some(ActiveContextMenu {
            point,
            items,
            focused: None,
        });
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active.is_some()
    }

    #[must_use]
    pub fn target_point(&self) -> Option<DipPoint> {
        self.active.as_ref().map(|active| active.point)
    }

    pub fn dismiss(&mut self) -> Vec<QueuedContextMenuAction> {
        if self.active.take().is_some() {
            vec![QueuedContextMenuAction::Dismiss]
        } else {
            Vec::new()
        }
    }

    pub fn handle_key(&mut self, key: PopoverKey) -> Vec<QueuedContextMenuAction> {
        if key == PopoverKey::Escape {
            return self.dismiss();
        }
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        let actions = match key {
            PopoverKey::Next => active.focus_delta(1),
            PopoverKey::Previous => active.focus_delta(-1),
            PopoverKey::Activate => active.activate(),
            PopoverKey::Escape => Vec::new(),
        };
        if actions
            .iter()
            .any(|action| matches!(action, QueuedContextMenuAction::Execute { .. }))
        {
            self.active = None;
        }
        actions
    }

    pub fn handle_pointer_move(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedContextMenuAction> {
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        let hit = layout_context_menu_scene(&active.scene(), surface)
            .hit_test(point)
            .filter(|index| active.items[*index].enabled());
        if active.focused == hit {
            Vec::new()
        } else {
            active.focused = hit;
            vec![QueuedContextMenuAction::Redraw]
        }
    }

    pub fn handle_pointer_click(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedContextMenuAction> {
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let command = layout_context_menu_scene(&active.scene(), surface)
            .hit_test(point)
            .filter(|index| active.items[*index].enabled())
            .and_then(|index| active.items[index].command());
        let target = active.point;
        self.active = None;
        match command {
            Some(command) => vec![QueuedContextMenuAction::Execute {
                point: target,
                command,
            }],
            None => vec![QueuedContextMenuAction::Dismiss],
        }
    }

    #[must_use]
    pub fn scene(&self) -> Option<ContextMenuScene> {
        self.active.as_ref().map(ActiveContextMenu::scene)
    }
}

impl Default for DockContextMenuController {
    fn default() -> Self {
        Self::new()
    }
}

struct ActiveContextMenu {
    point: DipPoint,
    items: Vec<DockContextMenuItem>,
    focused: Option<usize>,
}

impl ActiveContextMenu {
    fn scene(&self) -> ContextMenuScene {
        ContextMenuScene::new(
            self.items
                .iter()
                .map(DockContextMenuItem::render_entry)
                .collect(),
            self.focused,
        )
    }

    fn focus_delta(&mut self, delta: isize) -> Vec<QueuedContextMenuAction> {
        let enabled = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| item.enabled().then_some(index))
            .collect::<Vec<_>>();
        if enabled.is_empty() {
            return Vec::new();
        }
        let next = match self.focused {
            None if delta < 0 => *enabled.last().unwrap_or(&enabled[0]),
            None => enabled[0],
            Some(current) => {
                let position = enabled
                    .iter()
                    .position(|index| *index == current)
                    .unwrap_or(0);
                let next = position.saturating_add_signed(delta).min(enabled.len() - 1);
                enabled[next]
            }
        };
        self.focused = Some(next);
        vec![QueuedContextMenuAction::Redraw]
    }

    fn activate(&self) -> Vec<QueuedContextMenuAction> {
        self.focused
            .and_then(|index| self.items.get(index))
            .filter(|item| item.enabled())
            .and_then(DockContextMenuItem::command)
            .map_or_else(Vec::new, |command| {
                vec![QueuedContextMenuAction::Execute {
                    point: self.point,
                    command,
                }]
            })
    }
}
