//! Universal alternative-group semantics and exact report accounting.

use super::super::model;
use crate::{AlternativeGroupId, RequirementEntry, RequirementEvidence, RequirementLedger};
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Every listed group has one complete branch among actual ledger entries.
pub open spec fn groups_satisfied(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: Seq<RequirementEvidence>,
    groups: Seq<AlternativeGroupId>,
) -> bool {
    forall |index: int| 0 <= index < groups.len() ==>
        model::group_complete(ledger, candidate, ledger.spec_entries(), evidence,
            #[trigger] groups[index])
}

/// The group represented by each entry has one complete branch in the actual ledger.
pub open spec fn alternative_entries_satisfied(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: Seq<RequirementEvidence>,
    entries: Seq<RequirementEntry>,
) -> bool {
    forall |index: int| 0 <= index < entries.len() ==>
        match #[trigger] entries[index].spec_specification().spec_alternative() {
            Some((group, _branch)) =>
                model::group_complete(ledger, candidate, ledger.spec_entries(), evidence, group),
            None => true,
        }
}

proof fn matching_groups_preserve_completion(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: Seq<RequirementEvidence>,
    left: AlternativeGroupId,
    right: AlternativeGroupId,
)
    requires model::same_group(left, right),
    ensures model::group_complete(ledger, candidate, ledger.spec_entries(), evidence, left)
        == model::group_complete(ledger, candidate, ledger.spec_entries(), evidence, right),
{
    let entries = ledger.spec_entries();
    assert forall |index: int| 0 <= index < entries.len() implies
        match #[trigger] entries[index].spec_specification().spec_alternative() {
            Some((group, branch)) => {
                &&& model::same_group(group, left) == model::same_group(group, right)
                &&& model::branch_complete(ledger, candidate, entries, evidence, left, branch)
                    == model::branch_complete(ledger, candidate, entries, evidence, right, branch)
            },
            None => true,
        } by {
        if let Some((_group, _branch)) = entries[index].spec_specification().spec_alternative() {
            assert forall |member: int| 0 <= member < entries.len() implies
                match #[trigger] entries[member].spec_specification().spec_alternative() {
                    Some((member_group, _member_branch)) =>
                        model::same_group(member_group, left) == model::same_group(member_group, right),
                    None => true,
                } by {}
        }
    }
}

