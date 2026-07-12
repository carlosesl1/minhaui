#![deny(unsafe_code)]

use shell_core::{AppId, DockItemId, WindowId};

use crate::{FullscreenObservation, PreviewCapture};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedWindow {
    window: WindowId,
    app: AppId,
    foreground: bool,
    minimized: bool,
    preview: Option<PreviewCapture>,
    fullscreen: Option<FullscreenObservation>,
}

impl ObservedWindow {
    #[must_use]
    pub const fn new(window: WindowId, app: AppId, foreground: bool, minimized: bool) -> Self {
        Self {
            window,
            app,
            foreground,
            minimized,
            preview: None,
            fullscreen: None,
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
    pub const fn foreground(&self) -> bool {
        self.foreground
    }

    #[must_use]
    pub const fn minimized(&self) -> bool {
        self.minimized
    }

    #[must_use]
    pub const fn preview(&self) -> Option<PreviewCapture> {
        self.preview
    }

    #[must_use]
    pub const fn with_preview(mut self, preview: PreviewCapture) -> Self {
        self.preview = Some(preview);
        self
    }

    #[must_use]
    pub const fn fullscreen(&self) -> Option<FullscreenObservation> {
        self.fullscreen
    }

    #[must_use]
    pub const fn with_fullscreen(mut self, fullscreen: FullscreenObservation) -> Self {
        self.fullscreen = Some(fullscreen);
        self
    }
}

pub(crate) fn stable_item_id(app: &AppId) -> DockItemId {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in app.as_str().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    DockItemId::new(hash | 0x8000_0000_0000_0000)
}
