//! Verified first-seen alternative groups and complete-branch evaluation.

use super::super::{EvidenceVerdict, verdict};
use crate::{AlternativeBranchId, AlternativeGroupId, RequirementEvidence, RequirementLedger};
use peritus_run_settlement::CandidateIdentity;

pub(super) fn evaluate(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: &[RequirementEvidence],
) -> (usize, usize, Vec<AlternativeGroupId>) {
    let groups = collect_groups(ledger.entries());
    let mut satisfied = 0;
    let mut incomplete = Vec::new();
    let mut index = 0;
    while index < groups.len() {
        let group = groups[index];
        let complete = any_branch_complete(ledger, candidate, evidence, group);
        if complete {
            satisfied += 1;
        } else {
            incomplete.push(group);
        }
        index += 1;
    }
    (groups.len(), satisfied, incomplete)
}
fn collect_groups(entries: &[crate::RequirementEntry]) -> Vec<AlternativeGroupId> {
    let mut groups = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        let entry = &entries[index];
        match entry.specification().alternative() {
            Some((group, _)) if !group_present(groups.as_slice(), group) => {
                groups.push(group);
            }
            _ => {}
        }
        index += 1;
    }
    groups
}
fn group_present(groups: &[AlternativeGroupId], group: AlternativeGroupId) -> bool {
    let mut index = 0;
    while index < groups.len() {
        if crate::matching::group_ids_match(groups[index], group) {
            return true;
        }
        index += 1;
    }
    false
}
fn checked_branch_present(branches: &[AlternativeBranchId], branch: AlternativeBranchId) -> bool {
    let mut index = 0;
    while index < branches.len() {
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
) -> bool {
    let entries = ledger.entries();
    let mut checked_branches = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        match entries[index].specification().alternative() {
            Some((entry_group, branch)) if crate::matching::group_ids_match(entry_group, group) => {
                let already_checked = checked_branch_present(checked_branches.as_slice(), branch);

                if !already_checked {
                    let complete = branch_complete(ledger, candidate, evidence, group, branch);
                    if complete {
                        return true;
                    }
                    checked_branches.push(branch);
                }
            }
            _ => {}
        }
        index += 1;
    }
    false
}
fn branch_complete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: &[RequirementEvidence],
    group: AlternativeGroupId,
    branch: AlternativeBranchId,
) -> bool {
    let entries = ledger.entries();
    let mut index = 0;
    while index < entries.len() {
        let entry = &entries[index];
        if is_branch_member(entry, group, branch) {
            let value = verdict::evidence_verdict(ledger, candidate, entry, evidence);
            match value {
                EvidenceVerdict::Satisfied => {}
                EvidenceVerdict::Missing | EvidenceVerdict::Stale | EvidenceVerdict::Invalid => {
                    return false;
                }
            }
        }
        index += 1;
    }
    true
}
fn is_branch_member(
    entry: &crate::RequirementEntry,
    group: AlternativeGroupId,
    branch: AlternativeBranchId,
) -> bool {
    match entry.specification().alternative() {
        Some((member_group, member_branch)) => {
            crate::matching::group_ids_match(member_group, group)
                && crate::matching::branch_ids_match(member_branch, branch)
        }
        None => false,
    }
}
