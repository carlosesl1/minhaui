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
