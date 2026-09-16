//! Exact ordinary-entry counts and ordered diagnostic sequences.

use super::{entry_unresolved_condition, ordinary_entry_active, supplied_evidence_verdict};
use crate::{
    ConditionId, ConditionObservation, EvidenceVerdict, RequirementEntry, RequirementEvidence,
    RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use vstd::prelude::*;

verus! {

/// Number of active non-alternative obligations.
pub open spec fn ordinary_required_count(
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        ordinary_required_count(entries.drop_last(), conditions)
            + if ordinary_entry_active(&entries.last(), conditions) { 1nat } else { 0nat }
    }
}

/// Number of active ordinary obligations with satisfying current evidence.
pub open spec fn ordinary_satisfied_count(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        let entry = entries.last();
        ordinary_satisfied_count(ledger, candidate, entries.drop_last(), conditions, evidence)
            + if ordinary_entry_active(&entry, conditions)
                && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                    == EvidenceVerdict::Satisfied
            { 1nat } else { 0nat }
    }
}

/// Number of active ordinary obligations with no supplied evidence.
pub open spec fn missing_count(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        let entry = entries.last();
        missing_count(ledger, candidate, entries.drop_last(), conditions, evidence)
            + if ordinary_entry_active(&entry, conditions)
                && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                    == EvidenceVerdict::Missing
            { 1nat } else { 0nat }
    }
}

/// Number of active ordinary obligations with stale evidence.
pub open spec fn stale_count(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        let entry = entries.last();
        stale_count(ledger, candidate, entries.drop_last(), conditions, evidence)
            + if ordinary_entry_active(&entry, conditions)
                && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                    == EvidenceVerdict::Stale
            { 1nat } else { 0nat }
    }
}

/// Number of active ordinary obligations with current but invalid evidence.
pub open spec fn invalid_count(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> nat
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else {
        let entry = entries.last();
        invalid_count(ledger, candidate, entries.drop_last(), conditions, evidence)
            + if ordinary_entry_active(&entry, conditions)
                && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                    == EvidenceVerdict::Invalid
            { 1nat } else { 0nat }
    }
}

/// Active ordinary requirement identities whose verdict is not satisfied, in ledger order.
pub open spec fn unsatisfied_requirements(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> Seq<RequirementId>
    decreases entries.len(),
{
    if entries.len() == 0 {
        Seq::empty()
    } else {
        let entry = entries.last();
        let prior = unsatisfied_requirements(
            ledger, candidate, entries.drop_last(), conditions, evidence);
        if ordinary_entry_active(&entry, conditions)
            && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                != EvidenceVerdict::Satisfied
        {
            prior.push(entry.spec_id())
        } else {
            prior
        }
    }
}

/// Unresolved conditional identities in ledger-entry order, preserving repeated declarations.
pub open spec fn unresolved_conditions(
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
) -> Seq<ConditionId>
    decreases entries.len(),
{
    if entries.len() == 0 {
        Seq::empty()
    } else {
        let entry = entries.last();
        let prior = unresolved_conditions(entries.drop_last(), conditions);
        match entry_unresolved_condition(&entry, conditions) {
            Some(id) => prior.push(id),
            None => prior,
        }
    }
}

/// Appending one ledger entry gives the exact next ordinary report contribution.
pub proof fn ordinary_after_push(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    entry: RequirementEntry,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
)
    ensures
        ordinary_required_count(entries.push(entry), conditions)
            == ordinary_required_count(entries, conditions)
                + if ordinary_entry_active(&entry, conditions) { 1nat } else { 0nat },
        ordinary_satisfied_count(ledger, candidate, entries.push(entry), conditions, evidence)
            == ordinary_satisfied_count(ledger, candidate, entries, conditions, evidence)
                + if ordinary_entry_active(&entry, conditions)
                    && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                        == EvidenceVerdict::Satisfied
                { 1nat } else { 0nat },
        missing_count(ledger, candidate, entries.push(entry), conditions, evidence)
            == missing_count(ledger, candidate, entries, conditions, evidence)
                + if ordinary_entry_active(&entry, conditions)
                    && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                        == EvidenceVerdict::Missing
                { 1nat } else { 0nat },
        stale_count(ledger, candidate, entries.push(entry), conditions, evidence)
            == stale_count(ledger, candidate, entries, conditions, evidence)
                + if ordinary_entry_active(&entry, conditions)
                    && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                        == EvidenceVerdict::Stale
                { 1nat } else { 0nat },
        invalid_count(ledger, candidate, entries.push(entry), conditions, evidence)
            == invalid_count(ledger, candidate, entries, conditions, evidence)
                + if ordinary_entry_active(&entry, conditions)
                    && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                        == EvidenceVerdict::Invalid
                { 1nat } else { 0nat },
        unsatisfied_requirements(
            ledger, candidate, entries.push(entry), conditions, evidence)
            == if ordinary_entry_active(&entry, conditions)
                && supplied_evidence_verdict(ledger, candidate, &entry, evidence)
                    != EvidenceVerdict::Satisfied
            {
                unsatisfied_requirements(ledger, candidate, entries, conditions, evidence)
                    .push(entry.spec_id())
            } else {
                unsatisfied_requirements(ledger, candidate, entries, conditions, evidence)
            },
        unresolved_conditions(entries.push(entry), conditions)
            == match entry_unresolved_condition(&entry, conditions) {
                Some(id) => unresolved_conditions(entries, conditions).push(id),
                None => unresolved_conditions(entries, conditions),
            },
{
    assert(entries.push(entry).drop_last() == entries);
    assert(entries.push(entry).last() == entry);
}

/// Entry-local required-count update used by partition proofs.
pub proof fn ordinary_after_push_dummy(
    entries: Seq<RequirementEntry>,
    entry: RequirementEntry,
    conditions: Seq<ConditionObservation>,
)
    ensures ordinary_required_count(entries.push(entry), conditions)
        == ordinary_required_count(entries, conditions)
            + if ordinary_entry_active(&entry, conditions) { 1nat } else { 0nat },
{
    assert(entries.push(entry).drop_last() == entries);
    assert(entries.push(entry).last() == entry);
}

} // verus!
