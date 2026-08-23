//! Authenticated operation descriptors and canonical registry lookup.

use crate::{
    ActorRole, AuthorizationDenialReason, CanonicalCollection, CapabilityScope, OperationClass,
    Permission, PolicyError, RiskClass, RiskSet,
};
use core::cmp::Ordering;
use peritus_types::CapabilityName;
use vstd::prelude::*;

verus! {

/// Immutable authenticated classification for one exact capability name.
#[derive(Debug, Eq, PartialEq)]
pub struct OperationDescriptor {
    pub(crate) name: CapabilityName,
    pub(crate) operation_class: OperationClass,
    pub(crate) risks: RiskSet,
}

impl OperationDescriptor {
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool {
        self.risks.spec_contains(self.operation_class.spec_mandatory_risk())
    }

    pub(crate) const fn authenticated_mandatory_risk(&self) -> (risk: RiskClass)
        ensures
            risk == self.spec_operation_class().spec_mandatory_risk(),
            self.risks.spec_contains(risk),
            self.spec_has_risk(risk),
    {
        proof { use_type_invariant(self); }
        self.operation_class.mandatory_risk()
    }

    /// Returns the exact capability-name bytes used by descriptor specifications.
    pub closed spec fn spec_name(&self) -> Seq<u8> { self.name.spec_bytes() }

    /// Returns the exact authenticated canonical risk sequence.
    pub closed spec fn spec_risks(&self) -> Seq<RiskClass> { self.risks.spec_values() }

    /// Returns exact authenticated risk membership for this descriptor.
    pub open spec fn spec_has_risk(&self, risk: RiskClass) -> bool {
        crate::risk::risk_sequence_contains(self.spec_risks(), risk)
    }

    /// Returns the exact capability-name order against supplied canonical bytes.
    pub closed spec fn spec_name_cmp(&self, name: Seq<u8>) -> Ordering {
        peritus_types::canonical_byte_order_from(self.name.spec_bytes(), name, 0)
    }

    /// Returns the compiled operation class used by role-separation specifications.
    pub closed spec fn spec_operation_class(&self) -> OperationClass {
        self.operation_class
    }

    /// Creates a descriptor from validated domain values.
    ///
    /// # Errors
    ///
    /// Returns an invalid-operation-risk failure when the descriptor omits the mandatory risk
    /// classification for its compiled operation class.
    pub fn new(
        name: CapabilityName,
        operation_class: OperationClass,
        risks: RiskSet,
    ) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(descriptor) => {
                    descriptor.spec_name() == name.spec_bytes()
                        && descriptor.spec_operation_class() == operation_class
                        && descriptor.spec_risks() == risks.spec_values()
                        && risks.spec_contains(operation_class.spec_mandatory_risk())
                }
                Err(error) => {
                    !risks.spec_contains(operation_class.spec_mandatory_risk())
                        && error.spec_kind() == crate::PolicyErrorKind::InvalidOperationRisk
                        && error.spec_dimension().is_none()
                        && error.spec_collection().is_none()
                }
            },
    {
        if !risks.contains(operation_class.mandatory_risk()) {
            return Err(PolicyError::invalid_operation_risk());
        }
        let descriptor = Self { name, operation_class, risks };
        reveal(OperationDescriptor::spec_name);
        reveal(OperationDescriptor::spec_operation_class);
        reveal(OperationDescriptor::spec_risks);
        assert(descriptor.spec_name() == name.spec_bytes());
        assert(descriptor.spec_operation_class() == operation_class);
        assert(descriptor.spec_risks() == risks.spec_values());
        Ok(descriptor)
    }

    /// Returns the compiled role-separation operation class.
    #[must_use]
    pub const fn operation_class(&self) -> (operation_class: OperationClass)
        ensures operation_class == self.spec_operation_class(),
    { self.operation_class }

    pub(crate) fn name_cmp(&self, name: &CapabilityName) -> (result: Ordering)
        ensures result == self.spec_name_cmp(name.spec_bytes()),
    {
        self.name.canonical_cmp(name)
    }

    pub(crate) fn has_risk(&self, risk: RiskClass) -> (result: bool)
        ensures result == self.spec_has_risk(risk),
    {
        self.risks.contains(risk)
    }

}

