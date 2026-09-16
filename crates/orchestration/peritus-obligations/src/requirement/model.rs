//! Exact specification shape, classification and variant content.

use super::ObligationSpec;
#[cfg(verus_only)]
use super::{
    AlternativeBranchId, AlternativeGroupId, ConditionId, RequirementClass, SchemaDirection,
};
use vstd::prelude::*;

verus! {

impl ObligationSpec {
    /// The variant and its stored schema direction agree.
    pub open spec fn spec_valid(&self) -> bool {
        match self {
            Self::RequestSchema(requirement) => requirement.spec_direction() == SchemaDirection::Request,
            Self::ResponseSchema(requirement) => requirement.spec_direction() == SchemaDirection::Response,
            _ => true,
        }
    }

    /// Complete content equality, including exact schema field names and identities.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::RequestSchema(left), Self::RequestSchema(right)) => left.spec_same_content(right),
            (Self::ResponseSchema(left), Self::ResponseSchema(right)) => left.spec_same_content(right),
            _ => self == other,
        }
    }

    /// Exact public class of the stored variant.
    pub open spec fn spec_class(&self) -> RequirementClass {
        match self {
            Self::Hard => RequirementClass::Hard,
            Self::Conditional { .. } => RequirementClass::Conditional,
            Self::Alternative { .. } => RequirementClass::Alternative,
            Self::Example => RequirementClass::Example,
            Self::GeneratedOutput => RequirementClass::GeneratedOutput,
            Self::Performance(_) => RequirementClass::Performance,
            Self::LifecycleIngress(_) => RequirementClass::LifecycleIngress,
            Self::RequestSchema(_) => RequirementClass::RequestSchema,
            Self::ResponseSchema(_) => RequirementClass::ResponseSchema,
            Self::BrowserSemantics(_) => RequirementClass::BrowserSemantics,
            Self::ExternalEffect { .. } => RequirementClass::ExternalEffect,
        }
    }

    /// Exact condition identity only for a conditional requirement.
    pub open spec fn spec_condition_id(&self) -> Option<ConditionId> {
        match self { Self::Conditional { condition_id } => Some(*condition_id), _ => None }
    }

    /// Exact alternative group and branch only for an alternative requirement.
    pub open spec fn spec_alternative(&self) -> Option<(AlternativeGroupId, AlternativeBranchId)> {
        match self {
            Self::Alternative { group_id, branch_id } => Some((*group_id, *branch_id)),
            _ => None,
        }
    }
}

} // verus!
