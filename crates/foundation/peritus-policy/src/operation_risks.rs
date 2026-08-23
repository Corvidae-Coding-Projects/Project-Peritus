//! Exact policy-owned risk union for whole-request escalation.

use crate::{
    CapabilityScope, OperationClass, OperationDescriptor, OperationRegistry, Permission,
    RiskClass, RiskSet,
};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

impl OperationClass {
    /// Returns the non-configurable mandatory risk for an operation class.
    pub open spec fn spec_mandatory_risk(self) -> RiskClass {
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

    pub(crate) const fn mandatory_risk(self) -> (risk: RiskClass)
        ensures risk == self.spec_mandatory_risk(),
    {
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

pub open spec fn descriptor_has_risk_from(
    descriptors: Seq<OperationDescriptor>,
    name: Seq<u8>,
    risk: RiskClass,
    index: nat,
) -> bool
    decreases descriptors.len() - index,
{
    if index >= descriptors.len() {
        false
    } else {
        match descriptors[index as int].spec_name_cmp(name) {
            Ordering::Less => descriptor_has_risk_from(descriptors, name, risk, index + 1),
            Ordering::Equal => descriptors[index as int].spec_has_risk(risk),
            Ordering::Greater => false,
        }
    }
}

fn descriptor_has_risk_checked(
    descriptors: &[OperationDescriptor],
    name: &peritus_types::CapabilityName,
    risk: RiskClass,
    index: usize,
) -> (result: bool)
    requires index <= descriptors.len(),
    ensures result == descriptor_has_risk_from(descriptors@, name.spec_bytes(), risk, index as nat),
    decreases descriptors.len() - index,
{
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

pub open spec fn permissions_require_risk_from(
    permissions: Seq<Permission>,
    descriptors: Seq<OperationDescriptor>,
    risk: RiskClass,
    index: nat,
) -> bool
    decreases permissions.len() - index,
{
    if index >= permissions.len() {
        false
    } else {
        descriptor_has_risk_from(
            descriptors,
            permissions[index as int].spec_capability_name(),
            risk,
            0,
        ) || permissions_require_risk_from(permissions, descriptors, risk, index + 1)
    }
}

fn permissions_require_risk_checked(
    permissions: &[Permission],
    descriptors: &[OperationDescriptor],
    risk: RiskClass,
    index: usize,
) -> (result: bool)
    requires index <= permissions.len(),
    ensures result == permissions_require_risk_from(
        permissions@,
        descriptors@,
        risk,
        index as nat,
    ),
    decreases permissions.len() - index,
{
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

proof fn resolved_descriptor_lookup_is_some(
    descriptors: Seq<OperationDescriptor>,
    role: crate::ActorRole,
    permission: &Permission,
    index: nat,
)
    requires
        index <= descriptors.len(),
        crate::model::descriptor_denial_from(descriptors, role, permission, index).is_none(),
    ensures crate::operation_access::descriptor_for_from(
        descriptors,
        permission.spec_capability_name(),
        index,
    ).is_some(),
    decreases descriptors.len() - index,
{
    if index < descriptors.len() {
        match descriptors[index as int].spec_name_cmp(permission.spec_capability_name()) {
            Ordering::Less => {
                resolved_descriptor_lookup_is_some(
                    descriptors,
                    role,
                    permission,
                    index + 1,
                );
            }
            Ordering::Equal => {}
            Ordering::Greater => {}
        }
    }
}

proof fn lookup_risk_is_descriptor_risk(
    descriptors: Seq<OperationDescriptor>,
    permission: &Permission,
    descriptor: OperationDescriptor,
    risk: RiskClass,
    index: nat,
)
    requires
        index <= descriptors.len(),
        crate::operation_access::descriptor_for_from(
            descriptors,
            permission.spec_capability_name(),
            index,
        ) == Some(descriptor),
        descriptor.spec_has_risk(risk),
    ensures descriptor_has_risk_from(
        descriptors,
        permission.spec_capability_name(),
        risk,
        index,
    ),
    decreases descriptors.len() - index,
{
    if index < descriptors.len() {
        match descriptors[index as int].spec_name_cmp(permission.spec_capability_name()) {
            Ordering::Less => lookup_risk_is_descriptor_risk(
                descriptors,
                permission,
                descriptor,
                risk,
                index + 1,
            ),
            Ordering::Equal => {},
            Ordering::Greater => {},
        }
    }
}

proof fn some_required_risk_makes_union_nonempty(
    registry: &OperationRegistry,
    permissions: Seq<Permission>,
)
    requires
        exists |risk: RiskClass| registry.spec_permissions_require_risk(permissions, risk),
    ensures registry.spec_risks_for_permissions(permissions).len() > 0,
{
    let risk = choose |risk: RiskClass| registry.spec_permissions_require_risk(permissions, risk);
    match risk {
        RiskClass::Read
        | RiskClass::ScopedWrite
        | RiskClass::Execution
        | RiskClass::Network
        | RiskClass::DependencyEnvironment
        | RiskClass::RepositoryHistoryMutation
        | RiskClass::SecretUse
        | RiskClass::ExternalSideEffect
        | RiskClass::PolicyAuthority
        | RiskClass::HarnessPromotion => {}
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
) -> (result: Vec<RiskClass>)
    requires
        RiskSet::spec_values_are_sorted(values@),
        forall |index: int| 0 <= index < values@.len() ==>
            #[trigger] values@[index].spec_rank() < risk.spec_rank(),
    ensures
        result@ == if permissions_require_risk_from(
            permissions@,
            descriptors@,
            risk,
            0,
        ) { values@.push(risk) } else { values@ },
        RiskSet::spec_values_are_sorted(result@),
        forall |index: int| 0 <= index < result@.len() ==>
            #[trigger] result@[index].spec_rank() <= risk.spec_rank(),
{
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
    ) -> (descriptor: &'a OperationDescriptor)
        requires crate::model::descriptor_denial_from(
            self.spec_descriptors(),
            _role,
            permission,
            0,
        ).is_none(),
        ensures
            crate::operation_access::descriptor_for_from(
                self.spec_descriptors(),
                permission.spec_capability_name(),
                0,
            ) == Some(*descriptor),
    {
        proof {
            resolved_descriptor_lookup_is_some(
                self.spec_descriptors(),
                _role,
                permission,
                0,
            );
        }
        if let Some(descriptor) = self.descriptor_for(permission.capability_name()) {
            return descriptor;
        }
        proof { assert(false); }
        &self.as_slice()[0]
    }

    fn required_risk_for_scope(&self, scope: &CapabilityScope) -> (risk: RiskClass)
        requires self.spec_first_denial(scope).is_none(),
        ensures self.spec_permissions_require_risk(scope.spec_permissions(), risk),
    {
        let permissions = scope.permissions();
        let first = &permissions.as_slice()[0];
        proof {
            assert(crate::model::descriptor_denial_from(
                self.spec_descriptors(),
                scope.spec_role(),
                first,
                0,
            ).is_none());
        }
        let descriptor = self.admitted_descriptor(first, scope.role());
        let risk = descriptor.authenticated_mandatory_risk();
        proof {
            assert(descriptor.spec_has_risk(risk));
            lookup_risk_is_descriptor_risk(
                self.spec_descriptors(),
                first,
                *descriptor,
                risk,
                0,
            );
        }
        risk
    }

    pub(crate) open spec fn spec_permissions_require_risk(
        &self,
        permissions: Seq<Permission>,
        risk: RiskClass,
    ) -> bool {
        permissions_require_risk_from(permissions, self.spec_descriptors(), risk, 0)
    }

    pub(crate) open spec fn spec_risks_for_permissions(
        &self,
        permissions: Seq<Permission>,
    ) -> Seq<RiskClass> {
        crate::operation_risk_fold::all_risks(
            permissions,
            self.spec_descriptors(),
        )
    }

    pub(crate) fn risks_for_scope(&self, scope: &CapabilityScope) -> (risks: RiskSet)
        requires self.spec_first_denial(scope).is_none(),
        ensures risks.spec_values() == self.spec_risks_for_permissions(scope.spec_permissions()),
    {
        let permissions = scope.permissions();
        let descriptors = self.as_slice();
        let values = crate::operation_risk_fold::collect_all_risks(
            permissions.as_slice(),
            descriptors,
        );
        let _required_risk = self.required_risk_for_scope(scope);
        proof {
            assert(RiskSet::spec_values_are_sorted(values@));
            some_required_risk_makes_union_nonempty(self, scope.spec_permissions());
            assert(values@.len() > 0);
        }
        RiskSet::from_derived_values(values)
    }
}

} // verus!
