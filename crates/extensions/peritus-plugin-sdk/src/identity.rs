//! Validated plugin, request, version, and digest identities.

use std::fmt;

use serde::Deserialize;
use serde::ser::SerializeStruct;

use crate::{SdkError, SdkErrorKind};

/// Canonical lowercase plugin identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PluginId(String);

impl PluginId {
    /// Creates a dot-separated identifier such as `vendor.plugin-name`.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, uppercase, empty-segment, or non-ASCII identifiers.
    pub fn new(value: impl Into<String>) -> Result<Self, SdkError> {
        let value = value.into();
        validate_name(&value, 128, "plugin id")?;
        Ok(Self(value))
    }

    /// Borrows the canonical identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl serde::Serialize for PluginId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PluginId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Strict three-part semantic version used by plugin artifacts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(deny_unknown_fields)]
pub struct PluginVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

impl serde::Serialize for PluginVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("PluginVersion", 3)?;
        state.serialize_field("major", &self.major)?;
        state.serialize_field("minor", &self.minor)?;
        state.serialize_field("patch", &self.patch)?;
        state.end()
    }
}

impl PluginVersion {
    /// Creates a semantic version.
    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self { major, minor, patch }
    }

    /// Returns the major version.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the minor version.
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }

    /// Returns the patch version.
    #[must_use]
    pub const fn patch(self) -> u16 {
        self.patch
    }
}

impl fmt::Display for PluginVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Caller-generated request identifier unique within one plugin lifecycle.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(String);

impl RequestId {
    /// Creates a bounded opaque request identifier.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or control-containing values.
    pub fn new(value: impl Into<String>) -> Result<Self, SdkError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
            return Err(SdkError::new(
                SdkErrorKind::InvalidIdentity,
                "validate request id",
                "request id must contain 1 to 128 non-control UTF-8 bytes",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the opaque request identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl serde::Serialize for RequestId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RequestId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// SHA-256 of canonical manifest trust material.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManifestDigest([u8; 32]);

impl ManifestDigest {
    /// Creates a digest from exact bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrows the digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns lowercase hexadecimal encoding.
    #[must_use]
    pub fn to_hex(self) -> String {
        let mut encoded = String::with_capacity(64);
        for byte in self.0 {
            use std::fmt::Write as _;
            let _ = write!(encoded, "{byte:02x}");
        }
        encoded
    }
}

fn validate_name(value: &str, maximum: usize, label: &'static str) -> Result<(), SdkError> {
    let valid = !value.is_empty()
        && value.len() <= maximum
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
        && value
            .split('.')
            .all(|part| !part.is_empty() && !part.starts_with('-') && !part.ends_with('-'));
    if valid {
        Ok(())
    } else {
        Err(SdkError::new(
            SdkErrorKind::InvalidIdentity,
            "validate canonical identity",
            format!("{label} is not canonical lowercase ASCII"),
        ))
    }
}
