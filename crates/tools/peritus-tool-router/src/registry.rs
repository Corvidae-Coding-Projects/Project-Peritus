//! Canonical immutable descriptor registry bound to B1 operations.

use std::{collections::BTreeSet, sync::Arc};

use peritus_policy::OperationRegistry;
use peritus_tool_protocol::{ImplementationIdentity, SemanticVersion, ToolDescriptor};
use peritus_types::CapabilityName;

use crate::{RouterError, RouterErrorKind};

/// Canonical immutable registered descriptor set.
pub struct ToolRegistry {
    descriptors: Vec<Arc<ToolDescriptor>>,
}

impl ToolRegistry {
    /// Validates canonical order, identity uniqueness, digest integrity, and exact B1 binding.
    ///
    /// # Errors
    ///
    /// Rejects empty/noncanonical registries, duplicate implementation identities, descriptor
    /// digest drift, or any exact operation mismatch with the authenticated B1 registry.
    pub fn new(
        descriptors: Vec<Arc<ToolDescriptor>>,
        operations: &OperationRegistry,
    ) -> Result<Self, RouterError> {
        if descriptors.is_empty() {
            return Err(registry("tool registry must not be empty"));
        }
        if descriptors.windows(2).any(|pair| {
            (pair[0].name().as_str(), pair[0].version())
                >= (pair[1].name().as_str(), pair[1].version())
        }) {
            return Err(registry("descriptors are not in strict canonical name/version order"));
        }
        let mut implementations = BTreeSet::new();
        for descriptor in &descriptors {
            if !implementations.insert(descriptor.implementation_identity().as_str()) {
                return Err(registry("implementation identity is registered more than once"));
            }
            let recomputed = peritus_codec::sha256(&descriptor.canonical_bytes());
            if descriptor.descriptor_digest().get() != recomputed
                || descriptor.schema_digest() != descriptor.schema().digest()
            {
                return Err(registry("descriptor or schema digest differs from canonical bytes"));
            }
            let Some(operation) = operations.descriptor_for(descriptor.name()) else {
                return Err(registry("descriptor operation is absent from the B1 registry"));
            };
            if operation != descriptor.operation() {
                return Err(registry("descriptor does not equal its authenticated B1 operation"));
            }
        }
        Ok(Self { descriptors })
    }

    /// Returns an exact name/version match.
    #[must_use]
    pub fn descriptor(
        &self,
        name: &CapabilityName,
        version: SemanticVersion,
    ) -> Option<Arc<ToolDescriptor>> {
        self.descriptors
            .binary_search_by(|descriptor| {
                (descriptor.name().as_str(), descriptor.version()).cmp(&(name.as_str(), version))
            })
            .ok()
            .map(|index| Arc::clone(&self.descriptors[index]))
    }

    /// Borrows descriptors in canonical order.
    #[must_use]
    pub fn descriptors(&self) -> &[Arc<ToolDescriptor>] {
        &self.descriptors
    }

    /// Returns a descriptor by immutable implementation identity.
    #[must_use]
    pub fn by_implementation(
        &self,
        identity: &ImplementationIdentity,
    ) -> Option<Arc<ToolDescriptor>> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.implementation_identity() == identity)
            .map(Arc::clone)
    }
}

const fn registry(detail: &'static str) -> RouterError {
    RouterError::new(RouterErrorKind::Registry, "register tools", detail)
}