/// Returns the first duplicate or descending adjacent operation descriptor.
pub open spec fn first_operation_order_error(
    descriptors: Seq<OperationDescriptor>,
    index: nat,
) -> Option<crate::PolicyErrorKind>
    decreases descriptors.len() - index,
{
    if index >= descriptors.len() {
        None
    } else {
        match descriptors[index as int - 1].spec_name_cmp(descriptors[index as int].spec_name()) {
            Ordering::Less => first_operation_order_error(descriptors, index + 1),
            Ordering::Equal => Some(crate::PolicyErrorKind::DuplicateCanonicalValue),
            Ordering::Greater => Some(crate::PolicyErrorKind::NonCanonicalOrder),
        }
    }
}

/// Canonical immutable mapping from capability names to authenticated descriptors.
#[derive(Debug, Eq, PartialEq)]
pub struct OperationRegistry {
    pub(crate) descriptors: Vec<OperationDescriptor>,
}

impl OperationRegistry {
    /// Returns the exact canonical descriptor sequence used by specifications.
    pub closed spec fn spec_descriptors(&self) -> Seq<OperationDescriptor> {
        self.descriptors@
    }

    /// Returns the exact first unknown-operation or role-separation denial.
    pub open spec fn spec_first_denial(&self, scope: &CapabilityScope) -> Option<AuthorizationDenialReason> {
        crate::model::operation_denial_from(
            scope.spec_permissions(),
            self.spec_descriptors(),
            scope.spec_role(),
            0,
        )
    }

    /// Validates descriptors in strict canonical capability-name order.
    ///
    /// Empty registries are valid and authorize no operation.
    ///
    /// # Errors
    ///
    /// Returns a precise order or duplicate failure.
    pub fn new(descriptors: Vec<OperationDescriptor>) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(registry) => {
                    first_operation_order_error(descriptors@, 1).is_none()
                        && registry.spec_descriptors() == descriptors@
                }
                Err(error) => {
                    first_operation_order_error(descriptors@, 1) == Some(error.spec_kind())
                        && error.spec_collection() == Some(CanonicalCollection::Operations)
                        && error.spec_dimension().is_none()
                }
            },
    {
        let mut index = 1;
        while index < descriptors.len()
            invariant
                (descriptors.len() == 0 && index == 1)
                    || 1 <= index <= descriptors.len(),
                first_operation_order_error(descriptors@, 1)
                    == first_operation_order_error(descriptors@, index as nat),
            decreases descriptors.len() - index,
        {
            match descriptors[index - 1].name_cmp(&descriptors[index].name) {
                Ordering::Less => {},
                Ordering::Equal => {
                    return Err(PolicyError::duplicate_canonical_value(
                        CanonicalCollection::Operations,
                    ));
                }
                Ordering::Greater => {
                    return Err(PolicyError::non_canonical_order(
                        CanonicalCollection::Operations,
                    ));
                }
            }
            index += 1;
        }
        let registry = Self { descriptors };
        reveal(OperationRegistry::spec_descriptors);
        Ok(registry)
    }

    fn descriptor_denial_from(
        descriptors: &[OperationDescriptor],
        role: ActorRole,
        permission: &Permission,
        index: usize,
    ) -> (result: Option<AuthorizationDenialReason>)
        requires index <= descriptors.len(),
        ensures
            result == crate::model::descriptor_denial_from(
                descriptors@,
                role,
                permission,
                index as nat,
            ),
        decreases descriptors.len() - index,
    {
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
    ) -> (result: Option<AuthorizationDenialReason>)
        requires index <= permissions.len(),
        ensures
            result == crate::model::operation_denial_from(
                permissions@,
                descriptors@,
                role,
                index as nat,
            ),
        decreases permissions.len() - index,
    {
        if index == permissions.len() {
            None
        } else {
            let denial = Self::descriptor_denial_from(
                descriptors,
                role,
                &permissions[index],
                0,
            );
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
    ) -> (result: Option<AuthorizationDenialReason>)
        ensures result == self.spec_first_denial(scope),
    {
        let permissions = scope.permissions();
        Self::operation_denial_from(
            permissions.as_slice(),
            self.descriptors.as_slice(),
            scope.role(),
            0,
        )
    }

    /// Borrows descriptors in canonical order.
    #[must_use]
    pub const fn as_slice(&self) -> (descriptors: &[OperationDescriptor])
        ensures descriptors@ == self.spec_descriptors(),
    { self.descriptors.as_slice() }

}

} // verus!
