#![deny(unsafe_code)]

use shell_core::{AppId, DockItemId, WindowId};

use crate::{FullscreenObservation, PreviewCapture};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedWindow {
    window: WindowId,
    app: AppId,
    aliases: Vec<AppId>,
    title: String,
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
            aliases: Vec::new(),
            title: String::new(),
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
    pub fn with_aliases(mut self, aliases: Vec<AppId>) -> Self {
        self.aliases = aliases;
        self
    }

    #[must_use]
    pub fn aliases(&self) -> &[AppId] {
        &self.aliases
    }

    #[must_use]
    pub fn matches_app(&self, app: &AppId) -> bool {
        app_ids_match(&self.app, app) || self.aliases.iter().any(|alias| app_ids_match(alias, app))
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
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

pub(crate) fn canonical_app_id(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "app.calculator"
        | "calculator"
        | "calculatorapp.exe"
        | "microsoft.windowscalculator_8wekyb3d8bbwe!app" => "calc.exe".to_owned(),
        value => value.to_owned(),
    }
}

pub(crate) fn app_ids_match(left: &AppId, right: &AppId) -> bool {
    canonical_app_id(left.as_str()) == canonical_app_id(right.as_str())
}
