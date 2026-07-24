#![deny(unsafe_code)]

use shell_renderer::{
    ContextMenuEntry, ContextMenuScene, DipPoint, DipRect, Dpi, PhysicalRect,
    context_menu_surface_width, layout_context_menu_scene, physical_from_dip,
};

use crate::{BackgroundAppId, PopoverKey};

const SHELL_OWNED_HEADER: &str = "Minha UI menu (shell-owned)";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackgroundAppMenuCommand {
    OpenOrFocus(BackgroundAppId),
    OpenFileLocation(BackgroundAppId),
    TryAlternateActivation(BackgroundAppId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QueuedBackgroundAppMenuAction {
    Redraw,
    Dismiss,
    Execute(BackgroundAppMenuCommand),
}

pub(crate) struct BackgroundAppMenuController {
    active: Option<ActiveBackgroundAppMenu>,
}

impl BackgroundAppMenuController {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self { active: None }
    }

    pub(crate) fn open(&mut self, app: BackgroundAppId, allow_alternate: bool) {
        let mut items = vec![
            BackgroundAppMenuItem::label(SHELL_OWNED_HEADER),
            BackgroundAppMenuItem::Separator,
            BackgroundAppMenuItem::action(
                "Open or focus",
                BackgroundAppMenuCommand::OpenOrFocus(app),
            ),
            BackgroundAppMenuItem::action(
                "Open file location",
                BackgroundAppMenuCommand::OpenFileLocation(app),
            ),
        ];
        if allow_alternate {
            items.push(BackgroundAppMenuItem::action(
                "Try alternate activation",
                BackgroundAppMenuCommand::TryAlternateActivation(app),
            ));
        }
        self.active = Some(ActiveBackgroundAppMenu {
            app,
            items,
            focused: None,
        });
    }

    #[must_use]
    pub(crate) const fn is_active(&self) -> bool {
        self.active.is_some()
    }

    #[must_use]
    pub(crate) fn target_app(&self) -> Option<BackgroundAppId> {
        self.active.as_ref().map(|active| active.app)
    }

    pub(crate) fn dismiss(&mut self) -> Vec<QueuedBackgroundAppMenuAction> {
        if self.active.take().is_some() {
            vec![QueuedBackgroundAppMenuAction::Dismiss]
        } else {
            Vec::new()
        }
    }

    pub(crate) fn handle_key(&mut self, key: PopoverKey) -> Vec<QueuedBackgroundAppMenuAction> {
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
            PopoverKey::ContextMenu | PopoverKey::Escape => Vec::new(),
        };
        if actions
            .iter()
            .any(|action| matches!(action, QueuedBackgroundAppMenuAction::Execute(_)))
        {
            self.active = None;
        }
        actions
    }

    pub(crate) fn handle_pointer_move(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedBackgroundAppMenuAction> {
        let Some(active) = &mut self.active else {
            return Vec::new();
        };
        let hit = layout_context_menu_scene(&active.scene(), surface)
            .hit_test(point)
            .filter(|index| {
                active
                    .items
                    .get(*index)
                    .is_some_and(BackgroundAppMenuItem::enabled)
            });
        if active.focused == hit {
            Vec::new()
        } else {
            active.focused = hit;
            vec![QueuedBackgroundAppMenuAction::Redraw]
        }
    }

    pub(crate) fn handle_pointer_release(
        &mut self,
        point: DipPoint,
        surface: DipRect,
    ) -> Vec<QueuedBackgroundAppMenuAction> {
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let command = layout_context_menu_scene(&active.scene(), surface)
            .hit_test(point)
            .and_then(|index| active.items.get(index))
            .filter(|item| item.enabled())
            .and_then(BackgroundAppMenuItem::command);
        self.active = None;
        command.map_or_else(
            || vec![QueuedBackgroundAppMenuAction::Dismiss],
            |command| vec![QueuedBackgroundAppMenuAction::Execute(command)],
        )
    }

    #[must_use]
    pub(crate) fn scene(&self) -> Option<ContextMenuScene> {
        self.active.as_ref().map(ActiveBackgroundAppMenu::scene)
    }
}

impl Default for BackgroundAppMenuController {
    fn default() -> Self {
        Self::new()
    }
}

struct ActiveBackgroundAppMenu {
    app: BackgroundAppId,
    items: Vec<BackgroundAppMenuItem>,
    focused: Option<usize>,
}

impl ActiveBackgroundAppMenu {
    fn scene(&self) -> ContextMenuScene {
        ContextMenuScene::new(
            self.items
                .iter()
                .map(BackgroundAppMenuItem::render_entry)
                .collect(),
            self.focused,
        )
    }

    fn focus_delta(&mut self, delta: isize) -> Vec<QueuedBackgroundAppMenuAction> {
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
                enabled[position.saturating_add_signed(delta).min(enabled.len() - 1)]
            }
        };
        if self.focused == Some(next) {
            return Vec::new();
        }
        self.focused = Some(next);
        vec![QueuedBackgroundAppMenuAction::Redraw]
    }

    fn activate(&self) -> Vec<QueuedBackgroundAppMenuAction> {
        self.focused
            .and_then(|index| self.items.get(index))
            .filter(|item| item.enabled())
            .and_then(BackgroundAppMenuItem::command)
            .map_or_else(Vec::new, |command| {
                vec![QueuedBackgroundAppMenuAction::Execute(command)]
            })
    }
}

enum BackgroundAppMenuItem {
    Label(String),
    Action {
        label: String,
        command: BackgroundAppMenuCommand,
    },
    Separator,
}

impl BackgroundAppMenuItem {
    fn label(label: &str) -> Self {
        Self::Label(label.to_owned())
    }

    fn action(label: &str, command: BackgroundAppMenuCommand) -> Self {
        Self::Action {
            label: label.to_owned(),
            command,
        }
    }

    const fn enabled(&self) -> bool {
        matches!(self, Self::Action { .. })
    }

    const fn command(&self) -> Option<BackgroundAppMenuCommand> {
        match self {
            Self::Action { command, .. } => Some(*command),
            Self::Label(_) | Self::Separator => None,
        }
    }

    fn render_entry(&self) -> ContextMenuEntry {
        match self {
            Self::Label(label) => ContextMenuEntry::action(label, false),
            Self::Action { label, .. } => ContextMenuEntry::action(label, true),
            Self::Separator => ContextMenuEntry::separator(),
        }
    }
}

#[must_use]
pub(crate) fn background_app_menu_rect(
    anchor: PhysicalRect,
    work: PhysicalRect,
    dpi: Dpi,
    height_dip: f32,
) -> PhysicalRect {
    let width = physical_from_dip(context_menu_surface_width(), dpi).min(work.width.max(1));
    let height = physical_from_dip(height_dip.max(1.0), dpi).min(work.height.max(1));
    let work_right = work.x.saturating_add(work.width);
    let work_bottom = work.y.saturating_add(work.height);
    let max_x = work_right.saturating_sub(width);
    let max_y = work_bottom.saturating_sub(height);
    let right = anchor.x.saturating_add(anchor.width);
    let left = anchor.x.saturating_sub(width);
    let x = if right.saturating_add(width) <= work_right {
        right
    } else if left >= work.x {
        left
    } else {
        right.clamp(work.x, max_x)
    };
    PhysicalRect::new(x, anchor.y.clamp(work.y, max_y), width, height)
}
