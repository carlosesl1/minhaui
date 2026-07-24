#![forbid(unsafe_code)]

mod persistence;
mod quick_settings;
mod recovery;
mod schema;
mod settings;
mod theme;

pub use persistence::{
    AtomicPaths, AtomicWriter, ConfigSource, ConfigStore, PersistenceError, StoreLoad,
};
pub use quick_settings::QuickSettingsSettings;
pub use schema::{
    ConfigError, ConfigLoad, RecoveryKind, RecoveryReport, ShellConfigV1, decode_config,
    encode_config,
};
pub use settings::{
    AdvancedSettings, AppearanceSettings, BehaviorSettings, DockAlignmentPreference, DockSettings,
    TopbarDensityPreference, TopbarSettings,
};
pub use shell_core::{Accessibility, PerformancePreset, QuickControlKind, QuickControlPlacement};
pub use theme::{
    MAX_THEME_BYTES, ThemeError, ThemePayload, ThemeTokens, export_theme, import_theme,
};

/// Maximum accepted configuration document size.
pub const MAX_CONFIG_BYTES: usize = 256 * 1024;

/// Returns the stable crate identity used by workspace smoke tests.
#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-config"
}
