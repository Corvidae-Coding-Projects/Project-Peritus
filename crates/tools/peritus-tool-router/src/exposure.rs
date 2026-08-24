//! Canonical role/capability exposure intersection.

use std::sync::Arc;

use peritus_policy::{ActorRole, CapabilityScope};
use peritus_tool_protocol::ToolDescriptor;

use crate::{RouterError, RouterErrorKind, ToolRegistry};

/// Canonical immutable tool set exposed to one exact authenticated scope.
pub struct ExposedTools {
    descriptors: Vec<Arc<ToolDescriptor>>,
}

impl ExposedTools {
    /// Computes descriptor ∩ role separation ∩ exact capability permission.
    ///
    /// # Errors
    ///
    /// Rejects a claimed role that differs from the authenticated capability scope.
    pub fn plan(
        registry: &ToolRegistry,
        role: ActorRole,
        scope: &CapabilityScope,
    ) -> Result<Self, RouterError> {
        if role != scope.role() {
            return Err(RouterError::new(
                RouterErrorKind::Exposure,
                "plan tool exposure",
                "claimed role differs from authenticated capability scope",
            ));
        }
        let descriptors = registry
            .descriptors()
            .iter()
            .filter(|descriptor| {
                role.permits_operation(descriptor.operation().operation_class())
                    && scope
                        .permissions()
                        .as_slice()
                        .iter()
                        .any(|permission| permission.capability_name() == descriptor.name())
            })
            .map(Arc::clone)
            .collect();
        Ok(Self { descriptors })
    }

    /// Borrows exposed descriptors in canonical registry order.
    #[must_use]
    pub fn descriptors(&self) -> &[Arc<ToolDescriptor>] {
        &self.descriptors
    }

    /// Returns whether an exact descriptor digest is exposed.
    #[must_use]
    pub fn contains(&self, candidate: &ToolDescriptor) -> bool {
        self.descriptors
            .iter()
            .any(|descriptor| descriptor.descriptor_digest() == candidate.descriptor_digest())
    }
}
