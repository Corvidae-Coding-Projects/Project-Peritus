//! Deny-wins eligibility and deterministic stable multi-objective selection.

mod assessment;
mod decision;
mod engine;

#[cfg(test)]
mod tests;

pub use assessment::{
    Criterion, CriterionOutcome, CriterionResult, ObjectiveVector, VariantAssessment,
};
pub use decision::{SelectionDecision, SelectionRecord, VariantRejection};
pub use engine::{assess_variant, select_variant};
pub(crate) use assessment::criteria_match_schema;
pub(crate) use engine::criteria_match_policy;
