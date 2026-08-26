//! Validated textual and path values used by component declarations.

use crate::domain::{HarnessDomainError, HarnessDomainErrorKind};

const MAX_PATH_BYTES: usize = 4_096;
const MAX_PATH_COMPONENT_BYTES: usize = 255;
const MAX_PATH_COMPONENTS: usize = 256;
const MAX_MEDIA_TYPE_BYTES: usize = 255;
const MAX_OWNER_BYTES: usize = 256;
const MAX_PROVENANCE_BYTES: usize = 4_096;

/// Validated component owner identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Owner(String);

impl Owner {
    /// Validates a nonempty bounded UTF-8 owner.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or control-character-containing text.
    pub fn new(value: impl Into<String>) -> Result<Self, HarnessDomainError> {
        let value = value.into();
        if value.is_empty() {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::EmptyValue,
                "component owner is empty",
            ));
        }
        if value.len() > MAX_OWNER_BYTES {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::ValueTooLong,
                "component owner is too long",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::InvalidValue,
                "component owner contains a control character",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the exact validated owner.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated component provenance statement.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Provenance(String);

impl Provenance {
    /// Validates nonempty bounded UTF-8 provenance.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or control-character-containing text.
    pub fn new(value: impl Into<String>) -> Result<Self, HarnessDomainError> {
        let value = value.into();
        if value.is_empty() {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::EmptyValue,
                "component provenance is empty",
            ));
        }
        if value.len() > MAX_PROVENANCE_BYTES {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::ValueTooLong,
                "component provenance is too long",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::InvalidValue,
                "component provenance contains a control character",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the exact validated provenance.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Canonical source path below `.peritus-harness/components/`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourcePath(String);

impl SourcePath {
    /// Validates exact containment below the component source root.
    ///
    /// # Errors
    ///
    /// Rejects nonportable, noncanonical, uncontained, or protected paths.
    pub fn new(value: impl Into<String>) -> Result<Self, HarnessDomainError> {
        let value = value.into();
        validate_relative_path(&value)?;
        let prefix = ".peritus-harness/components/";
        if !value.starts_with(prefix) || value.len() == prefix.len() {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::InvalidPath,
                "source path is not below .peritus-harness/components/",
            ));
        }
        if value.split('/').skip(2).any(c1_protected_component) {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::ProtectedPath,
                "source path contains protected workspace metadata",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the normalized source path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Canonical C1-relative materialization target outside workspace control roots.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetPath(String);

impl TargetPath {
    /// Validates a normalized, non-protected workspace-relative target.
    ///
    /// # Errors
    ///
    /// Rejects nonportable, noncanonical, absolute, or protected workspace paths.
    pub fn new(value: impl Into<String>) -> Result<Self, HarnessDomainError> {
        let value = value.into();
        validate_relative_path(&value)?;
        if value.split('/').any(target_protected_component) {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::ProtectedPath,
                "target path names a protected workspace control root",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the normalized workspace-relative target.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_relative_path(value: &str) -> Result<(), HarnessDomainError> {
    if value.is_empty()
        || value.len() > MAX_PATH_BYTES
        || value.split('/').count() > MAX_PATH_COMPONENTS
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value.contains('\0')
        || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f || byte == b':')
        || value.split('/').any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || segment.len() > MAX_PATH_COMPONENT_BYTES
                || segment.ends_with(['.', ' '])
                || windows_device_name(segment)
        })
    {
        return Err(HarnessDomainError::detail(
            HarnessDomainErrorKind::InvalidPath,
            "path is not normalized portable relative UTF-8",
        ));
    }
    Ok(())
}

fn c1_protected_component(component: &str) -> bool {
    component.eq_ignore_ascii_case(".git")
        || component.eq_ignore_ascii_case(".peritus")
        || component.to_ascii_lowercase().starts_with(".peritus-txn-")
}

fn target_protected_component(component: &str) -> bool {
    c1_protected_component(component) || component.eq_ignore_ascii_case(".peritus-harness")
}

fn windows_device_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let uppercase = stem.to_ascii_uppercase();
    matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || (uppercase.len() == 4
            && (uppercase.starts_with("COM") || uppercase.starts_with("LPT"))
            && matches!(uppercase.as_bytes()[3], b'1'..=b'9'))
}

/// Validated component media type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MediaType(String);

impl MediaType {
    /// Validates a bounded visible-ASCII media type containing a type/subtype separator.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, non-visible, or malformed media types.
    pub fn new(value: impl Into<String>) -> Result<Self, HarnessDomainError> {
        let value = value.into();
        let essence = value.split(';').next().unwrap_or_default();
        let valid = !value.is_empty()
            && value.len() <= MAX_MEDIA_TYPE_BYTES
            && value.bytes().all(|byte| byte.is_ascii_graphic())
            && essence.split_once('/').is_some_and(|(kind, subtype)| {
                !kind.is_empty() && !subtype.is_empty() && !subtype.contains('/')
            });
        if !valid {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::InvalidValue,
                "media type is not bounded canonical ASCII type/subtype",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the validated media type.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
