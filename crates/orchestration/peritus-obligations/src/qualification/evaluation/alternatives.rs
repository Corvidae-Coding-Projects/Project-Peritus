//! Verified first-seen alternative groups and complete-branch evaluation.

#[cfg(verus_only)]
use super::super::model;
use super::super::{EvidenceVerdict, verdict};
use crate::{AlternativeBranchId, AlternativeGroupId, RequirementEvidence, RequirementLedger};
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

pub(super) fn evaluate(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: &[RequirementEvidence],
) -> (result: (usize, usize, Vec<AlternativeGroupId>))
    requires crate::order::ordered(model::evidence_keys(evidence@)),
    ensures
        result.0 as nat == model::alternative_groups(ledger.spec_entries()).len(),
        result.1 as nat == model::completed_group_count(
            ledger,
            candidate,
            ledger.spec_entries(),
            evidence@,
            model::alternative_groups(ledger.spec_entries()),
        ),
        result.2@ == model::incomplete_groups(
            ledger,
            candidate,
            ledger.spec_entries(),
            evidence@,
            model::alternative_groups(ledger.spec_entries()),
        ),
        result.1 <= result.0,
{
    assert(crate::order::ordered(model::evidence_keys(evidence@)));
    let groups = collect_groups(ledger.entries());
    let mut satisfied = 0;
    let mut incomplete = Vec::new();
    let mut index = 0;
    while index < groups.len()
        invariant
            groups@ == model::alternative_groups(ledger.spec_entries()),
            index <= groups@.len(),
            crate::order::ordered(model::evidence_keys(evidence@)),
            satisfied as nat == model::completed_group_count(
                ledger, candidate, ledger.spec_entries(), evidence@,
                groups@.take(index as int)),
            incomplete@ == model::incomplete_groups(
                ledger, candidate, ledger.spec_entries(), evidence@,
                groups@.take(index as int)),
            satisfied <= index,
        decreases groups.len() - index,
    {
        let group = groups[index];
        let ghost prior = groups@.take(index as int);
        let complete = any_branch_complete(ledger, candidate, evidence, group);
        if complete {
            satisfied += 1;
        } else {
            incomplete.push(group);
        }
        proof {
            assert(groups@.take(index as int + 1) == prior.push(group));
            model::group_accounting_after_push(
                ledger, candidate, ledger.spec_entries(), evidence@, prior, group);
        }
        index += 1;
    }
    assert(groups@.take(index as int) =~= groups@);
    (groups.len(), satisfied, incomplete)
}

fn collect_groups(
    entries: &[crate::RequirementEntry],
) -> (groups: Vec<AlternativeGroupId>)
    ensures groups@ == model::alternative_groups(entries@),
{
    let mut groups = Vec::new();
    let mut index = 0;
    while index < entries.len()
        invariant
            index <= entries@.len(),
            groups@ == model::alternative_groups(entries@.take(index as int)),
        decreases entries.len() - index,
    {
        let entry = &entries[index];
        let ghost prior_entries = entries@.take(index as int);
        let ghost prior_groups = groups@;
        match entry.specification().alternative() {
            Some((group, _)) if !group_present(groups.as_slice(), group) => {
                groups.push(group);
            },
            _ => {},
        }
        proof {
            assert(entries@.take(index as int + 1) == prior_entries.push(*entry));
            model::alternative_groups_after_push(prior_entries, *entry);
        }
        index += 1;
    }
    assert(entries@.take(index as int) =~= entries@);
    groups
}

