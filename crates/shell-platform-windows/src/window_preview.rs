#![deny(unsafe_code)]

use shell_core::{AppId, WindowId};
pub use shell_renderer::PreviewUnavailableReason;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewCapture {
    DwmThumbnail,
    Restricted(PreviewUnavailableReason),
}

impl PreviewCapture {
    #[must_use]
    pub const fn dwm_thumbnail() -> Self {
        Self::DwmThumbnail
    }

    #[must_use]
    pub const fn restricted(reason: PreviewUnavailableReason) -> Self {
        Self::Restricted(reason)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewAction {
    Focus,
    Close,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewQueuedAction {
    Focus { window: WindowId, app: AppId },
    Close { window: WindowId, app: AppId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewWindowState {
    window: WindowId,
    app: AppId,
    aliases: Vec<AppId>,
    title: String,
    capture: PreviewCapture,
    foreground: bool,
    minimized: bool,
}

impl PreviewWindowState {
    #[must_use]
    pub fn from_observed(window: &crate::ObservedWindow, capture: PreviewCapture) -> Self {
        Self {
            window: window.window(),
            app: window.app().clone(),
            aliases: window.aliases().to_vec(),
            title: window.title().to_owned(),
            capture,
            foreground: window.foreground(),
            minimized: window.minimized(),
        }
    }

    #[must_use]
    pub const fn window(&self) -> WindowId {
        self.window
    }

    #[must_use]
    pub const fn app(&self) -> &AppId {
        &self.app
    }

    #[must_use]
    pub fn matches_app(&self, app: &AppId) -> bool {
        crate::dock_window_sync::app_ids_match(&self.app, app)
            || self
                .aliases
                .iter()
                .any(|alias| crate::dock_window_sync::app_ids_match(alias, app))
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub const fn capture(&self) -> PreviewCapture {
        self.capture
    }

    #[must_use]
    pub const fn foreground(&self) -> bool {
        self.foreground
    }

    #[must_use]
    pub const fn minimized(&self) -> bool {
        self.minimized
    }
}
