#![deny(unsafe_code)]

use shell_core::{Effect, ShellEvent};

use crate::{DockController, DockControllerError, QueuedDockAction};

impl DockController {
    pub fn reveal_from_physical_edge(
        &mut self,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        if self.autohide_active() && !self.state.dock().is_revealed() {
            self.apply(ShellEvent::RevealDock)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn set_fullscreen_autohide(
        &mut self,
        active: bool,
    ) -> Result<Vec<QueuedDockAction>, DockControllerError> {
        if self.fullscreen_autohide == active {
            return Ok(Vec::new());
        }
        self.fullscreen_autohide = active;
        let mut actions = self.apply(ShellEvent::EnableAutohide(self.autohide_active()))?;
        actions.retain(|action| {
            !matches!(
                action,
                QueuedDockAction::Effect(Effect::PersistConfiguration)
            )
        });
        if active {
            actions.extend(self.apply(ShellEvent::HideDock)?);
        }
        Ok(actions)
    }

    pub(crate) const fn autohide_active(&self) -> bool {
        self.config.autohide() || self.fullscreen_autohide
    }
}