/// Complete and incomplete group accounting partitions the entire group sequence.
pub proof fn group_accounting_complete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: Seq<RequirementEvidence>,
    groups: Seq<AlternativeGroupId>,
)
    ensures
        model::completed_group_count(ledger, candidate, ledger.spec_entries(), evidence, groups)
            + model::incomplete_groups(ledger, candidate, ledger.spec_entries(), evidence, groups).len()
            == groups.len(),
        (model::completed_group_count(ledger, candidate, ledger.spec_entries(), evidence, groups)
            == groups.len()) == groups_satisfied(ledger, candidate, evidence, groups),
    decreases groups.len(),
{
    if groups.len() > 0 {
        let prior = groups.drop_last();
        let last = groups.last();
        group_accounting_complete(ledger, candidate, evidence, prior);
        assert(groups == prior.push(last));
        assert(groups_satisfied(ledger, candidate, evidence, groups)
            == (groups_satisfied(ledger, candidate, evidence, prior)
                && model::group_complete(ledger, candidate, ledger.spec_entries(), evidence, last))) by {
            if groups_satisfied(ledger, candidate, evidence, groups) {
                assert forall |index: int| 0 <= index < prior.len() implies
                    model::group_complete(ledger, candidate, ledger.spec_entries(), evidence,
                        #[trigger] prior[index]) by { assert(prior[index] == groups[index]); }
                assert(groups[groups.len() - 1] == last);
            } else if groups_satisfied(ledger, candidate, evidence, prior)
                && model::group_complete(ledger, candidate, ledger.spec_entries(), evidence, last)
            {
                assert forall |index: int| 0 <= index < groups.len() implies
                    model::group_complete(ledger, candidate, ledger.spec_entries(), evidence,
                        #[trigger] groups[index]) by {
                    if index < prior.len() { assert(groups[index] == prior[index]); }
                    else { assert(groups[index] == last); }
                }
            }
        }
    }
}

/// First-seen group enumeration covers exactly the groups represented by input entries.
pub proof fn group_enumeration_complete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: Seq<RequirementEvidence>,
    entries: Seq<RequirementEntry>,
)
    ensures groups_satisfied(ledger, candidate, evidence, model::alternative_groups(entries))
        == alternative_entries_satisfied(ledger, candidate, evidence, entries),
    decreases entries.len(),
{
    if entries.len() > 0 {
        let prior = entries.drop_last();
        let last = entries.last();
        let prior_groups = model::alternative_groups(prior);
        group_enumeration_complete(ledger, candidate, evidence, prior);
        assert(entries == prior.push(last));
        model::alternative_groups_after_push(prior, last);
        assert(alternative_entries_satisfied(ledger, candidate, evidence, entries)
            == (alternative_entries_satisfied(ledger, candidate, evidence, prior)
                && match last.spec_specification().spec_alternative() {
                    Some((group, _branch)) => model::group_complete(
                        ledger, candidate, ledger.spec_entries(), evidence, group),
                    None => true,
                })) by {
            if alternative_entries_satisfied(ledger, candidate, evidence, entries) {
                assert forall |index: int| 0 <= index < prior.len() implies
                    match #[trigger] prior[index].spec_specification().spec_alternative() {
                        Some((group, _branch)) => model::group_complete(
                            ledger, candidate, ledger.spec_entries(), evidence, group),
                        None => true,
                    } by { assert(prior[index] == entries[index]); }
                assert(entries[entries.len() - 1] == last);
            } else if alternative_entries_satisfied(ledger, candidate, evidence, prior) {
                assert forall |index: int| 0 <= index < entries.len() implies
                    match #[trigger] entries[index].spec_specification().spec_alternative() {
                        Some((group, _branch)) => model::group_complete(
                            ledger, candidate, ledger.spec_entries(), evidence, group),
                        None => true,
                    } || index == entries.len() - 1 by {
                    if index < prior.len() { assert(entries[index] == prior[index]); }
                }
            }
        }
        if let Some((group, _branch)) = last.spec_specification().spec_alternative() {
            if model::group_present(prior_groups, group) {
                if groups_satisfied(ledger, candidate, evidence, prior_groups) {
                    let index = choose |index: int| 0 <= index < prior_groups.len()
                        && model::same_group(#[trigger] prior_groups[index], group);
                    matching_groups_preserve_completion(
                        ledger, candidate, evidence, prior_groups[index], group);
                }
            } else {
                let groups = prior_groups.push(group);
                assert(groups_satisfied(ledger, candidate, evidence, groups)
                    == (groups_satisfied(ledger, candidate, evidence, prior_groups)
                        && model::group_complete(ledger, candidate, ledger.spec_entries(), evidence, group))) by {
                    if groups_satisfied(ledger, candidate, evidence, groups) {
                        assert forall |index: int| 0 <= index < prior_groups.len() implies
                            model::group_complete(ledger, candidate, ledger.spec_entries(), evidence,
                                #[trigger] prior_groups[index]) by {
                            assert(prior_groups[index] == groups[index]);
                        }
                        assert(groups[groups.len() - 1] == group);
                    } else if groups_satisfied(ledger, candidate, evidence, prior_groups)
                        && model::group_complete(ledger, candidate, ledger.spec_entries(), evidence, group)
                    {
                        assert forall |index: int| 0 <= index < groups.len() implies
                            model::group_complete(ledger, candidate, ledger.spec_entries(), evidence,
                                #[trigger] groups[index]) by {
                            if index < prior_groups.len() { assert(groups[index] == prior_groups[index]); }
                            else { assert(groups[index] == group); }
                        }
                    }
                }
            }
        }
    }
}

} // verus!
