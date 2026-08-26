//! Schema compatibility and inert provider/platform feature requirements.

use peritus_types::Sha256Digest;

use crate::domain::{ComponentId, ComponentKind, HarnessDomainError, HarnessDomainErrorKind};

const MAX_FEATURE_TAG_BYTES: usize = 128;

/// Nonzero schema version of one component declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchemaVersion(u32);

impl SchemaVersion {
    /// Creates a nonzero schema version.
    ///
    /// # Errors
    ///
    /// Rejects version zero, which is reserved.
    pub const fn new(value: u32) -> Result<Self, HarnessDomainError> {
        if value == 0 {
            Err(HarnessDomainError::plain(HarnessDomainErrorKind::InvalidSchemaVersion))
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the primitive schema number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Inclusive, nonempty schema compatibility interval.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SchemaInterval {
    minimum: SchemaVersion,
    maximum: SchemaVersion,
}

impl SchemaInterval {
    /// Creates an inclusive interval with `minimum <= maximum`.
    ///
    /// # Errors
    ///
    /// Rejects an inverted interval.
    pub const fn new(
        minimum: SchemaVersion,
        maximum: SchemaVersion,
    ) -> Result<Self, HarnessDomainError> {
        if minimum.0 > maximum.0 {
            Err(HarnessDomainError::plain(HarnessDomainErrorKind::InvalidSchemaInterval))
        } else {
            Ok(Self { minimum, maximum })
        }
    }

    /// Returns the inclusive minimum version.
    #[must_use]
    pub const fn minimum(self) -> SchemaVersion {
        self.minimum
    }

    /// Returns the inclusive maximum version.
    #[must_use]
    pub const fn maximum(self) -> SchemaVersion {
        self.maximum
    }

    /// Returns whether the interval includes `version`.
    #[must_use]
    pub const fn contains(self, version: SchemaVersion) -> bool {
        self.minimum.0 <= version.0 && version.0 <= self.maximum.0
    }
}

/// Canonical inert feature tag; unknown tags carry no implicit support.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FeatureTag(String);

impl FeatureTag {
    /// Validates a lowercase portable feature tag.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, uppercase, or structurally invalid tags.
    pub fn new(value: impl Into<String>) -> Result<Self, HarnessDomainError> {
        let value = value.into();
        if value.is_empty() {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::EmptyValue,
                "feature tag is empty",
            ));
        }
        if value.len() > MAX_FEATURE_TAG_BYTES {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::ValueTooLong,
                "feature tag exceeds 128 bytes",
            ));
        }
        let mut bytes = value.bytes();
        let first = bytes
            .next()
            .ok_or_else(|| HarnessDomainError::plain(HarnessDomainErrorKind::EmptyValue))?;
        if !first.is_ascii_lowercase()
            || !bytes.all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::InvalidValue,
                "feature tag is not canonical lowercase ASCII",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the canonical feature text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FeatureTag {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Exact requirement imposed on one dependency.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyRequirement {
    component_id: ComponentId,
    required_kind: ComponentKind,
    compatible_schema: SchemaInterval,
    exact_content_digest: Option<Sha256Digest>,
}

impl DependencyRequirement {
    /// Constructs a dependency requirement from already validated values.
    #[must_use]
    pub const fn new(
        component_id: ComponentId,
        required_kind: ComponentKind,
        compatible_schema: SchemaInterval,
        exact_content_digest: Option<Sha256Digest>,
    ) -> Self {
        Self { component_id, required_kind, compatible_schema, exact_content_digest }
    }

    /// Returns the required component identity.
    #[must_use]
    pub const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }
    /// Returns the exact required kind.
    #[must_use]
    pub const fn required_kind(&self) -> ComponentKind {
        self.required_kind
    }
    /// Returns the inclusive compatible schema interval.
    #[must_use]
    pub const fn compatible_schema(&self) -> SchemaInterval {
        self.compatible_schema
    }
    /// Returns the strengthening exact digest requirement, when present.
    #[must_use]
    pub const fn exact_content_digest(&self) -> Option<Sha256Digest> {
        self.exact_content_digest
    }
}

/// A component's own schema support and explicit runtime feature requirements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityContract {
    supported_schema: SchemaInterval,
    provider_features: Vec<FeatureTag>,
    platform_features: Vec<FeatureTag>,
}

impl CompatibilityContract {
    /// Creates a contract, requiring both feature sets in strict canonical order.
    ///
    /// # Errors
    ///
    /// Rejects duplicate or descending feature tags.
    pub fn new(
        supported_schema: SchemaInterval,
        provider_features: Vec<FeatureTag>,
        platform_features: Vec<FeatureTag>,
    ) -> Result<Self, HarnessDomainError> {
        validate_feature_order(&provider_features, "provider")?;
        validate_feature_order(&platform_features, "platform")?;
        Ok(Self { supported_schema, provider_features, platform_features })
    }

    /// Returns the component's own supported schema interval.
    #[must_use]
    pub const fn supported_schema(&self) -> SchemaInterval {
        self.supported_schema
    }
    /// Borrows provider feature requirements in canonical order.
    #[must_use]
    pub fn provider_features(&self) -> &[FeatureTag] {
        &self.provider_features
    }
    /// Borrows platform feature requirements in canonical order.
    #[must_use]
    pub fn platform_features(&self) -> &[FeatureTag] {
        &self.platform_features
    }
}

fn validate_feature_order(
    features: &[FeatureTag],
    family: &'static str,
) -> Result<(), HarnessDomainError> {
    for pair in features.windows(2) {
        if pair[0] >= pair[1] {
            return Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::NonCanonicalOrder,
                format!("{family} feature requirements are not in strict canonical order"),
            ));
        }
    }
    Ok(())
}
