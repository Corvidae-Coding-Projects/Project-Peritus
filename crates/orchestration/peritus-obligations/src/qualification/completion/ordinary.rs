//! Universal ordinary-obligation semantics derived from exact report accounting.

use super::super::model;
use crate::{
    ConditionObservation, EvidenceVerdict, RequirementEntry, RequirementEvidence, RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Every active ordinary entry has satisfying current typed evidence.
pub open spec fn active_entries_satisfied(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> bool {
    forall |index: int| #![trigger entries[index]] 0 <= index < entries.len()
        && model::ordinary_entry_active(&entries[index], conditions) ==>
        model::supplied_evidence_verdict(ledger, candidate, &entries[index], evidence)
            == EvidenceVerdict::Satisfied
}

/// Every condition consulted by an ordinary entry has a resolved observation.
pub open spec fn all_conditions_resolved(
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
) -> bool {
    forall |index: int| #![trigger entries[index]] 0 <= index < entries.len() ==>
        model::entry_unresolved_condition(&entries[index], conditions).is_none()
}

/// Exact count partition and empty unresolved list are equivalent to universal entry conditions.
pub proof fn ordinary_accounting_complete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
)
    ensures
        model::ordinary_required_count(entries, conditions)
            == model::ordinary_satisfied_count(ledger, candidate, entries, conditions, evidence)
                + model::missing_count(ledger, candidate, entries, conditions, evidence)
                + model::stale_count(ledger, candidate, entries, conditions, evidence)
                + model::invalid_count(ledger, candidate, entries, conditions, evidence),
        (model::ordinary_required_count(entries, conditions)
            == model::ordinary_satisfied_count(ledger, candidate, entries, conditions, evidence))
            == active_entries_satisfied(ledger, candidate, entries, conditions, evidence),
        (model::unresolved_conditions(entries, conditions).len() == 0)
            == all_conditions_resolved(entries, conditions),
    decreases entries.len(),
{
    if entries.len() > 0 {
        let prior = entries.drop_last();
        let last = entries.last();
        ordinary_accounting_complete(ledger, candidate, prior, conditions, evidence);
        assert(entries == prior.push(last));
        model::ordinary_after_push(ledger, candidate, prior, last, conditions, evidence);
        assert(active_entries_satisfied(ledger, candidate, entries, conditions, evidence)
            == (active_entries_satisfied(ledger, candidate, prior, conditions, evidence)
                && (model::ordinary_entry_active(&last, conditions) ==>
                    model::supplied_evidence_verdict(ledger, candidate, &last, evidence)
                        == EvidenceVerdict::Satisfied))) by {
            if active_entries_satisfied(ledger, candidate, entries, conditions, evidence) {
                assert forall |index: int| #![trigger prior[index]] 0 <= index < prior.len()
                    && model::ordinary_entry_active(&prior[index], conditions) implies
                    model::supplied_evidence_verdict(ledger, candidate, &prior[index], evidence)
                        == EvidenceVerdict::Satisfied by {
                    assert(prior[index] == entries[index]);
                }
                assert(entries[entries.len() - 1] == last);
            } else if active_entries_satisfied(ledger, candidate, prior, conditions, evidence)
                && (model::ordinary_entry_active(&last, conditions) ==>
                    model::supplied_evidence_verdict(ledger, candidate, &last, evidence)
                        == EvidenceVerdict::Satisfied)
            {
                assert forall |index: int| #![trigger entries[index]] 0 <= index < entries.len()
                    && model::ordinary_entry_active(&entries[index], conditions) implies
                    model::supplied_evidence_verdict(ledger, candidate, &entries[index], evidence)
                        == EvidenceVerdict::Satisfied by {
                    if index < prior.len() { assert(entries[index] == prior[index]); }
                    else { assert(entries[index] == last); }
                }
            }
        }
        assert(all_conditions_resolved(entries, conditions)
            == (all_conditions_resolved(prior, conditions)
                && model::entry_unresolved_condition(&last, conditions).is_none())) by {
            if all_conditions_resolved(entries, conditions) {
                assert forall |index: int| #![trigger prior[index]] 0 <= index < prior.len() implies
                    model::entry_unresolved_condition(&prior[index], conditions).is_none()
                    by { assert(prior[index] == entries[index]); }
                assert(entries[entries.len() - 1] == last);
            } else if all_conditions_resolved(prior, conditions)
                && model::entry_unresolved_condition(&last, conditions).is_none()
            {
                assert forall |index: int| #![trigger entries[index]] 0 <= index < entries.len() implies
                    model::entry_unresolved_condition(&entries[index], conditions).is_none()
                    by {
                    if index < prior.len() { assert(entries[index] == prior[index]); }
                    else { assert(entries[index] == last); }
                }
            }
        }
    }
}

} // verus!
