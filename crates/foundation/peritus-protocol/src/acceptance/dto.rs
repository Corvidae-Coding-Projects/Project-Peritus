//! Stable data-only projections of non-copy B2 contract components.

use peritus_spec::EvidenceRequirementId;
use peritus_spec::{
    FindingSeverity, GateDefinition, GateExecutionPlan, ReviewCategory, ReviewPolicy,
    ReviewerIndependence,
};
use peritus_types::GateId;

/// One gate node with canonical dependency and evidence identifiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateDefinitionDto {
    /// Stable gate identity.
    pub id: GateId,
    /// Immutable execution plan.
    pub plan: GateExecutionPlan,
    /// Canonical direct dependencies.
    pub dependencies: Vec<GateId>,
    /// Canonical gate-specific evidence declarations.
    pub required_evidence: Vec<EvidenceRequirementId>,
}

/// Complete checked review-policy data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewPolicyDto {
    /// Canonical nonempty review categories.
    pub required_categories: Vec<ReviewCategory>,
    /// Required reviewer count.
    pub reviewer_quorum: u16,
    /// Required independence conjunction.
    pub independence: ReviewerIndependence,
    /// Lowest blocking finding severity.
    pub blocking_severity: FindingSeverity,
}

impl From<&GateDefinition> for GateDefinitionDto {
    fn from(value: &GateDefinition) -> Self {
        Self {
            id: value.id(),
            plan: value.plan(),
            dependencies: value.dependencies().to_vec(),
            required_evidence: value.required_evidence().to_vec(),
        }
    }
}

impl From<&ReviewPolicy> for ReviewPolicyDto {
    fn from(value: &ReviewPolicy) -> Self {
        Self {
            required_categories: value.required_categories().to_vec(),
            reviewer_quorum: value.reviewer_quorum(),
            independence: value.independence(),
            blocking_severity: value.blocking_severity(),
        }
    }
}
