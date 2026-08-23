//! Canonical fixed-order fold for authenticated whole-request risks.

use crate::{OperationDescriptor, Permission, RiskClass};
#[cfg(verus_only)]
use crate::RiskSet;
use vstd::prelude::*;

verus! {

pub open spec fn lower_risks(
    permissions: Seq<Permission>,
    descriptors: Seq<OperationDescriptor>,
) -> Seq<RiskClass> {
    let values = Seq::empty();
    let values = if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::Read, 0,
    ) { values.push(RiskClass::Read) } else { values };
    let values = if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::ScopedWrite, 0,
    ) { values.push(RiskClass::ScopedWrite) } else { values };
    let values = if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::Execution, 0,
    ) { values.push(RiskClass::Execution) } else { values };
    let values = if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::Network, 0,
    ) { values.push(RiskClass::Network) } else { values };
    if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::DependencyEnvironment, 0,
    ) { values.push(RiskClass::DependencyEnvironment) } else { values }
}

pub open spec fn upper_risks(
    permissions: Seq<Permission>,
    descriptors: Seq<OperationDescriptor>,
    values: Seq<RiskClass>,
) -> Seq<RiskClass> {
    let values = if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::RepositoryHistoryMutation, 0,
    ) { values.push(RiskClass::RepositoryHistoryMutation) } else { values };
    let values = if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::SecretUse, 0,
    ) { values.push(RiskClass::SecretUse) } else { values };
    let values = if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::ExternalSideEffect, 0,
    ) { values.push(RiskClass::ExternalSideEffect) } else { values };
    let values = if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::PolicyAuthority, 0,
    ) { values.push(RiskClass::PolicyAuthority) } else { values };
    if crate::operation_risks::permissions_require_risk_from(
        permissions, descriptors, RiskClass::HarnessPromotion, 0,
    ) { values.push(RiskClass::HarnessPromotion) } else { values }
}

pub open spec fn all_risks(
    permissions: Seq<Permission>,
    descriptors: Seq<OperationDescriptor>,
) -> Seq<RiskClass> {
    upper_risks(permissions, descriptors, lower_risks(permissions, descriptors))
}

fn collect_lower_risks(
    permissions: &[Permission],
    descriptors: &[OperationDescriptor],
) -> (values: Vec<RiskClass>)
    ensures
        values@ == lower_risks(permissions@, descriptors@),
        RiskSet::spec_values_are_sorted(values@),
        forall |index: int| 0 <= index < values@.len() ==>
            #[trigger] values@[index].spec_rank()
                <= RiskClass::DependencyEnvironment.spec_rank(),
{
    let mut values = Vec::new();
    values = crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::Read, values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::ScopedWrite, values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::Execution, values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::Network, values,
    );
    crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::DependencyEnvironment, values,
    )
}

pub fn collect_all_risks(
    permissions: &[Permission],
    descriptors: &[OperationDescriptor],
) -> (values: Vec<RiskClass>)
    ensures
        values@ == all_risks(permissions@, descriptors@),
        RiskSet::spec_values_are_sorted(values@),
{
    let mut values = collect_lower_risks(permissions, descriptors);
    values = crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::RepositoryHistoryMutation, values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::SecretUse, values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::ExternalSideEffect, values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::PolicyAuthority, values,
    );
    crate::operation_risks::append_required_risk(
        permissions, descriptors, RiskClass::HarnessPromotion, values,
    )
}

} // verus!
