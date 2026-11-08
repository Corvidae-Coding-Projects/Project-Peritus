//! Model proposals are validated before they become scheduling or qualification inputs.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::invalid;
use crate::ProductRunnerError;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub id: String,
    /// Exact public obligation IDs; the independent scope reviewer checks semantic coverage.
    pub requirements: Vec<String>,
    pub source_quote: String,
    pub outcome: String,
    pub depends_on: Vec<String>,
    pub kind: CapabilityKind,
    pub positive: String,
    /// Required rejection, preservation, recovery, or an explicit applicability rationale.
    pub negative: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Behavior,
    Artifact,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftHandoff {
    pub capability: String,
    pub implementation: String,
    /// Remaining implementation debt is kept open; it cannot count as drafted coverage.
    pub remaining: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryReview {
    pub scope_complete: bool,
    pub scope_evidence: String,
    pub capabilities: Vec<CapabilityReview>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityReview {
    pub id: String,
    pub satisfied: bool,
    pub positive_observation: String,
    pub negative_observation: String,
    /// Short host-issued references bound to successful commands and their candidate.
    pub commands: Vec<String>,
}

pub(super) fn validate_plan(
    plan: &[Capability],
    source: &str,
    requirements: &[String],
) -> Result<(), ProductRunnerError> {
    if plan.is_empty() {
        return Err(invalid("a capability plan cannot be empty"));
    }
    let mut ids = BTreeSet::new();
    let mut covered = BTreeSet::new();
    for item in plan {
        if item.id.is_empty()
            || !item.id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
            || !ids.insert(item.id.as_str())
            || item.source_quote.trim().is_empty()
            || !source.contains(&item.source_quote)
            || item.outcome.trim().is_empty()
            || item.positive.trim().is_empty()
            || item.negative.trim().is_empty()
            || item.requirements.is_empty()
        {
            return Err(invalid(
                "capabilities need unique IDs, exact source anchors, outcomes, positive/failure criteria, and requirement mappings",
            ));
        }
        for requirement in &item.requirements {
            if !requirements.contains(requirement) {
                return Err(invalid(format!("unknown public requirement: {requirement}")));
            }
            covered.insert(requirement);
        }
    }
    if requirements.iter().any(|id| !covered.contains(id)) {
        return Err(invalid("the plan omits a public requirement"));
    }
    let mut reachable = BTreeSet::new();
    loop {
        let before = reachable.len();
        for item in plan {
            if item.depends_on.iter().all(|id| reachable.contains(id.as_str())) {
                reachable.insert(item.id.as_str());
            }
        }
        if reachable.len() == plan.len() {
            return Ok(());
        }
        if reachable.len() == before {
            return Err(invalid("capability dependencies contain a cycle or missing ID"));
        }
    }
}
