//! Authenticated operation descriptors and canonical registry lookup.

use crate::{
    ActorRole, AuthorizationDenialReason, CanonicalCollection, CapabilityScope, OperationClass,
    Permission, PolicyError, RiskClass, RiskSet,
};
use core::cmp::Ordering;
use peritus_types::CapabilityName;

#[doc = " Immutable authenticated classification for one exact capability name."]
#[derive(Debug, Eq, PartialEq)]
pub struct OperationDescriptor {
    pub(crate) name: CapabilityName,
    pub(crate) operation_class: OperationClass,
    pub(crate) risks: RiskSet,
}
impl OperationDescriptor {
    pub(crate) const fn authenticated_mandatory_risk(&self) -> RiskClass {
        self.operation_class.mandatory_risk()
    }
    #[doc = " Creates a descriptor from validated domain values."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns an invalid-operation-risk failure when the descriptor omits the mandatory risk"]
    #[doc = " classification for its compiled operation class."]
    pub fn new(
        name: CapabilityName,
        operation_class: OperationClass,
        risks: RiskSet,
    ) -> Result<Self, PolicyError> {
        if !risks.contains(operation_class.mandatory_risk()) {
            return Err(PolicyError::invalid_operation_risk());
        }
        let descriptor = Self { name, operation_class, risks };
        Ok(descriptor)
    }
    #[doc = " Returns the compiled role-separation operation class."]
    #[must_use]
    pub const fn operation_class(&self) -> OperationClass {
        self.operation_class
    }
    pub(crate) fn name_cmp(&self, name: &CapabilityName) -> Ordering {
        self.name.canonical_cmp(name)
    }
    pub(crate) fn has_risk(&self, risk: RiskClass) -> bool {
        self.risks.contains(risk)
    }
}
#[doc = " Canonical immutable mapping from capability names to authenticated descriptors."]
#[derive(Debug, Eq, PartialEq)]
pub struct OperationRegistry {
    pub(crate) descriptors: Vec<OperationDescriptor>,
}
impl OperationRegistry {
    #[doc = " Validates descriptors in strict canonical capability-name order."]
    #[doc = ""]
    #[doc = " Empty registries are valid and authorize no operation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a precise order or duplicate failure."]
    pub fn new(descriptors: Vec<OperationDescriptor>) -> Result<Self, PolicyError> {
        let mut index = 1;
        while index < descriptors.len() {
            match descriptors[index - 1].name_cmp(&descriptors[index].name) {
                Ordering::Less => {}
                Ordering::Equal => {
                    return Err(PolicyError::duplicate_canonical_value(
                        CanonicalCollection::Operations,
                    ));
                }
                Ordering::Greater => {
                    return Err(PolicyError::non_canonical_order(CanonicalCollection::Operations));
                }
            }
            index += 1;
        }
        let registry = Self { descriptors };
        Ok(registry)
    }
    fn descriptor_denial_from(
        descriptors: &[OperationDescriptor],
        role: ActorRole,
        permission: &Permission,
        index: usize,
    ) -> Option<AuthorizationDenialReason> {
        if index == descriptors.len() {
            Some(AuthorizationDenialReason::UnknownOperation)
        } else {
            match descriptors[index].name_cmp(permission.capability_name()) {
                Ordering::Less => {
                    Self::descriptor_denial_from(descriptors, role, permission, index + 1)
                }
                Ordering::Greater => Some(AuthorizationDenialReason::UnknownOperation),
                Ordering::Equal => {
                    if role.permits_operation(descriptors[index].operation_class()) {
                        None
                    } else {
                        Some(AuthorizationDenialReason::RoleSeparation)
                    }
                }
            }
        }
    }
    fn operation_denial_from(
        permissions: &[Permission],
        descriptors: &[OperationDescriptor],
        role: ActorRole,
        index: usize,
    ) -> Option<AuthorizationDenialReason> {
        if index == permissions.len() {
            None
        } else {
            let denial = Self::descriptor_denial_from(descriptors, role, &permissions[index], 0);
            if denial.is_some() {
                denial
            } else {
                Self::operation_denial_from(permissions, descriptors, role, index + 1)
            }
        }
    }
    pub(crate) fn first_denial(
        &self,
        scope: &CapabilityScope,
    ) -> Option<AuthorizationDenialReason> {
        let permissions = scope.permissions();
        Self::operation_denial_from(
            permissions.as_slice(),
            self.descriptors.as_slice(),
            scope.role(),
            0,
        )
    }
    #[doc = " Borrows descriptors in canonical order."]
    #[must_use]
    pub const fn as_slice(&self) -> &[OperationDescriptor] {
        self.descriptors.as_slice()
    }
}
