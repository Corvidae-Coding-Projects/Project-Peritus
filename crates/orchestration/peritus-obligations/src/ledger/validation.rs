//! Verified extraction admission in production error order.

use super::RequirementDraft;

use crate::{ObligationError, ObligationErrorKind, ObligationLimits, PublicTaskSource};
use core::cmp::Ordering;

fn validate_pair(
    _source: &PublicTaskSource,
    drafts: &[RequirementDraft],
    index: usize,
    _limits: ObligationLimits,
) -> Result<(), ObligationError> {
    let order = crate::order::compare(
        drafts[index - 1].id().digest().as_bytes(),
        drafts[index].id().digest().as_bytes(),
    );
    match order {
        Ordering::Equal => Err(ObligationError::requirement(
            ObligationErrorKind::DuplicateValue,
            drafts[index].id(),
        )),
        Ordering::Greater => Err(ObligationError::requirement(
            ObligationErrorKind::NonCanonicalOrder,
            drafts[index].id(),
        )),
        Ordering::Less => Ok(()),
    }
}
fn validate_draft(
    source: &PublicTaskSource,
    drafts: &[RequirementDraft],
    index: usize,
    limits: ObligationLimits,
) -> Result<(), ObligationError> {
    let draft = &drafts[index];
    let clause_length = draft.byte_end().saturating_sub(draft.byte_start());
    if draft.byte_start() >= draft.byte_end()
        || draft.byte_end() > source.content().len()
        || clause_length > limits.max_clause_bytes()
    {
        return Err(ObligationError::requirement(
            ObligationErrorKind::InvalidClauseSpan,
            draft.id(),
        ));
    }
    if index > u32::MAX as usize {
        return Err(ObligationError::requirement(ObligationErrorKind::LimitExceeded, draft.id()));
    }
    draft.specification().validate()?;
    crate::path::validate_paths(draft.paths(), limits.max_paths_per_requirement())?;
    Ok(())
}
pub(super) fn validate_drafts(
    source: &PublicTaskSource,
    drafts: &[RequirementDraft],
    limits: ObligationLimits,
) -> Result<(), ObligationError> {
    if drafts.is_empty() || drafts.len() > limits.max_requirements() {
        return Err(ObligationError::numbers(
            ObligationErrorKind::LimitExceeded,
            limits.max_requirements() as u64,
            drafts.len() as u64,
        ));
    }
    let mut index = 0;
    while index < drafts.len() {
        if index > 0 {
            validate_pair(source, drafts, index, limits)?;
        }
        validate_draft(source, drafts, index, limits)?;
        index += 1;
    }
    validate_alternatives(drafts)?;
    Ok(())
}
fn validate_alternatives(drafts: &[RequirementDraft]) -> Result<(), ObligationError> {
    let mut index = 0;
    while index < drafts.len() {
        if let Some((group, branch)) = drafts[index].specification().alternative() {
            let mut other = 0;
            let mut found = false;
            while other < drafts.len() && !found {
                match drafts[other].specification().alternative() {
                    Some((other_group, other_branch))
                        if crate::matching::group_ids_match(group, other_group)
                            && !crate::matching::branch_ids_match(branch, other_branch) =>
                    {
                        found = true;
                    }
                    _ => {}
                }
                other += 1;
            }

            if !found {
                return Err(ObligationError::plain(ObligationErrorKind::InvalidAlternative));
            }
        }
        index += 1;
    }
    Ok(())
}
