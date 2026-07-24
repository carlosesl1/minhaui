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
pub enum DockRenderAction {
    None,
    RedrawDock,
    #[expect(
        dead_code,
        reason = "kept distinct from full rebuild for native surface recovery"
    )]
    RebuildDockSurface,
    RebuildAllSurfaces,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DockRenderChange {
    pub state_changed: bool,
    pub visual_changed: bool,
    pub dock_visibility_changed: bool,
    pub rebuild_requested: bool,
}

#[must_use]
pub const fn classify_dock_render_action(change: DockRenderChange) -> DockRenderAction {
    if change.rebuild_requested {
        DockRenderAction::RebuildAllSurfaces
    } else if change.visual_changed || (change.state_changed && !change.dock_visibility_changed) {
        DockRenderAction::RedrawDock
    } else {
        DockRenderAction::None
    }
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
            PlatformEvent::AppBarPositionChanged => RuntimeAction::None,
            PlatformEvent::DpiChanged(rect) => RuntimeAction::ResizeAndRebuild(rect),
            PlatformEvent::DockPointer(_)
            | PlatformEvent::DockEdgeProbe
            | PlatformEvent::DockAnimationFrame
            | PlatformEvent::PreviewTimer
            | PlatformEvent::PreviewPointerMoved(_)
            | PlatformEvent::PreviewPointerPressed(_)
            | PlatformEvent::PreviewPointerReleased(_)
            | PlatformEvent::PreviewDismissed
            | PlatformEvent::DockKey(_)
            | PlatformEvent::DockContextMenu { .. }
            | PlatformEvent::DockContextMenuRequested { .. } => RuntimeAction::None,
            PlatformEvent::TopbarPointer(_) | PlatformEvent::TopbarKey(_) => RuntimeAction::None,
            PlatformEvent::PopoverKey(_) => RuntimeAction::None,
            PlatformEvent::PopoverPointer(_)
            | PlatformEvent::PopoverPointerMoved(_)
            | PlatformEvent::PopoverScroll(_) => RuntimeAction::None,
            PlatformEvent::DismissTransientOverlays => RuntimeAction::None,
            PlatformEvent::SettingsKey(_) => RuntimeAction::None,
            PlatformEvent::DockDrop { .. }
            | PlatformEvent::SyncWindows
            | PlatformEvent::ShellObservationLoaded(_)
            | PlatformEvent::BackgroundAppsLoaded(_) => RuntimeAction::None,
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
