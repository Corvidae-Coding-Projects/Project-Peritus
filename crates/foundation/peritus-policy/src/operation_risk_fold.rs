//! Canonical fixed-order fold for authenticated whole-request risks.

use crate::{OperationDescriptor, Permission, RiskClass};

fn collect_lower_risks(
    permissions: &[Permission],
    descriptors: &[OperationDescriptor],
) -> Vec<RiskClass> {
    let mut values = Vec::new();
    values = crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::Read,
        values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::ScopedWrite,
        values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::Execution,
        values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::Network,
        values,
    );
    crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::DependencyEnvironment,
        values,
    )
}
pub fn collect_all_risks(
    permissions: &[Permission],
    descriptors: &[OperationDescriptor],
) -> Vec<RiskClass> {
    let mut values = collect_lower_risks(permissions, descriptors);
    values = crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::RepositoryHistoryMutation,
        values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::SecretUse,
        values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::ExternalSideEffect,
        values,
    );
    values = crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::PolicyAuthority,
        values,
    );
    crate::operation_risks::append_required_risk(
        permissions,
        descriptors,
        RiskClass::HarnessPromotion,
        values,
    )
}
