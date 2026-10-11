//! Transactional retrieval orchestration and complete explanations.

use super::filter::exclusion;
use super::output::{
    CandidateExplanation, ExcludedMemory, ExclusionReason, MemoryCandidate, RankScore,
    RetrievalPlan,
};
use super::ranking::score;
use super::types::{MAX_RETRIEVAL_INPUTS, RetrievalPolicy, RetrievalQuery};
use crate::{MemoryError, MemoryErrorKind, MemoryField, MemoryRecord, MemoryTombstone};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorkingCandidate {
    record_index: usize,
    score: Option<RankScore>,
    reason: Option<ExclusionReason>,
    selected: bool,
}
#[doc = " Filters, ranks, and budget-selects memory with one explanation for every input record."]
#[doc = ""]
#[doc = " Input order does not affect the result. Equal scores are ordered by stable memory identifier."]
#[doc = " Malformed duplicate record identities or conflicting tombstones fail transactionally."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = ""]
#[doc = " Returns a typed error for excessive input, duplicate memory identities, conflicting"]
#[doc = " tombstones, tombstone digest mismatch, or checked arithmetic failure."]
#[allow(clippy::too_many_lines, reason = "transactional planning keeps one auditable pass")]
pub fn retrieve(
    records: &[MemoryRecord],
    tombstones: &[MemoryTombstone],
    policy: &RetrievalPolicy,
    query: &RetrievalQuery,
) -> Result<RetrievalPlan, MemoryError> {
    if records.len() > MAX_RETRIEVAL_INPUTS {
        return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::Records));
    }
    if tombstones.len() > MAX_RETRIEVAL_INPUTS {
        return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::Tombstones));
    }
    validate_tombstones(tombstones)?;
    let record_order = record_order(records)?;
    let mut working = Vec::new();
    let mut ordered_index = 0;
    while ordered_index < record_order.len() {
        let record_index = record_order[ordered_index];
        if record_index >= records.len() {
            return Err(MemoryError::field(
                MemoryErrorKind::ConflictingRevision,
                MemoryField::Records,
            ));
        }
        let record = &records[record_index];
        let reason = exclusion(record, tombstones, policy, query)?;
        let candidate_score =
            if reason.is_none() { Some(score(record, policy, query)?) } else { None };
        working.push(WorkingCandidate {
            record_index,
            score: candidate_score,
            reason,
            selected: false,
        });
        ordered_index += 1;
    }
    let ranked = ranked_positions(&working, records);
    let token_budget = query.token_budget();
    let mut selected = Vec::new();
    let mut used_tokens = 0_u32;
    let mut rank_index = 0;
    while rank_index < ranked.len() {
        let position = ranked[rank_index];
        if position >= working.len() {
            return Err(MemoryError::field(
                MemoryErrorKind::ConflictingRevision,
                MemoryField::Records,
            ));
        }
        let record_index = working[position].record_index;
        if record_index >= records.len() {
            return Err(MemoryError::field(
                MemoryErrorKind::ConflictingRevision,
                MemoryField::Records,
            ));
        }
        let record = &records[record_index];
        let Some(candidate_score) = working[position].score else {
            return Err(MemoryError::memory(
                MemoryErrorKind::ConflictingRevision,
                MemoryField::Records,
                record.id(),
            ));
        };
        if selected.len() >= usize::from(policy.limits().max_results()) {
            working[position].reason = Some(ExclusionReason::ResultLimit);
        } else {
            let next_tokens = used_tokens.checked_add(record.material().estimated_tokens()).ok_or(
                MemoryError::field(MemoryErrorKind::ArithmeticOverflow, MemoryField::TokenBudget),
            )?;
            if next_tokens > token_budget {
                working[position].reason = Some(ExclusionReason::TokenBudget);
            } else {
                working[position].selected = true;
                used_tokens = next_tokens;
                selected.push(MemoryCandidate::new(
                    record.id(),
                    record.revision(),
                    *record.scope(),
                    record.material().clone(),
                    candidate_score,
                ));
            }
        }
        rank_index += 1;
    }
    let mut explanations = Vec::new();
    let mut explanation_index = 0;
    while explanation_index < working.len() {
        let entry = working[explanation_index];
        if entry.record_index >= records.len() {
            return Err(MemoryError::field(
                MemoryErrorKind::ConflictingRevision,
                MemoryField::Records,
            ));
        }
        let record = &records[entry.record_index];
        if entry.selected {
            let Some(candidate_score) = entry.score else {
                return Err(MemoryError::memory(
                    MemoryErrorKind::ConflictingRevision,
                    MemoryField::Records,
                    record.id(),
                ));
            };
            explanations.push(CandidateExplanation::Selected(
                record.id(),
                record.revision(),
                candidate_score,
            ));
        } else {
            let Some(reason) = entry.reason else {
                return Err(MemoryError::memory(
                    MemoryErrorKind::ConflictingRevision,
                    MemoryField::Records,
                    record.id(),
                ));
            };
            explanations.push(CandidateExplanation::Excluded(ExcludedMemory::new(
                record.id(),
                record.revision(),
                reason,
                entry.score,
            )));
        }
        explanation_index += 1;
    }
    Ok(RetrievalPlan::new(selected, explanations, token_budget, used_tokens))
}
fn record_order(records: &[MemoryRecord]) -> Result<Vec<usize>, MemoryError> {
    let mut order = Vec::new();
    let mut source_index = 0;
    while source_index < records.len() {
        order.push(source_index);
        let mut position = order.len() - 1;
        while position > 0 {
            let previous_index = order[position - 1];
            let current_index = order[position];
            if previous_index >= records.len() || current_index >= records.len() {
                return Err(MemoryError::field(
                    MemoryErrorKind::ConflictingRevision,
                    MemoryField::Records,
                ));
            }
            if records[previous_index].id() <= records[current_index].id() {
                break;
            }
            let previous = previous_index;
            let current = current_index;
            order[position - 1] = current;
            order[position] = previous;
            position -= 1;
        }
        source_index += 1;
    }
    if order.len() > 1 {
        let mut index = 1;
        while index < order.len() {
            let previous_index = order[index - 1];
            let current_index = order[index];
            if previous_index >= records.len() || current_index >= records.len() {
                return Err(MemoryError::field(
                    MemoryErrorKind::ConflictingRevision,
                    MemoryField::Records,
                ));
            }
            if records[previous_index].id() == records[current_index].id() {
                return Err(MemoryError::memory(
                    MemoryErrorKind::DuplicateValue,
                    MemoryField::Records,
                    records[current_index].id(),
                ));
            }
            index += 1;
        }
    }
    Ok(order)
}
fn ranked_positions(working: &[WorkingCandidate], records: &[MemoryRecord]) -> Vec<usize> {
    let mut ranked = Vec::new();
    let mut index = 0;
    while index < working.len() {
        if working[index].score.is_some() {
            ranked.push(index);
            let mut position = ranked.len() - 1;
            while position > 0 {
                let current_index = ranked[position];
                let previous_index = ranked[position - 1];
                if current_index >= working.len() || previous_index >= working.len() {
                    return Vec::new();
                }
                if !ranks_before(current_index, previous_index, working, records) {
                    break;
                }
                let previous = ranked[position - 1];
                let current = ranked[position];
                ranked[position - 1] = current;
                ranked[position] = previous;
                position -= 1;
            }
        }
        index += 1;
    }
    ranked
}
fn ranks_before(
    left: usize,
    right: usize,
    working: &[WorkingCandidate],
    records: &[MemoryRecord],
) -> bool {
    if left >= working.len() || right >= working.len() {
        return false;
    }
    let left_score = match working[left].score {
        Some(value) => value.total().get(),
        None => return false,
    };
    let right_score = match working[right].score {
        Some(value) => value.total().get(),
        None => return true,
    };
    let left_record = working[left].record_index;
    let right_record = working[right].record_index;
    if left_record >= records.len() || right_record >= records.len() {
        return false;
    }
    left_score > right_score
        || left_score == right_score && records[left_record].id() < records[right_record].id()
}
fn validate_tombstones(tombstones: &[MemoryTombstone]) -> Result<(), MemoryError> {
    let mut left = 0;
    while left < tombstones.len() {
        let mut right = left + 1;
        while right < tombstones.len() {
            if tombstones[left].memory_id() == tombstones[right].memory_id()
                && tombstones[left].last_known_revision() == tombstones[right].last_known_revision()
            {
                return Err(MemoryError::memory(
                    MemoryErrorKind::ConflictingRevision,
                    MemoryField::Tombstones,
                    tombstones[left].memory_id(),
                ));
            }
            right += 1;
        }
        left += 1;
    }
    Ok(())
}
