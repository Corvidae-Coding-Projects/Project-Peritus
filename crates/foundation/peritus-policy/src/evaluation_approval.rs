//! Executable exact conjunction of all matching approval restrictions.

use crate::{
    ApprovalRequirement, AuthorizationDenialReason, CapabilityScope, PolicyDefinition, PolicyError,
};

#[doc = " Total result of conjoining every matching approval restriction."]
pub enum RestrictionResult {
    #[doc = " Exact optional effective approval requirement."]
    Accepted(Option<ApprovalRequirement>),
    #[doc = " Stable semantic approval conflict denial."]
    Denied(AuthorizationDenialReason),
}
enum AccumulationResult {
    Accumulated(Option<ApprovalRequirement>),
    Conflict,
}
fn accumulate_requirement(
    approval: Option<ApprovalRequirement>,
    requirement: &ApprovalRequirement,
) -> Result<AccumulationResult, PolicyError> {
    match approval {
        None => Ok(AccumulationResult::Accumulated(Some(requirement.duplicate()))),
        Some(accumulated) => {
            let conjunction = accumulated.conjunction(requirement)?;
            let Some(value) = conjunction else {
                return Ok(AccumulationResult::Conflict);
            };
            Ok(AccumulationResult::Accumulated(Some(value)))
        }
    }
}
#[doc = " Conjoins every matching approval restriction in canonical policy order."]
pub fn approval_conjunction(
    policy: &PolicyDefinition,
    requested: &CapabilityScope,
) -> Result<RestrictionResult, PolicyError> {
    let mut approval: Option<ApprovalRequirement> = None;
    let layers = policy.layers();
    let mut layer_index = 0;
    while layer_index < layers.len() {
        let rules = layers[layer_index].rules();
        let mut rule_index = 0;
        while rule_index < rules.len() {
            let rule = &rules[rule_index];
            let requirement = rule.matching_approval_requirement(requested);
            if let Some(requirement) = requirement {
                match accumulate_requirement(approval, requirement)? {
                    AccumulationResult::Accumulated(next) => approval = next,
                    AccumulationResult::Conflict => {
                        return Ok(RestrictionResult::Denied(
                            AuthorizationDenialReason::ApprovalConstraintConflict,
                        ));
                    }
                }
            }
            rule_index += 1;
        }
        layer_index += 1;
    }
    Ok(RestrictionResult::Accepted(approval))
}
