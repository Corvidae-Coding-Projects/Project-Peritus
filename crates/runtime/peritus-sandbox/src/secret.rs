//! Secret references and delivery contracts. Secret values never enter this crate.

use crate::{EnvironmentName, SandboxError, SandboxPath};
use peritus_types::{ResourceId, Sha256Digest};

const MAX_GRANTS: usize = 128;
const MAX_LABEL_BYTES: usize = 128;

/// An opaque secret identity bound to a capability-addressed resource and version digest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SecretReference {
    resource_id: ResourceId,
    version: Sha256Digest,
}

impl SecretReference {
    /// Creates a reference without acquiring or containing secret material.
    #[must_use]
    pub const fn new(resource_id: ResourceId, version: Sha256Digest) -> Self {
        Self { resource_id, version }
    }
    /// Returns the resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }
    /// Returns the version digest.
    #[must_use]
    pub const fn version(&self) -> Sha256Digest {
        self.version
    }
}

/// A stable label for a brokered secret handle.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BrokeredHandleLabel(String);

impl BrokeredHandleLabel {
    /// Validates a nonempty printable ASCII label.
    ///
    /// # Errors
    /// Rejects empty, oversized, or control-containing labels.
    pub fn new(value: impl Into<String>) -> Result<Self, SandboxError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_LABEL_BYTES
            || !value.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(crate::error::invalid("invalid brokered handle label"));
        }
        Ok(Self(value))
    }
    /// Returns the label.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Authorized delivery mechanism for one secret reference.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecretDelivery {
    /// Deliver through one environment variable.
    Environment(EnvironmentName),
    /// Materialize at an exact sandbox path.
    File(SandboxPath),
    /// Deliver through a backend-owned handle.
    BrokeredHandle(BrokeredHandleLabel),
}

/// One exact secret delivery grant.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SecretGrant {
    reference: SecretReference,
    delivery: SecretDelivery,
}

impl SecretGrant {
    /// Creates an exact grant.
    #[must_use]
    pub const fn new(reference: SecretReference, delivery: SecretDelivery) -> Self {
        Self { reference, delivery }
    }
    /// Returns the reference.
    #[must_use]
    pub const fn reference(&self) -> SecretReference {
        self.reference
    }
    /// Returns delivery mechanism and destination.
    #[must_use]
    pub const fn delivery(&self) -> &SecretDelivery {
        &self.delivery
    }
}

/// Secret delivery required by an invocation.
pub type SecretRequirement = SecretGrant;

/// Canonical exact-match secret contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretContract {
    grants: Vec<SecretGrant>,
}

impl SecretContract {
    /// Validates, sorts, and deduplicates grants.
    ///
    /// # Errors
    /// Returns a limit error for more than 128 grants.
    pub fn new(mut grants: Vec<SecretGrant>) -> Result<Self, SandboxError> {
        if grants.len() > MAX_GRANTS {
            return Err(crate::error::bound("too many secret grants"));
        }
        grants.sort();
        grants.dedup();
        Ok(Self { grants })
    }
    /// Returns no grants.
    #[must_use]
    pub const fn deny_all() -> Self {
        Self { grants: Vec::new() }
    }
    /// Returns canonical grants.
    #[must_use]
    pub fn grants(&self) -> &[SecretGrant] {
        &self.grants
    }
    /// Reports whether an exact delivery is granted.
    #[must_use]
    pub fn permits(&self, requirement: &SecretRequirement) -> bool {
        self.grants.contains(requirement)
    }
}
