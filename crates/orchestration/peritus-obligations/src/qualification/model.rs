//! Input-defined qualification admission, lookup, evidence, and report model.

mod alternatives;
mod ordinary;

pub use alternatives::{
    alternative_groups, alternative_groups_after_push, branch_complete, branch_complete_member,
    branch_present, checked_branch_is_incomplete, checked_branches_after_push,
    checked_branches_sound, completed_group_count, group_accounting_after_push, group_complete,
    group_present, incomplete_groups, ordinary_group_partition_bound, same_branch,
    same_branch_preserves_completion, same_group,
};
pub use ordinary::{
    invalid_count, missing_count, ordinary_after_push, ordinary_after_push_dummy,
    ordinary_required_count, ordinary_satisfied_count, stale_count, unresolved_conditions,
    unsatisfied_requirements,
};

use super::{EvidenceVerdict, QualificationReport};
use crate::{
    ConditionId, ConditionObservation, ConditionState, ObligationSpec, PathMention,
    RequirementEntry, RequirementEvidence, RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use vstd::prelude::*;

verus! {

/// Every byte of one requirement identity.
pub open spec fn requirement_key(id: RequirementId) -> Seq<u8> {
    id.spec_digest().spec_bytes()@
}

/// Every byte of one condition identity.
pub open spec fn condition_key(id: ConditionId) -> Seq<u8> {
    id.spec_digest().spec_bytes()@
}

/// Condition identities in supplied order.
pub open spec fn condition_keys(conditions: Seq<ConditionObservation>) -> Seq<Seq<u8>> {
    conditions.map(|_index: int, value: ConditionObservation|
        condition_key(value.spec_condition_id()))
}

/// Evidence requirement identities in supplied order.
pub open spec fn evidence_keys(evidence: Seq<RequirementEvidence>) -> Seq<Seq<u8>> {
    evidence.map(|_index: int, value: RequirementEvidence|
        requirement_key(value.spec_binding().spec_requirement_id()))
}

/// Selects one adjacent comparison from canonical sequence ordering.
pub proof fn ordered_at(keys: Seq<Seq<u8>>, index: int)
    requires crate::order::ordered(keys), 1 <= index < keys.len(),
    ensures crate::order::byte_order(keys[index - 1], keys[index])
        == core::cmp::Ordering::Less,
{
}

/// A strictly ordered identity sequence has at most one matching position.
pub proof fn ordered_matching_index_unique(
    keys: Seq<Seq<u8>>,
    left: int,
    right: int,
)
    requires
        crate::order::ordered(keys),
        forall |index: int| 0 <= index < keys.len() ==>
            #[trigger] keys[index].len() == 32,
        0 <= left < keys.len(),
        0 <= right < keys.len(),
        keys[left] == keys[right],
    ensures left == right,
{
    crate::order::ordered_implies_unique(keys, 32);
    if left < right {
        assert(keys[left] != keys[right]);
    } else if right < left {
        assert(keys[right] != keys[left]);
    }
}

/// Whether the ledger contains one exact requirement identity.
pub open spec fn ledger_contains(entries: Seq<RequirementEntry>, id: RequirementId) -> bool {
    exists |index: int| 0 <= index < entries.len()
        && requirement_key(#[trigger] entries[index].spec_id()) == requirement_key(id)
}

/// Every evidence value names a requirement in the supplied ledger.
pub open spec fn all_evidence_known(
    ledger: &RequirementLedger,
    evidence: Seq<RequirementEvidence>,
) -> bool {
    forall |index: int| 0 <= index < evidence.len() ==>
        ledger_contains(
            ledger.spec_entries(),
            #[trigger] evidence[index].spec_binding().spec_requirement_id(),
        )
}

/// Complete input-defined qualification admission.
pub open spec fn qualification_inputs_valid(
    ledger: &RequirementLedger,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> bool {
    &&& crate::order::ordered(condition_keys(conditions))
    &&& evidence.len() <= ledger.spec_limits().spec_max_evidence()
    &&& crate::order::ordered(evidence_keys(evidence))
    &&& all_evidence_known(ledger, evidence)
}

/// Index of the first exact condition observation by identity.
pub open spec fn condition_index(
    conditions: Seq<ConditionObservation>,
    id: ConditionId,
) -> Option<int> {
    if exists |index: int| 0 <= index < conditions.len()
        && condition_key(#[trigger] conditions[index].spec_condition_id()) == condition_key(id)
    {
        Some(choose |index: int| 0 <= index < conditions.len()
            && condition_key(#[trigger] conditions[index].spec_condition_id()) == condition_key(id))
    } else {
        None
    }
}

/// Index of the first exact evidence value by requirement identity.
pub open spec fn evidence_index(
    evidence: Seq<RequirementEvidence>,
    id: RequirementId,
) -> Option<int> {
    if exists |index: int| 0 <= index < evidence.len()
        && requirement_key(#[trigger] evidence[index].spec_binding().spec_requirement_id())
            == requirement_key(id)
    {
        Some(choose |index: int| 0 <= index < evidence.len()
            && requirement_key(#[trigger] evidence[index].spec_binding().spec_requirement_id())
                == requirement_key(id))
    } else {
        None
    }
}

/// Every candidate-output path declared by an entry occurs in the evidence binding.
pub open spec fn required_paths_current(
    entry: &RequirementEntry,
    evidence: &RequirementEvidence,
) -> bool {
    forall |index: int| 0 <= index < entry.spec_paths().len()
        && #[trigger] entry.spec_paths()[index].spec_role().spec_requires_candidate_evidence() ==>
            evidence.spec_binding().spec_contains_path(entry.spec_paths()[index].spec_id())
}

/// Exact typed satisfaction for the actual obligation and evidence variants.
pub open spec fn typed_evidence_satisfied(
    entry: &RequirementEntry,
    evidence: &RequirementEvidence,
) -> bool {
    match (entry.spec_specification(), evidence) {
        (
            ObligationSpec::Hard
            | ObligationSpec::Conditional { .. }
            | ObligationSpec::Alternative { .. }
            | ObligationSpec::GeneratedOutput,
            RequirementEvidence::Direct(value),
        ) => value.spec_satisfied(),
        (ObligationSpec::Performance(requirement), RequirementEvidence::Performance(value)) =>
            value.spec_satisfies(requirement),
        (ObligationSpec::LifecycleIngress(requirement), RequirementEvidence::Lifecycle(value)) =>
            value.spec_satisfies(requirement),
        (
            ObligationSpec::RequestSchema(requirement)
            | ObligationSpec::ResponseSchema(requirement),
            RequirementEvidence::Schema(value),
        ) => value.spec_covers(&requirement),
        (ObligationSpec::BrowserSemantics(requirement), RequirementEvidence::Browser(value)) =>
            value.spec_satisfies(requirement),
        (
            ObligationSpec::ExternalEffect { effect_identity },
            RequirementEvidence::ExternalEffect(value),
        ) => value.spec_satisfies(effect_identity),
        (ObligationSpec::Example, _) => true,
        _ => false,
    }
}

/// Exact missing, stale, path-invalid, typed-invalid, or satisfied evidence verdict.
pub open spec fn evidence_verdict(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entry: &RequirementEntry,
    evidence: Option<RequirementEvidence>,
) -> EvidenceVerdict {
    match evidence {
        None => EvidenceVerdict::Missing,
        Some(value) => if !value.spec_binding().spec_is_current_for(
            entry.spec_id(), ledger.spec_digest(), candidate)
        {
            EvidenceVerdict::Stale
        } else if !required_paths_current(entry, &value) {
            EvidenceVerdict::Invalid
        } else if typed_evidence_satisfied(entry, &value) {
            EvidenceVerdict::Satisfied
        } else {
            EvidenceVerdict::Invalid
        },
    }
}

/// Verdict selected from the complete supplied evidence sequence.
pub open spec fn supplied_evidence_verdict(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entry: &RequirementEntry,
    evidence: Seq<RequirementEvidence>,
) -> EvidenceVerdict {
    match evidence_index(evidence, entry.spec_id()) {
        Some(index) => evidence_verdict(ledger, candidate, entry, Some(evidence[index])),
        None => EvidenceVerdict::Missing,
    }
}

/// Exact condition state selected from the supplied observations.
pub open spec fn supplied_condition_state(
    conditions: Seq<ConditionObservation>,
    id: ConditionId,
) -> Option<ConditionState> {
    match condition_index(conditions, id) {
        Some(index) => Some(conditions[index].spec_state()),
        None => None,
    }
}

/// Whether one ordinary entry is active and therefore requires evidence.
pub open spec fn ordinary_entry_active(
    entry: &RequirementEntry,
    conditions: Seq<ConditionObservation>,
) -> bool {
    if entry.spec_specification().spec_class() == crate::RequirementClass::Example
        || entry.spec_specification().spec_alternative().is_some()
    {
        false
    } else {
        match entry.spec_specification().spec_condition_id() {
            Some(id) => supplied_condition_state(conditions, id) == Some(ConditionState::Holds),
            None => true,
        }
    }
}

/// Unresolved active condition recorded for one ordinary entry, if any.
pub open spec fn entry_unresolved_condition(
    entry: &RequirementEntry,
    conditions: Seq<ConditionObservation>,
) -> Option<ConditionId> {
    if entry.spec_specification().spec_class() == crate::RequirementClass::Example
        || entry.spec_specification().spec_alternative().is_some()
    {
        None
    } else {
        match entry.spec_specification().spec_condition_id() {
            Some(id) => match supplied_condition_state(conditions, id) {
                Some(ConditionState::Unknown) | None => Some(id),
                _ => None,
            },
            None => None,
        }
    }
}

/// Complete input-defined successful qualification report.
pub open spec fn report_refines(
    report: &QualificationReport,
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> bool {
    let entries = ledger.spec_entries();
    let groups = alternative_groups(entries);
    let required = ordinary_required_count(entries, conditions) + groups.len();
    let satisfied = ordinary_satisfied_count(ledger, candidate, entries, conditions, evidence)
        + completed_group_count(ledger, candidate, entries, evidence, groups);
    let missing = missing_count(ledger, candidate, entries, conditions, evidence);
    let stale = stale_count(ledger, candidate, entries, conditions, evidence);
    let invalid = invalid_count(ledger, candidate, entries, conditions, evidence);
    let unsatisfied = unsatisfied_requirements(ledger, candidate, entries, conditions, evidence);
    let unresolved = unresolved_conditions(entries, conditions);
    let incomplete = incomplete_groups(ledger, candidate, entries, evidence, groups);
    &&& qualification_inputs_valid(ledger, conditions, evidence)
    &&& report.spec_required_count() == required
    &&& report.spec_satisfied_count() == satisfied
    &&& report.spec_missing_count() == missing
    &&& report.spec_stale_count() == stale
    &&& report.spec_invalid_count() == invalid
    &&& report.spec_unsatisfied_requirements() == unsatisfied
    &&& report.spec_unresolved_conditions() == unresolved
    &&& report.spec_incomplete_alternatives() == incomplete
    &&& report.spec_qualified() == (
        satisfied == required && missing == 0 && stale == 0 && invalid == 0
            && unresolved.len() == 0 && incomplete.len() == 0
    )
}

} // verus!
