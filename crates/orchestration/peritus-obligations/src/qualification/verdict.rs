//! Verified lookups, required-path checks, and typed evidence verdicts.

use super::EvidenceVerdict;

use crate::{
    ConditionId, ConditionObservation, ConditionState, ObligationSpec, RequirementEntry,
    RequirementEvidence, RequirementLedger,
};
use core::cmp::Ordering;
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;

pub(super) fn condition_state(
    conditions: &[ConditionObservation],
    id: ConditionId,
) -> Option<ConditionState> {
    let mut low = 0usize;
    let mut high = conditions.len();
    while low < high {
        let middle = low + (high - low) / 2;
        let order = crate::order::compare(
            conditions[middle].condition_id().digest().as_bytes(),
            id.digest().as_bytes(),
        );
        match order {
            Ordering::Equal => {
                return Some(conditions[middle].state());
            }
            Ordering::Less => {
                low = middle + 1;
            }
            Ordering::Greater => {
                high = middle;
            }
        }
    }
    None
}
fn evidence_index(evidence: &[RequirementEvidence], id: RequirementId) -> Option<usize> {
    let mut low = 0usize;
    let mut high = evidence.len();
    while low < high {
        let middle = low + (high - low) / 2;
        let order = crate::order::compare(
            evidence[middle].requirement_id().digest().as_bytes(),
            id.digest().as_bytes(),
        );
        match order {
            Ordering::Equal => {
                return Some(middle);
            }
            Ordering::Less => {
                low = middle + 1;
            }
            Ordering::Greater => {
                high = middle;
            }
        }
    }
    None
}
fn required_paths_current(entry: &RequirementEntry, evidence: &RequirementEvidence) -> bool {
    let paths = entry.paths();
    let mut index = 0;
    while index < paths.len() {
        if paths[index].role().requires_candidate_evidence()
            && !evidence.binding().contains_path(paths[index].id())
        {
            return false;
        }
        index += 1;
    }
    true
}
fn typed_evidence_satisfied(entry: &RequirementEntry, evidence: &RequirementEvidence) -> bool {
    match (entry.specification(), evidence) {
        (
            ObligationSpec::Hard
            | ObligationSpec::Conditional { .. }
            | ObligationSpec::Alternative { .. }
            | ObligationSpec::GeneratedOutput,
            RequirementEvidence::Direct(value),
        ) => value.satisfied(),
        (ObligationSpec::Performance(requirement), RequirementEvidence::Performance(value)) => {
            value.satisfies(*requirement)
        }
        (ObligationSpec::LifecycleIngress(requirement), RequirementEvidence::Lifecycle(value)) => {
            value.satisfies(*requirement)
        }
        (
            ObligationSpec::RequestSchema(requirement)
            | ObligationSpec::ResponseSchema(requirement),
            RequirementEvidence::Schema(value),
        ) => value.covers(requirement),
        (ObligationSpec::BrowserSemantics(requirement), RequirementEvidence::Browser(value)) => {
            value.satisfies(*requirement)
        }
        (
            ObligationSpec::ExternalEffect { effect_identity },
            RequirementEvidence::ExternalEffect(value),
        ) => value.satisfies(*effect_identity),
        (ObligationSpec::Example, _) => true,
        _ => false,
    }
}
fn verdict_for_value(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entry: &RequirementEntry,
    evidence: &RequirementEvidence,
) -> EvidenceVerdict {
    if !evidence.binding().is_current_for(entry.id(), ledger.digest(), candidate) {
        return EvidenceVerdict::Stale;
    }
    if !required_paths_current(entry, evidence) {
        return EvidenceVerdict::Invalid;
    }
    if typed_evidence_satisfied(entry, evidence) {
        EvidenceVerdict::Satisfied
    } else {
        EvidenceVerdict::Invalid
    }
}
pub(super) fn evidence_verdict(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entry: &RequirementEntry,
    evidence: &[RequirementEvidence],
) -> EvidenceVerdict {
    let Some(index) = evidence_index(evidence, entry.id()) else {
        return EvidenceVerdict::Missing;
    };
    verdict_for_value(ledger, candidate, entry, &evidence[index])
}
