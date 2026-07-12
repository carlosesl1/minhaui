use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use shell_core::{
    Accessibility, DockItem, PerformancePreset, TaskbarPolicy, TopbarModule, TopbarModuleKind,
};
use thiserror::Error;

use crate::MAX_CONFIG_BYTES;

const SCHEMA_VERSION: u16 = 1;

/// Current validated persisted configuration schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellConfigV1 {
    schema_version: u16,
    autohide: bool,
    taskbar_policy: TaskbarPolicy,
    accessibility: Accessibility,
    performance: PerformancePreset,
    dock_items: Vec<DockItem>,
    topbar_modules: Vec<TopbarModule>,
}

impl Default for ShellConfigV1 {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            autohide: false,
            taskbar_policy: TaskbarPolicy::Off,
            accessibility: Accessibility::new(false, false, true),
            performance: PerformancePreset::Balanced,
            dock_items: Vec::new(),
            topbar_modules: default_topbar(),
        }
    }
}

impl ShellConfigV1 {
    /// Validates schema identity, bounds, and ordered collection uniqueness.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedVersion(self.schema_version));
        }
        if self.dock_items.len() > 256 || self.topbar_modules.len() > 32 {
            return Err(ConfigError::CollectionBound);
        }
        if !all_unique(self.dock_items.iter().map(DockItem::id)) {
            return Err(ConfigError::DuplicateDockItem);
        }
        if !all_unique(self.topbar_modules.iter().map(|module| module.kind())) {
            return Err(ConfigError::DuplicateTopbarModule);
        }
        Ok(())
    }

    /// Returns a copy with the autohide preference changed.
    #[must_use]
    pub fn with_autohide(&self, enabled: bool) -> Self {
        let mut config = self.clone();
        config.autohide = enabled;
        config
    }

    /// Returns the autohide preference.
    #[must_use]
    pub const fn autohide(&self) -> bool {
        self.autohide
    }
    /// Returns the taskbar policy.
    #[must_use]
    pub const fn taskbar_policy(&self) -> TaskbarPolicy {
        self.taskbar_policy
    }
    /// Returns accessibility preferences.
    #[must_use]
    pub const fn accessibility(&self) -> Accessibility {
        self.accessibility
    }
    /// Returns the performance preset.
    #[must_use]
    pub const fn performance(&self) -> PerformancePreset {
        self.performance
    }
    /// Borrows persisted dock entries in order.
    #[must_use]
    pub fn dock_items(&self) -> &[DockItem] {
        &self.dock_items
    }
    /// Borrows persisted top-bar modules in order.
    #[must_use]
    pub fn topbar_modules(&self) -> &[TopbarModule] {
        &self.topbar_modules
    }
}

/// Describes why parsing fell back to safe configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryKind {
    /// No primary configuration file existed.
    Missing,
    /// JSON was malformed, truncated, or failed validation.
    Malformed,
    /// The input advertised a newer schema.
    FutureVersion,
    /// The input exceeded the public byte bound.
    Oversized,
    /// A preserved backup replaced an invalid primary document.
    Backup,
}

/// Structured recovery details suitable for diagnostics/UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryReport {
    kind: RecoveryKind,
    detected_version: Option<u16>,
}

impl RecoveryReport {
    pub(crate) const fn new(kind: RecoveryKind, detected_version: Option<u16>) -> Self {
        Self {
            kind,
            detected_version,
        }
    }

    /// Returns the recovery class.
    #[must_use]
    pub const fn kind(self) -> RecoveryKind {
        self.kind
    }
    /// Returns an advertised schema version when one was available.
    #[must_use]
    pub const fn detected_version(self) -> Option<u16> {
        self.detected_version
    }
}

/// Outcome of decoding bytes at the configuration trust boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigLoad {
    /// Valid current schema.
    Current(ShellConfigV1),
    /// Successfully migrated legacy schema.
    Migrated { config: ShellConfigV1, from: u16 },
    /// Safe defaults used after explicit recovery.
    Recovered {
        config: ShellConfigV1,
        report: RecoveryReport,
    },
}

