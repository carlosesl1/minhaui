use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

macro_rules! numeric_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            /// Creates an identifier from its platform-stable numeric value.
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            /// Returns the underlying stable numeric value.
            #[must_use]
            pub const fn value(self) -> u64 {
                self.0
            }
        }
    };
}

numeric_id!(MonitorId, "A stable identifier for one display monitor.");
numeric_id!(DockItemId, "A stable identifier for one dock entry.");
numeric_id!(WindowId, "A stable identifier for a platform window.");

/// A validated application identity used for launch intents.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct AppId(String);

impl AppId {
    /// Parses a bounded application identifier.
    pub fn parse(value: &str) -> Result<Self, AppIdError> {
        let valid_length = !value.is_empty() && value.len() <= 128;
        let valid_characters = value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
        if valid_length && valid_characters {
            Ok(Self(value.to_owned()))
        } else {
            Err(AppIdError::Invalid)
        }
    }

    /// Borrows the application identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for AppId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Describes why an application identifier was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AppIdError {
    /// The identifier was empty, too long, or contained unsupported characters.
    #[error("application identifier must be 1..=128 ASCII letters, digits, '.', '-' or '_'")]
    Invalid,
}
