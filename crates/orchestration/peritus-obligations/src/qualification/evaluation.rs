//! Verified ordinary and alternative qualification traversal.

mod alternatives;

#[cfg(verus_only)]
use super::model;
use super::{EvidenceVerdict, QualificationReport, verdict};
use crate::{
    ConditionId, ConditionObservation, ConditionState, RequirementEntry, RequirementEvidence,
    RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

enum OrdinaryAction {
    Ignored,
    Unresolved(ConditionId),
    Active,
}

fn ordinary_action(
    entry: &RequirementEntry,
    conditions: &[ConditionObservation],
) -> (action: OrdinaryAction)
    requires crate::order::ordered(model::condition_keys(conditions@)),
    ensures match action {
        OrdinaryAction::Ignored => !model::ordinary_entry_active(entry, conditions@)
            && model::entry_unresolved_condition(entry, conditions@).is_none(),
        OrdinaryAction::Unresolved(id) => !model::ordinary_entry_active(entry, conditions@)
            && model::entry_unresolved_condition(entry, conditions@) == Some(id),
        OrdinaryAction::Active => model::ordinary_entry_active(entry, conditions@)
            && model::entry_unresolved_condition(entry, conditions@).is_none(),
    },
{
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
) -> (report: QualificationReport)
    requires model::qualification_inputs_valid(ledger, conditions@, evidence@),
    ensures model::report_refines(&report, ledger, candidate, conditions@, evidence@),
{
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
    while index < entries.len()
        invariant
            entries@ == ledger.spec_entries(),
            index <= entries@.len(),
            crate::order::ordered(model::condition_keys(conditions@)),
            crate::order::ordered(model::evidence_keys(evidence@)),
            report.required_count as nat == model::ordinary_required_count(
                entries@.take(index as int), conditions@),
            report.satisfied_count as nat == model::ordinary_satisfied_count(
                ledger, candidate, entries@.take(index as int), conditions@, evidence@),
            report.missing_count as nat == model::missing_count(
                ledger, candidate, entries@.take(index as int), conditions@, evidence@),
            report.stale_count as nat == model::stale_count(
                ledger, candidate, entries@.take(index as int), conditions@, evidence@),
            report.invalid_count as nat == model::invalid_count(
                ledger, candidate, entries@.take(index as int), conditions@, evidence@),
            report.unsatisfied_requirements@ == model::unsatisfied_requirements(
                ledger, candidate, entries@.take(index as int), conditions@, evidence@),
            report.unresolved_conditions@ == model::unresolved_conditions(
                entries@.take(index as int), conditions@),
            report.incomplete_alternatives@.len() == 0,
            report.required_count <= index,
            report.satisfied_count <= report.required_count,
            report.missing_count <= report.required_count,
            report.stale_count <= report.required_count,
            report.invalid_count <= report.required_count,
        decreases entries.len() - index,
    {
        let entry = &entries[index];
        let ghost prefix = entries@.take(index as int);
        match ordinary_action(entry, conditions) {
            OrdinaryAction::Ignored => {},
            OrdinaryAction::Unresolved(condition_id) => {
                report.unresolved_conditions.push(condition_id);
            },
            OrdinaryAction::Active => {
                report.required_count += 1;
                let evidence_verdict =
                    verdict::evidence_verdict(ledger, candidate, entry, evidence);
                match evidence_verdict {
                    EvidenceVerdict::Satisfied => report.satisfied_count += 1,
                    EvidenceVerdict::Missing => {
                        report.missing_count += 1;
                        report.unsatisfied_requirements.push(entry.id());
                    },
                    EvidenceVerdict::Stale => {
                        report.stale_count += 1;
                        report.unsatisfied_requirements.push(entry.id());
                    },
                    EvidenceVerdict::Invalid => {
                        report.invalid_count += 1;
                        report.unsatisfied_requirements.push(entry.id());
                    },
                }
            },
        }
        proof {
            assert(entries@.take(index as int + 1) == prefix.push(*entry));
            model::ordinary_after_push(
                ledger, candidate, prefix, *entry, conditions@, evidence@);
        }
        index += 1;
    }

    assert(entries@.take(index as int) =~= entries@);

    let alternative = alternatives::evaluate(ledger, candidate, evidence);
    proof {
        model::ordinary_group_partition_bound(ledger.spec_entries(), conditions@);
        assert(report.required_count + alternative.0 <= entries.len());
        assert(report.satisfied_count + alternative.1
            <= report.required_count + alternative.0);
    }
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

} // verus!