impl ConfigLoad {
    /// Borrows the usable current configuration for every outcome.
    #[must_use]
    pub const fn config(&self) -> &ShellConfigV1 {
        match self {
            Self::Current(config)
            | Self::Migrated { config, .. }
            | Self::Recovered { config, .. } => config,
        }
    }
}

/// Encodes a validated current configuration.
pub fn encode_config(config: &ShellConfigV1) -> Result<Vec<u8>, ConfigError> {
    config.validate()?;
    serde_json::to_vec_pretty(config).map_err(ConfigError::Json)
}

/// Decodes bounded configuration bytes with migration and explicit recovery.
#[must_use]
pub fn decode_config(bytes: &[u8]) -> ConfigLoad {
    if bytes.len() > MAX_CONFIG_BYTES {
        return recovered(RecoveryKind::Oversized, None);
    }
    let value = match serde_json::from_slice::<Value>(bytes) {
        Ok(value) => value,
        Err(_) => return recovered(RecoveryKind::Malformed, None),
    };
    if let Some(version) = value.get("schema_version").and_then(Value::as_u64) {
        let Ok(version) = u16::try_from(version) else {
            return recovered(RecoveryKind::FutureVersion, None);
        };
        if version > SCHEMA_VERSION {
            return recovered(RecoveryKind::FutureVersion, Some(version));
        }
        if version == SCHEMA_VERSION {
            return match serde_json::from_value::<ShellConfigV1>(value) {
                Ok(config) if config.validate().is_ok() => ConfigLoad::Current(config),
                Ok(_) | Err(_) => recovered(RecoveryKind::Malformed, Some(version)),
            };
        }
        return recovered(RecoveryKind::Malformed, Some(version));
    }
    if value.get("version").and_then(Value::as_u64) == Some(0) {
        return match serde_json::from_value::<ConfigV0>(value) {
            Ok(old) => ConfigLoad::Migrated {
                config: old.migrate(),
                from: 0,
            },
            Err(_) => recovered(RecoveryKind::Malformed, Some(0)),
        };
    }
    recovered(RecoveryKind::Malformed, None)
}

fn recovered(kind: RecoveryKind, version: Option<u16>) -> ConfigLoad {
    ConfigLoad::Recovered {
        config: ShellConfigV1::default(),
        report: RecoveryReport::new(kind, version),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigV0 {
    version: u16,
    autohide: bool,
    taskbar: TaskbarPolicy,
    reduced_motion: bool,
    performance: PerformancePreset,
}

impl ConfigV0 {
    fn migrate(self) -> ShellConfigV1 {
        let _legacy_version = self.version;
        ShellConfigV1 {
            schema_version: SCHEMA_VERSION,
            autohide: self.autohide,
            taskbar_policy: self.taskbar,
            accessibility: Accessibility::new(self.reduced_motion, false, true),
            performance: self.performance,
            dock_items: Vec::new(),
            topbar_modules: default_topbar(),
        }
    }
}

fn default_topbar() -> Vec<TopbarModule> {
    [
        TopbarModuleKind::SystemMenu,
        TopbarModuleKind::Clock,
        TopbarModuleKind::Network,
        TopbarModuleKind::Volume,
        TopbarModuleKind::Power,
        TopbarModuleKind::Notifications,
    ]
    .into_iter()
    .map(|kind| TopbarModule::new(kind, true))
    .collect()
}

fn all_unique<T>(values: impl Iterator<Item = T>) -> bool
where
    T: Eq + std::hash::Hash,
{
    let mut seen = HashSet::new();
    values.into_iter().all(|value| seen.insert(value))
}

/// Describes invalid current configuration or serialization failure.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The schema version is unsupported.
    #[error("unsupported configuration schema version {0}")]
    UnsupportedVersion(u16),
    /// A persisted collection exceeded its bound.
    #[error("configuration collection exceeds its bound")]
    CollectionBound,
    /// Dock entry identities were not unique.
    #[error("dock item identities must be unique")]
    DuplicateDockItem,
    /// Top-bar module kinds were not unique.
    #[error("topbar module kinds must be unique")]
    DuplicateTopbarModule,
    /// JSON encoding failed.
    #[error("configuration JSON error: {0}")]
    Json(serde_json::Error),
}