fn group_present(groups: &[AlternativeGroupId], group: AlternativeGroupId) -> (present: bool)
    ensures present == model::group_present(groups@, group),
{
    let mut index = 0;
    while index < groups.len()
        invariant
            index <= groups@.len(),
            forall |prior: int| 0 <= prior < index ==>
                !model::same_group(#[trigger] groups@[prior], group),
        decreases groups.len() - index,
    {
        if crate::matching::group_ids_match(groups[index], group) {
            return true;
        }
        index += 1;
    }
    false
}

fn checked_branch_present(
    branches: &[AlternativeBranchId],
    branch: AlternativeBranchId,
) -> (present: bool)
    ensures present == model::branch_present(branches@, branch),
{
    let mut index = 0;
    while index < branches.len()
        invariant
            index <= branches@.len(),
            forall |prior: int| 0 <= prior < index ==>
                !model::same_branch(#[trigger] branches@[prior], branch),
        decreases branches.len() - index,
    {
        if crate::matching::branch_ids_match(branches[index], branch) {
            return true;
        }
        index += 1;
    }
    false
}

fn any_branch_complete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: &[RequirementEvidence],
    group: AlternativeGroupId,
) -> (complete: bool)
    requires crate::order::ordered(model::evidence_keys(evidence@)),
    ensures complete == model::group_complete(
        ledger, candidate, ledger.spec_entries(), evidence@, group),
{
    assert(crate::order::ordered(model::evidence_keys(evidence@)));
    let entries = ledger.entries();
    let mut checked_branches = Vec::new();
    let mut index = 0;
    while index < entries.len()
        invariant
            entries@ == ledger.spec_entries(),
            index <= entries@.len(),
            crate::order::ordered(model::evidence_keys(evidence@)),
            model::checked_branches_sound(entries@, index as int, group, checked_branches@),
            forall |prior: int| 0 <= prior < index ==>
                match #[trigger] entries@[prior].spec_specification().spec_alternative() {
                    Some((entry_group, branch)) if model::same_group(entry_group, group) =>
                        !model::branch_complete(
                            ledger, candidate, ledger.spec_entries(), evidence@, group, branch),
                    _ => true,
                },
        decreases entries.len() - index,
    {
        match entries[index].specification().alternative() {
            Some((entry_group, branch))
                if crate::matching::group_ids_match(entry_group, group) =>
            {
                let already_checked =
                    checked_branch_present(checked_branches.as_slice(), branch);
                if already_checked {
                    proof {
                        model::checked_branch_is_incomplete(
                            ledger,
                            candidate,
                            ledger.spec_entries(),
                            evidence@,
                            index as int,
                            group,
                            checked_branches@,
                            branch,
                        );
                    };
                }
                if !already_checked {
                    let complete = branch_complete(ledger, candidate, evidence, group, branch);
                    if complete {
                        return true;
                    }
                    assert(!model::branch_complete(
                        ledger, candidate, ledger.spec_entries(), evidence@, group, branch));
                    let ghost prior_checked = checked_branches@;
                    checked_branches.push(branch);
                    proof {
                        model::checked_branches_after_push(
                            entries@, index as int, group, prior_checked, branch,
                        );
                    };
                }
            },
            _ => {},
        }
        index += 1;
    }
    assert(index == entries.len());
    assert(!model::group_complete(
        ledger, candidate, ledger.spec_entries(), evidence@, group)) by {
        if model::group_complete(ledger, candidate, ledger.spec_entries(), evidence@, group) {
            let branch_entry = choose |branch_entry: int|
                0 <= branch_entry < ledger.spec_entries().len()
                    && match #[trigger] ledger.spec_entries()[branch_entry]
                        .spec_specification().spec_alternative()
                    {
                        Some((entry_group, branch)) => model::same_group(entry_group, group)
                            && model::branch_complete(
                                ledger, candidate, ledger.spec_entries(), evidence@, group, branch),
                        None => false,
                    };
            assert(branch_entry < index);
        }
    }
    false
}

fn branch_complete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: &[RequirementEvidence],
    group: AlternativeGroupId,
    branch: AlternativeBranchId,
) -> (complete: bool)
    requires crate::order::ordered(model::evidence_keys(evidence@)),
    ensures complete == model::branch_complete(
        ledger, candidate, ledger.spec_entries(), evidence@, group, branch),
{
    assert(crate::order::ordered(model::evidence_keys(evidence@)));
    let entries = ledger.entries();
    let mut index = 0;
    while index < entries.len()
        invariant
            entries@ == ledger.spec_entries(),
            index <= entries@.len(),
            crate::order::ordered(model::evidence_keys(evidence@)),
            forall |prior: int| 0 <= prior < index ==>
                match #[trigger] entries@[prior].spec_specification().spec_alternative() {
                    Some((member_group, member_branch))
                        if model::same_group(member_group, group)
                            && model::same_branch(member_branch, branch) =>
                    {
                        model::supplied_evidence_verdict(
                            ledger, candidate, &entries@[prior], evidence@,
                        ) == EvidenceVerdict::Satisfied
                    },
                    _ => true,
                },
        decreases entries.len() - index,
    {
        let entry = &entries[index];
        assert(*entry == entries@[index as int]);
        if is_branch_member(entry, group, branch) {
            let value = verdict::evidence_verdict(
                ledger, candidate, entry, evidence);
            assert(value == model::supplied_evidence_verdict(
                ledger, candidate, entry, evidence@));
            match value {
                EvidenceVerdict::Satisfied => {
                    assert(model::supplied_evidence_verdict(
                        ledger, candidate, entry, evidence@,
                    ) == EvidenceVerdict::Satisfied);
                },
                EvidenceVerdict::Missing
                | EvidenceVerdict::Stale
                | EvidenceVerdict::Invalid => {
                    assert(!model::branch_complete(
                        ledger,
                        candidate,
                        ledger.spec_entries(),
                        evidence@,
                        group,
                        branch,
                    )) by {
                        if model::branch_complete(
                            ledger,
                            candidate,
                            ledger.spec_entries(),
                            evidence@,
                            group,
                            branch,
                        ) {
                            model::branch_complete_member(
                                ledger,
                                candidate,
                                ledger.spec_entries(),
                                evidence@,
                                group,
                                branch,
                                index as int,
                            );
                            assert(model::supplied_evidence_verdict(
                                ledger, candidate, entry, evidence@,
                            ) == EvidenceVerdict::Satisfied);
                        }
                    }
                    return false;
                }
            }
        }
        index += 1;
    }
    assert(index == entries.len());
    true
}

fn is_branch_member(
    entry: &crate::RequirementEntry,
    group: AlternativeGroupId,
    branch: AlternativeBranchId,
) -> (member: bool)
    ensures member == match entry.spec_specification().spec_alternative() {
        Some((member_group, member_branch)) => model::same_group(member_group, group)
            && model::same_branch(member_branch, branch),
        None => false,
    },
{
    match entry.specification().alternative() {
        Some((member_group, member_branch)) => {
            crate::matching::group_ids_match(member_group, group)
                && crate::matching::branch_ids_match(member_branch, branch)
        },
        None => false,
    }
}

} // verus!
