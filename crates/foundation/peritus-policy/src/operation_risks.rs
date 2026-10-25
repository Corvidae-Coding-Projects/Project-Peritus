//! Exact policy-owned risk union for whole-request escalation.

use crate::{
    CapabilityScope, OperationClass, OperationDescriptor, OperationRegistry, Permission, RiskClass,
    RiskSet,
};
use core::cmp::Ordering;

impl OperationClass {
    pub(crate) const fn mandatory_risk(self) -> RiskClass {
        match self {
            Self::Inspection => RiskClass::Read,
            Self::WorkspaceMutation => RiskClass::ScopedWrite,
            Self::Execution => RiskClass::Execution,
            Self::Network => RiskClass::Network,
            Self::DependencyEnvironment => RiskClass::DependencyEnvironment,
            Self::RepositoryHistoryMutation => RiskClass::RepositoryHistoryMutation,
            Self::SecretUse => RiskClass::SecretUse,
            Self::ExternalSideEffect | Self::RawEffect => RiskClass::ExternalSideEffect,
            Self::Acceptance | Self::Waiver | Self::PolicyAmendment | Self::HumanAuthority => {
                RiskClass::PolicyAuthority
            }
            Self::HarnessPromotion => RiskClass::HarnessPromotion,
        }
    }
}
fn descriptor_has_risk_checked(
    descriptors: &[OperationDescriptor],
    name: &peritus_types::CapabilityName,
    risk: RiskClass,
    index: usize,
) -> bool {
    if index == descriptors.len() {
        false
    } else {
        match descriptors[index].name_cmp(name) {
            Ordering::Less => descriptor_has_risk_checked(descriptors, name, risk, index + 1),
            Ordering::Equal => descriptors[index].has_risk(risk),
            Ordering::Greater => false,
        }
    }
}
fn permissions_require_risk_checked(
    permissions: &[Permission],
    descriptors: &[OperationDescriptor],
    risk: RiskClass,
    index: usize,
) -> bool {
    if index == permissions.len() {
        false
    } else if descriptor_has_risk_checked(
        descriptors,
        permissions[index].capability_name(),
        risk,
        0,
    ) {
        true
    } else {
        permissions_require_risk_checked(permissions, descriptors, risk, index + 1)
    }
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "verified sibling helper must remain inaccessible outside the crate"
)]
pub(super) fn append_required_risk(
    permissions: &[Permission],
    descriptors: &[OperationDescriptor],
    risk: RiskClass,
    mut values: Vec<RiskClass>,
) -> Vec<RiskClass> {
    if permissions_require_risk_checked(permissions, descriptors, risk, 0) {
        values.push(risk);
    }
    values
}
impl OperationRegistry {
    fn admitted_descriptor<'a>(
        &'a self,
        permission: &Permission,
        _role: crate::ActorRole,
    ) -> &'a OperationDescriptor {
        if let Some(descriptor) = self.descriptor_for(permission.capability_name()) {
            return descriptor;
        }
        &self.as_slice()[0]
    }
    fn required_risk_for_scope(&self, scope: &CapabilityScope) -> RiskClass {
        let permissions = scope.permissions();
        let first = &permissions.as_slice()[0];
        let descriptor = self.admitted_descriptor(first, scope.role());

        descriptor.authenticated_mandatory_risk()
    }
    pub(crate) fn risks_for_scope(&self, scope: &CapabilityScope) -> RiskSet {
        let permissions = scope.permissions();
        let descriptors = self.as_slice();
        let values =
            crate::operation_risk_fold::collect_all_risks(permissions.as_slice(), descriptors);
        let _required_risk = self.required_risk_for_scope(scope);
        RiskSet::from_derived_values(values)
    }
}
