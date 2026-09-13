//! Exact first-group order and complete-branch qualification model.

use super::supplied_evidence_verdict;
use crate::{
    AlternativeBranchId, AlternativeGroupId, ConditionObservation, RequirementEntry,
    RequirementEvidence, RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Exact alternative-group identity equality.
pub open spec fn same_group(left: AlternativeGroupId, right: AlternativeGroupId) -> bool {
    left.spec_digest().spec_bytes()@ == right.spec_digest().spec_bytes()@
}

/// Exact alternative-branch identity equality.
pub open spec fn same_branch(left: AlternativeBranchId, right: AlternativeBranchId) -> bool {
    left.spec_digest().spec_bytes()@ == right.spec_digest().spec_bytes()@
}

/// Whether an exact branch identity already occurs in a checked branch sequence.
pub open spec fn branch_present(
    branches: Seq<AlternativeBranchId>,
    branch: AlternativeBranchId,
) -> bool {
    exists |index: int| 0 <= index < branches.len()
        && same_branch(#[trigger] branches[index], branch)
}

/// Every retained checked branch came from an earlier entry of the selected group.
pub open spec fn checked_branches_sound(
    entries: Seq<RequirementEntry>,
    end: int,
    group: AlternativeGroupId,
    branches: Seq<AlternativeBranchId>,
) -> bool {
    forall |checked: int| #![trigger branches[checked]] 0 <= checked < branches.len()
        ==> exists |entry: int|
        0 <= entry < end
            && match #[trigger] entries[entry].spec_specification().spec_alternative() {
                Some((entry_group, entry_branch)) => same_group(entry_group, group)
                    && same_branch(entry_branch, branches[checked]),
                None => false,
            }
}

/// A previously evaluated alias of a checked branch retains its incomplete result.
pub proof fn checked_branch_is_incomplete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    evidence: Seq<RequirementEvidence>,
    end: int,
    group: AlternativeGroupId,
    checked_branches: Seq<AlternativeBranchId>,
    branch: AlternativeBranchId,
)
    requires
        0 <= end <= entries.len(),
        checked_branches_sound(entries, end, group, checked_branches),
        branch_present(checked_branches, branch),
        forall |prior: int| 0 <= prior < end ==>
            match #[trigger] entries[prior].spec_specification().spec_alternative() {
                Some((entry_group, prior_branch)) if same_group(entry_group, group) =>
                    !branch_complete(ledger, candidate, entries, evidence, group, prior_branch),
                _ => true,
            },
    ensures !branch_complete(ledger, candidate, entries, evidence, group, branch),
{
    let checked = choose |checked: int| 0 <= checked < checked_branches.len()
        && same_branch(#[trigger] checked_branches[checked], branch);
    let origin = choose |origin: int| 0 <= origin < end
        && match #[trigger] entries[origin].spec_specification().spec_alternative() {
            Some((origin_group, origin_branch)) => same_group(origin_group, group)
                && same_branch(origin_branch, checked_branches[checked]),
            None => false,
        };
    let origin_alternative = entries[origin].spec_specification().spec_alternative();
    assert(origin_alternative.is_some());
    let origin_branch = origin_alternative.unwrap().1;
    assert(same_branch(origin_branch, branch));
    same_branch_preserves_completion(
        ledger, candidate, entries, evidence, group, origin_branch, branch,
    );
}

