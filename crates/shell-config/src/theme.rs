use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Maximum accepted inert docktheme JSON envelope size.
pub const MAX_THEME_BYTES: usize = 256 * 1024;
const THEME_VERSION: u16 = 1;

/// Fixed original visual token set permitted in a portable theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeTokens {
    /// Base glass surface token.
    pub surface_base: String,
    /// Raised glass surface token.
    pub surface_raised: String,
    /// Primary text token.
    pub text_primary: String,
    /// Action/focus accent token.
    pub accent_default: String,
    /// Dock corner radius in logical pixels.
    pub dock_radius: u16,
}

impl Default for ThemeTokens {
    fn default() -> Self {
        Self {
            surface_base: "#11151BEF".to_owned(),
            surface_raised: "#181D24F2".to_owned(),
            text_primary: "#F5F7FA".to_owned(),
            accent_default: "#4C9AFF".to_owned(),
            dock_radius: 18,
        }
    }
}

/// Validated inert payload shared by a `.docktheme` JSON document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemePayload {
    name: String,
    tokens: ThemeTokens,
    asset_references: Vec<String>,
}

impl Default for ThemePayload {
    fn default() -> Self {
        Self {
            name: "Original".to_owned(),
            tokens: ThemeTokens::default(),
            asset_references: Vec::new(),
        }
    }
}

impl ThemePayload {
    /// Returns a copy with a local inert asset reference for boundary validation.
    #[must_use]
    pub fn with_asset(mut self, reference: &str) -> Self {
        self.asset_references.push(reference.to_owned());
        self
    }

    /// Returns a copy with a bounded original dock radius token.
    #[must_use]
    pub fn with_dock_radius(mut self, radius: u16) -> Self {
        self.tokens.dock_radius = radius;
        self
    }

    /// Validates payload bounds, fixed token values, and inert asset references.
    pub fn validate(&self) -> Result<(), ThemeError> {
        if self.name.is_empty() || self.name.len() > 64 || self.asset_references.len() > 32 {
            return Err(ThemeError::InvalidTokens);
        }
        if !(6..=24).contains(&self.tokens.dock_radius)
            || ![
                self.tokens.surface_base.as_str(),
                self.tokens.surface_raised.as_str(),
                self.tokens.text_primary.as_str(),
                self.tokens.accent_default.as_str(),
            ]
            .into_iter()
            .all(valid_color)
        {
            return Err(ThemeError::InvalidTokens);
        }
        if self
            .asset_references
            .iter()
            .any(|reference| !valid_asset_reference(reference))
        {
            return Err(ThemeError::ForbiddenAssetReference);
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeEnvelope {
    schema_version: u16,
    payload: ThemePayload,
    checksum_sha256: String,
}

/// Exports a deterministic checksum-protected inert JSON envelope.
pub fn export_theme(theme: &ThemePayload) -> Result<Vec<u8>, ThemeError> {
    theme.validate()?;
    let payload = serde_json::to_vec(theme).map_err(ThemeError::Json)?;
    let envelope = ThemeEnvelope {
        schema_version: THEME_VERSION,
        payload: theme.clone(),
        checksum_sha256: checksum(&payload),
    };
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(ThemeError::Json)?;
    if bytes.len() > MAX_THEME_BYTES {
        Err(ThemeError::Oversized)
    } else {
        Ok(bytes)
    }
}

/// Imports and validates a bounded `.docktheme` JSON envelope.
pub fn import_theme(bytes: &[u8]) -> Result<ThemePayload, ThemeError> {
    if bytes.len() > MAX_THEME_BYTES {
        return Err(ThemeError::Oversized);
    }
    let envelope: ThemeEnvelope = serde_json::from_slice(bytes).map_err(ThemeError::Json)?;
    if envelope.schema_version != THEME_VERSION {
        return Err(ThemeError::UnsupportedVersion(envelope.schema_version));
    }
    envelope.payload.validate()?;
    let payload = serde_json::to_vec(&envelope.payload).map_err(ThemeError::Json)?;
    if checksum(&payload) != envelope.checksum_sha256 {
        return Err(ThemeError::ChecksumMismatch);
    }
    Ok(envelope.payload)
}

fn checksum(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_color(value: &str) -> bool {
    matches!(value.len(), 7 | 9)
        && value.starts_with('#')
        && value.bytes().skip(1).all(|byte| byte.is_ascii_hexdigit())
}

fn valid_asset_reference(reference: &str) -> bool {
    !reference.is_empty()
        && reference.len() <= 128
        && !reference.contains("://")
        && !reference.contains(':')
        && !reference.starts_with(['/', '\\'])
        && !reference
            .split(['/', '\\'])
            .any(|segment| segment == ".." || segment.is_empty())
}

/// Describes an invalid or tampered portable theme boundary.
#[derive(Debug, Error)]
pub enum ThemeError {
    /// The JSON envelope exceeded its byte bound.
    #[error("theme exceeds maximum size")]
    Oversized,
    /// Envelope schema was not supported.
    #[error("unsupported theme schema version {0}")]
    UnsupportedVersion(u16),
    /// Payload bytes did not match the advertised checksum.
    #[error("theme checksum mismatch")]
    ChecksumMismatch,
    /// Fixed tokens or their values were invalid.
    #[error("theme contains invalid original tokens")]
    InvalidTokens,
    /// Asset reference was absolute, external, or traversal-like.
    #[error("theme contains a forbidden asset reference")]
    ForbiddenAssetReference,
    /// Envelope JSON was malformed or structurally invalid.
    #[error("theme JSON error: {0}")]
    Json(serde_json::Error),
}

impl PartialEq for ThemeError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Oversized, Self::Oversized)
            | (Self::ChecksumMismatch, Self::ChecksumMismatch)
            | (Self::InvalidTokens, Self::InvalidTokens)
            | (Self::ForbiddenAssetReference, Self::ForbiddenAssetReference) => true,
            (Self::UnsupportedVersion(left), Self::UnsupportedVersion(right)) => left == right,
            (Self::Json(left), Self::Json(right)) => left.classify() == right.classify(),
            _ => false,
        }
    }
}
