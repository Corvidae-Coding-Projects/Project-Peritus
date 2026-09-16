//! Verified lookups, required-path checks, and typed evidence verdicts.

use super::EvidenceVerdict;
#[cfg(verus_only)]
use super::model;
use crate::{
    ConditionId, ConditionObservation, ConditionState, ObligationSpec, RequirementEntry,
    RequirementEvidence, RequirementLedger,
};
use core::cmp::Ordering;
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use vstd::prelude::*;

verus! {

pub(super) fn condition_state(
    conditions: &[ConditionObservation],
    id: ConditionId,
) -> (state: Option<ConditionState>)
    requires crate::order::ordered(model::condition_keys(conditions@)),
    ensures state == model::supplied_condition_state(conditions@, id),
{
    let mut low = 0usize;
    let mut high = conditions.len();
    while low < high
        invariant
            low <= high <= conditions@.len(),
            crate::order::ordered(model::condition_keys(conditions@)),
            forall |key_index: int| 0 <= key_index < conditions@.len() ==>
                #[trigger] model::condition_keys(conditions@)[key_index].len() == 32,
            forall |prior: int| 0 <= prior < low ==>
                model::condition_key(#[trigger] conditions@[prior].spec_condition_id())
                    != model::condition_key(id),
            forall |later: int| high <= later < conditions@.len() ==>
                model::condition_key(#[trigger] conditions@[later].spec_condition_id())
                    != model::condition_key(id),
        decreases high - low,
    {
        let middle = low + (high - low) / 2;
        assert(low <= middle < high);
        let order = crate::order::compare(
            conditions[middle].condition_id().digest().as_bytes(),
            id.digest().as_bytes(),
        );
        match order {
            Ordering::Equal => {
                proof {
                    let selected = choose |selected: int| 0 <= selected < conditions@.len()
                        && model::condition_key(
                            #[trigger] conditions@[selected].spec_condition_id())
                            == model::condition_key(id);
                    assert(model::condition_key(conditions@[selected].spec_condition_id())
                        == model::condition_key(
                            conditions@[middle as int].spec_condition_id()));
                    model::ordered_matching_index_unique(
                        model::condition_keys(conditions@), selected, middle as int);
                    assert(model::condition_index(conditions@, id) == Some(middle as int));
                }
                return Some(conditions[middle].state());
            },
            Ordering::Less => {
                proof {
                    crate::order::less_eliminates_through(
                        model::condition_keys(conditions@),
                        model::condition_key(id),
                        32,
                        middle as int,
                    );
                    assert forall |prior: int| 0 <= prior <= middle implies
                        model::condition_key(
                            #[trigger] conditions@[prior].spec_condition_id())
                            != model::condition_key(id) by {
                        assert(model::condition_keys(conditions@)[prior]
                            == model::condition_key(
                                conditions@[prior].spec_condition_id()));
                    }
                }
                low = middle + 1;
            },
            Ordering::Greater => {
                proof {
                    crate::order::greater_eliminates_from(
                        model::condition_keys(conditions@),
                        model::condition_key(id),
                        32,
                        middle as int,
                    );
                    assert forall |later: int| middle <= later < conditions@.len() implies
                        model::condition_key(
                            #[trigger] conditions@[later].spec_condition_id())
                            != model::condition_key(id) by {
                        assert(model::condition_keys(conditions@)[later]
                            == model::condition_key(
                                conditions@[later].spec_condition_id()));
                    }
                }
                high = middle;
            },
        }
    }
    assert(model::condition_index(conditions@, id).is_none());
    None
}

fn evidence_index(
    evidence: &[RequirementEvidence],
    id: RequirementId,
) -> (result: Option<usize>)
    requires crate::order::ordered(model::evidence_keys(evidence@)),
    ensures match result {
        Some(index) => model::evidence_index(evidence@, id) == Some(index as int),
        None => model::evidence_index(evidence@, id).is_none(),
    },
{
    let mut low = 0usize;
    let mut high = evidence.len();
    while low < high
        invariant
            low <= high <= evidence@.len(),
            crate::order::ordered(model::evidence_keys(evidence@)),
            forall |key_index: int| 0 <= key_index < evidence@.len() ==>
                #[trigger] model::evidence_keys(evidence@)[key_index].len() == 32,
            forall |prior: int| 0 <= prior < low ==>
                model::requirement_key(
                    #[trigger] evidence@[prior].spec_binding().spec_requirement_id())
                    != model::requirement_key(id),
            forall |later: int| high <= later < evidence@.len() ==>
                model::requirement_key(
                    #[trigger] evidence@[later].spec_binding().spec_requirement_id())
                    != model::requirement_key(id),
        decreases high - low,
    {
        let middle = low + (high - low) / 2;
        assert(low <= middle < high);
        let order = crate::order::compare(
            evidence[middle].requirement_id().digest().as_bytes(),
            id.digest().as_bytes(),
        );
        match order {
            Ordering::Equal => {
                proof {
                    let selected = choose |selected: int| 0 <= selected < evidence@.len()
                        && model::requirement_key(
                            #[trigger] evidence@[selected].spec_binding().spec_requirement_id())
                            == model::requirement_key(id);
                    assert(model::requirement_key(
                        evidence@[selected].spec_binding().spec_requirement_id())
                        == model::requirement_key(
                            evidence@[middle as int].spec_binding().spec_requirement_id()));
                    model::ordered_matching_index_unique(
                        model::evidence_keys(evidence@), selected, middle as int);
                    assert(model::evidence_index(evidence@, id) == Some(middle as int));
                }
                return Some(middle);
            },
            Ordering::Less => {
                proof {
                    crate::order::less_eliminates_through(
                        model::evidence_keys(evidence@),
                        model::requirement_key(id),
                        32,
                        middle as int,
                    );
                    assert forall |prior: int| 0 <= prior <= middle implies
                        model::requirement_key(
                            #[trigger] evidence@[prior]
                                .spec_binding().spec_requirement_id())
                            != model::requirement_key(id) by {
                        assert(model::evidence_keys(evidence@)[prior]
                            == model::requirement_key(
                                evidence@[prior].spec_binding().spec_requirement_id()));
                    }
                }
                low = middle + 1;
            },
            Ordering::Greater => {
                proof {
                    crate::order::greater_eliminates_from(
                        model::evidence_keys(evidence@),
                        model::requirement_key(id),
                        32,
                        middle as int,
                    );
                    assert forall |later: int| middle <= later < evidence@.len() implies
                        model::requirement_key(
                            #[trigger] evidence@[later]
                                .spec_binding().spec_requirement_id())
                            != model::requirement_key(id) by {
                        assert(model::evidence_keys(evidence@)[later]
                            == model::requirement_key(
                                evidence@[later].spec_binding().spec_requirement_id()));
                    }
                }
                high = middle;
            },
        }
    }
    assert(model::evidence_index(evidence@, id).is_none());
    None
}

fn required_paths_current(
    entry: &RequirementEntry,
    evidence: &RequirementEvidence,
) -> (current: bool)
    ensures current == model::required_paths_current(entry, evidence),
{
    let paths = entry.paths();
    let mut index = 0;
    while index < paths.len()
        invariant
            paths@ == entry.spec_paths(),
            index <= paths@.len(),
            forall |prior: int| 0 <= prior < index
                && #[trigger] paths@[prior].spec_role().spec_requires_candidate_evidence() ==>
                    evidence.spec_binding().spec_contains_path(paths@[prior].spec_id()),
        decreases paths.len() - index,
    {
        if paths[index].role().requires_candidate_evidence()
            && !evidence.binding().contains_path(paths[index].id())
        {
            assert(!model::required_paths_current(entry, evidence)) by {
                if model::required_paths_current(entry, evidence) {
                    assert(evidence.spec_binding().spec_contains_path(paths@[index as int].spec_id()));
                }
            }
            return false;
        }
        index += 1;
    }
    true
}

fn typed_evidence_satisfied(
    entry: &RequirementEntry,
    evidence: &RequirementEvidence,
) -> (satisfied: bool)
    ensures satisfied == model::typed_evidence_satisfied(entry, evidence),
{
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
        },
        (ObligationSpec::LifecycleIngress(requirement), RequirementEvidence::Lifecycle(value)) => {
            value.satisfies(*requirement)
        },
        (
            ObligationSpec::RequestSchema(requirement)
            | ObligationSpec::ResponseSchema(requirement),
            RequirementEvidence::Schema(value),
        ) => value.covers(requirement),
        (ObligationSpec::BrowserSemantics(requirement), RequirementEvidence::Browser(value)) => {
            value.satisfies(*requirement)
        },
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
) -> (verdict: EvidenceVerdict)
    ensures verdict == model::evidence_verdict(ledger, candidate, entry, Some(*evidence)),
{
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
) -> (verdict: EvidenceVerdict)
    requires crate::order::ordered(model::evidence_keys(evidence@)),
    ensures verdict == model::supplied_evidence_verdict(ledger, candidate, entry, evidence@),
{
    let Some(index) = evidence_index(evidence, entry.id()) else {
        return EvidenceVerdict::Missing;
    };
    verdict_for_value(ledger, candidate, entry, &evidence[index])
}

} // verus!
