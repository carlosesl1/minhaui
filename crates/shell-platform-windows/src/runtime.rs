#![deny(unsafe_code)]

use shell_renderer::PhysicalRect;

use crate::PlatformEvent;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeAction {
    None,
    Rebuild,
    RepositionAndRebuild,
    ResizeAndRebuild(PhysicalRect),
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeOrchestrator {
    generation: u32,
}

impl Default for RuntimeOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeOrchestrator {
    #[must_use]
    pub const fn new() -> Self {
        Self { generation: 1 }
    }

    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }

    pub fn handle(&mut self, event: PlatformEvent) -> RuntimeAction {
        let action = match event {
            PlatformEvent::DeviceLost | PlatformEvent::PowerResumed => RuntimeAction::Rebuild,
            PlatformEvent::DisplayChanged | PlatformEvent::TaskbarCreated => {
                RuntimeAction::RepositionAndRebuild
            }
            PlatformEvent::DpiChanged(rect) => RuntimeAction::ResizeAndRebuild(rect),
            PlatformEvent::DockPointer(_) | PlatformEvent::DockContextMenu { .. } => {
                RuntimeAction::None
            }
            PlatformEvent::DockDrop { .. } => RuntimeAction::None,
            PlatformEvent::QaExitRequested | PlatformEvent::CloseRequested => RuntimeAction::Quit,
            PlatformEvent::Destroyed => RuntimeAction::None,
        };
        if matches!(
            action,
            RuntimeAction::Rebuild
                | RuntimeAction::RepositionAndRebuild
                | RuntimeAction::ResizeAndRebuild(_)
        ) {
            self.generation += 1;
        }
        action
    }
}