/// Recording the current branch preserves checked-branch origin correspondence.
pub proof fn checked_branches_after_push(
    entries: Seq<RequirementEntry>,
    index: int,
    group: AlternativeGroupId,
    prior: Seq<AlternativeBranchId>,
    branch: AlternativeBranchId,
)
    requires
        0 <= index < entries.len(),
        checked_branches_sound(entries, index, group, prior),
        match entries[index].spec_specification().spec_alternative() {
            Some((entry_group, entry_branch)) => same_group(entry_group, group)
                && same_branch(entry_branch, branch),
            None => false,
        },
    ensures checked_branches_sound(entries, index + 1, group, prior.push(branch)),
{
    assert forall |checked: int| #![trigger prior.push(branch)[checked]]
        0 <= checked < prior.push(branch).len()
        implies exists |origin: int| 0 <= origin < index + 1
            && match #[trigger] entries[origin].spec_specification().spec_alternative() {
                Some((origin_group, origin_branch)) => same_group(origin_group, group)
                    && same_branch(origin_branch, prior.push(branch)[checked]),
                None => false,
            } by {
        if checked < prior.len() {
            let origin = choose |origin: int| 0 <= origin < index
                && match #[trigger] entries[origin].spec_specification().spec_alternative() {
                    Some((origin_group, origin_branch)) => same_group(origin_group, group)
                        && same_branch(origin_branch, prior[checked]),
                    None => false,
                };
            assert(prior.push(branch)[checked] == prior[checked]);
        } else {
            assert(checked == prior.len());
            assert(prior.push(branch)[checked] == branch);
            let origin = index;
        }
    }
}

/// Whether a group already occurs in a first-seen group sequence.
pub open spec fn group_present(groups: Seq<AlternativeGroupId>, group: AlternativeGroupId) -> bool {
    exists |index: int| 0 <= index < groups.len()
        && same_group(#[trigger] groups[index], group)
}

/// Alternative groups in order of their first ledger occurrence.
pub open spec fn alternative_groups(
    entries: Seq<RequirementEntry>,
) -> Seq<AlternativeGroupId>
    decreases entries.len(),
{
    if entries.len() == 0 {
        Seq::empty()
    } else {
        let prior = alternative_groups(entries.drop_last());
        match entries.last().spec_specification().spec_alternative() {
            Some((group, _)) if !group_present(prior, group) => prior.push(group),
            _ => prior,
        }
    }
}

/// One branch of a group has satisfying current evidence for every member.
pub open spec fn branch_complete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    evidence: Seq<RequirementEvidence>,
    group: AlternativeGroupId,
    branch: AlternativeBranchId,
) -> bool {
    forall |member: int| 0 <= member < entries.len() ==>
        match #[trigger] entries[member].spec_specification().spec_alternative() {
            Some((member_group, member_branch))
                if same_group(member_group, group) && same_branch(member_branch, branch) =>
            {
                supplied_evidence_verdict(ledger, candidate, &entries[member], evidence)
                    == crate::EvidenceVerdict::Satisfied
            },
            _ => true,
        }
}

/// Exact branch aliases have the same input-defined completion result.
pub proof fn same_branch_preserves_completion(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    evidence: Seq<RequirementEvidence>,
    group: AlternativeGroupId,
    left: AlternativeBranchId,
    right: AlternativeBranchId,
)
    requires same_branch(left, right),
    ensures branch_complete(ledger, candidate, entries, evidence, group, left)
        == branch_complete(ledger, candidate, entries, evidence, group, right),
{
    assert forall |member: int| 0 <= member < entries.len() implies
        match #[trigger] entries[member].spec_specification().spec_alternative() {
            Some((member_group, member_branch))
                if same_group(member_group, group) && same_branch(member_branch, left) =>
            {
                supplied_evidence_verdict(
                    ledger, candidate, &entries[member], evidence,
                ) == crate::EvidenceVerdict::Satisfied
            },
            _ => true,
        } == match entries[member].spec_specification().spec_alternative() {
            Some((member_group, member_branch))
                if same_group(member_group, group) && same_branch(member_branch, right) =>
            {
                supplied_evidence_verdict(
                    ledger, candidate, &entries[member], evidence,
                ) == crate::EvidenceVerdict::Satisfied
            },
            _ => true,
        } by {}
}

/// A complete branch supplies satisfying evidence for each exact member.
pub proof fn branch_complete_member(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    evidence: Seq<RequirementEvidence>,
    group: AlternativeGroupId,
    branch: AlternativeBranchId,
    member: int,
)
    requires
        branch_complete(ledger, candidate, entries, evidence, group, branch),
        0 <= member < entries.len(),
        match entries[member].spec_specification().spec_alternative() {
            Some((member_group, member_branch)) => same_group(member_group, group)
                && same_branch(member_branch, branch),
            None => false,
        },
    ensures supplied_evidence_verdict(ledger, candidate, &entries[member], evidence)
        == crate::EvidenceVerdict::Satisfied,
{
}

