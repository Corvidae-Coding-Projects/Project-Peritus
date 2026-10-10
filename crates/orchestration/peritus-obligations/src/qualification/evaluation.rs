//! Verified ordinary and alternative qualification traversal.

mod alternatives;

use super::{EvidenceVerdict, QualificationReport, verdict};
use crate::{
    ConditionId, ConditionObservation, ConditionState, RequirementEntry, RequirementEvidence,
    RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;

enum OrdinaryAction {
    Ignored,
    Unresolved(ConditionId),
    Active,
}
fn ordinary_action(
    entry: &RequirementEntry,
    conditions: &[ConditionObservation],
) -> OrdinaryAction {
    if entry.specification().is_example() || entry.specification().alternative().is_some() {
        return OrdinaryAction::Ignored;
    }
    if let Some(condition_id) = entry.specification().condition_id() {
        return match verdict::condition_state(conditions, condition_id) {
            Some(ConditionState::DoesNotHold) => OrdinaryAction::Ignored,
            Some(ConditionState::Holds) => OrdinaryAction::Active,
            Some(ConditionState::Unknown) | None => OrdinaryAction::Unresolved(condition_id),
        };
    }
    OrdinaryAction::Active
}
pub(super) fn evaluate(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    conditions: &[ConditionObservation],
    evidence: &[RequirementEvidence],
) -> QualificationReport {
    let entries = ledger.entries();
    let mut report = QualificationReport {
        qualified: false,
        required_count: 0,
        satisfied_count: 0,
        missing_count: 0,
        stale_count: 0,
        invalid_count: 0,
        unsatisfied_requirements: Vec::new(),
        unresolved_conditions: Vec::new(),
        incomplete_alternatives: Vec::new(),
    };
    let mut index = 0;
    while index < entries.len() {
        let entry = &entries[index];
        match ordinary_action(entry, conditions) {
            OrdinaryAction::Ignored => {}
            OrdinaryAction::Unresolved(condition_id) => {
                report.unresolved_conditions.push(condition_id);
            }
            OrdinaryAction::Active => {
                report.required_count += 1;
                let evidence_verdict =
                    verdict::evidence_verdict(ledger, candidate, entry, evidence);
                match evidence_verdict {
                    EvidenceVerdict::Satisfied => report.satisfied_count += 1,
                    EvidenceVerdict::Missing => {
                        report.missing_count += 1;
                        report.unsatisfied_requirements.push(entry.id());
                    }
                    EvidenceVerdict::Stale => {
                        report.stale_count += 1;
                        report.unsatisfied_requirements.push(entry.id());
                    }
                    EvidenceVerdict::Invalid => {
                        report.invalid_count += 1;
                        report.unsatisfied_requirements.push(entry.id());
                    }
                }
            }
        }
        index += 1;
    }
    let alternative = alternatives::evaluate(ledger, candidate, evidence);
    report.required_count += alternative.0;
    report.satisfied_count += alternative.1;
    report.incomplete_alternatives = alternative.2;
    let required_current = report.satisfied_count == report.required_count
        && report.missing_count == 0
        && report.stale_count == 0
        && report.invalid_count == 0;
    report.qualified = crate::verified::qualification_allowed(
        required_current,
        report.incomplete_alternatives.is_empty(),
        report.unresolved_conditions.is_empty(),
    );
    report
}