/// One branch of a group has satisfying current evidence for every member.
pub open spec fn group_complete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    evidence: Seq<RequirementEvidence>,
    group: AlternativeGroupId,
) -> bool {
    exists |branch_entry: int| 0 <= branch_entry < entries.len()
        && match #[trigger] entries[branch_entry].spec_specification().spec_alternative() {
            Some((entry_group, branch)) => same_group(entry_group, group)
                && branch_complete(ledger, candidate, entries, evidence, group, branch),
            None => false,
        }
}

/// Number of first-seen alternative groups with one complete branch.
pub open spec fn completed_group_count(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    evidence: Seq<RequirementEvidence>,
    groups: Seq<AlternativeGroupId>,
) -> nat
    decreases groups.len(),
{
    if groups.len() == 0 {
        0
    } else {
        completed_group_count(ledger, candidate, entries, evidence, groups.drop_last())
            + if group_complete(ledger, candidate, entries, evidence, groups.last()) {
                1nat
            } else {
                0nat
            }
    }
}

/// First-seen alternative groups without any complete branch.
pub open spec fn incomplete_groups(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    evidence: Seq<RequirementEvidence>,
    groups: Seq<AlternativeGroupId>,
) -> Seq<AlternativeGroupId>
    decreases groups.len(),
{
    if groups.len() == 0 {
        Seq::empty()
    } else {
        let prior = incomplete_groups(
            ledger, candidate, entries, evidence, groups.drop_last());
        if group_complete(ledger, candidate, entries, evidence, groups.last()) {
            prior
        } else {
            prior.push(groups.last())
        }
    }
}

/// Appending one entry updates the first-seen group sequence exactly.
pub proof fn alternative_groups_after_push(
    entries: Seq<RequirementEntry>,
    entry: RequirementEntry,
)
    ensures alternative_groups(entries.push(entry)) == {
        let prior = alternative_groups(entries);
        match entry.spec_specification().spec_alternative() {
            Some((group, _)) if !group_present(prior, group) => prior.push(group),
            _ => prior,
        }
    },
{
    assert(entries.push(entry).drop_last() == entries);
    assert(entries.push(entry).last() == entry);
}

/// Appending one first-seen group updates completion accounting exactly.
pub proof fn group_accounting_after_push(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entries: Seq<RequirementEntry>,
    evidence: Seq<RequirementEvidence>,
    groups: Seq<AlternativeGroupId>,
    group: AlternativeGroupId,
)
    ensures
        completed_group_count(
            ledger, candidate, entries, evidence, groups.push(group))
            == completed_group_count(ledger, candidate, entries, evidence, groups)
                + if group_complete(ledger, candidate, entries, evidence, group) {
                    1nat
                } else {
                    0nat
                },
        incomplete_groups(ledger, candidate, entries, evidence, groups.push(group))
            == if group_complete(ledger, candidate, entries, evidence, group) {
                incomplete_groups(ledger, candidate, entries, evidence, groups)
            } else {
                incomplete_groups(ledger, candidate, entries, evidence, groups).push(group)
            },
{
    assert(groups.push(group).drop_last() == groups);
    assert(groups.push(group).last() == group);
}

/// Ordinary required entries and first-seen alternative groups partition ledger positions.
pub proof fn ordinary_group_partition_bound(
    entries: Seq<RequirementEntry>,
    conditions: Seq<ConditionObservation>,
)
    ensures super::ordinary_required_count(entries, conditions)
        + alternative_groups(entries).len() <= entries.len(),
    decreases entries.len(),
{
    if entries.len() > 0 {
        ordinary_group_partition_bound(entries.drop_last(), conditions);
        super::ordinary_after_push_dummy(entries.drop_last(), entries.last(), conditions);
        alternative_groups_after_push(entries.drop_last(), entries.last());
    }
}

} // verus!
