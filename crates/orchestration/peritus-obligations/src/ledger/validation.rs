//! Verified extraction admission in production error order.

use super::RequirementDraft;
#[cfg(verus_only)]
use super::{admission, model};
use crate::{ObligationError, ObligationErrorKind, ObligationLimits, PublicTaskSource};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

fn validate_pair(
    _source: &PublicTaskSource,
    drafts: &[RequirementDraft],
    index: usize,
    _limits: ObligationLimits,
) -> (result: Result<(), ObligationError>)
    requires 1 <= index < drafts@.len(),
    ensures
        result.is_ok() == (crate::order::byte_order(
            model::draft_keys(drafts@)[index as int - 1],
            model::draft_keys(drafts@)[index as int],
        ) == Ordering::Less),
        result.is_err() ==> !model::extraction_inputs_valid(_source, drafts@, _limits),
        match result {
            Ok(()) => true,
            Err(error) => admission::pair_error(drafts@, index as int, &error),
        },
{
    let order = crate::order::compare(
        drafts[index - 1].id().digest().as_bytes(),
        drafts[index].id().digest().as_bytes(),
    );
    assert(model::draft_keys(drafts@)[index as int - 1]
        == model::requirement_key(drafts@[index as int - 1].spec_id()));
    assert(model::draft_keys(drafts@)[index as int]
        == model::requirement_key(drafts@[index as int].spec_id()));
    assert(order == crate::order::byte_order(
        model::draft_keys(drafts@)[index as int - 1],
        model::draft_keys(drafts@)[index as int],
    ));
    match order {
        Ordering::Equal => {
            assert(!crate::order::ordered(model::draft_keys(drafts@))) by {
                if crate::order::ordered(model::draft_keys(drafts@)) {
                    model::ordered_at(model::draft_keys(drafts@), index as int);
                }
            }
            Err(ObligationError::requirement(
                ObligationErrorKind::DuplicateValue,
                drafts[index].id(),
            ))
        },
        Ordering::Greater => {
            assert(!crate::order::ordered(model::draft_keys(drafts@))) by {
                if crate::order::ordered(model::draft_keys(drafts@)) {
                    model::ordered_at(model::draft_keys(drafts@), index as int);
                }
            }
            Err(ObligationError::requirement(
                ObligationErrorKind::NonCanonicalOrder,
                drafts[index].id(),
            ))
        },
        Ordering::Less => Ok(()),
    }
}

fn validate_draft(
    source: &PublicTaskSource,
    drafts: &[RequirementDraft],
    index: usize,
    limits: ObligationLimits,
) -> (result: Result<(), ObligationError>)
    requires index < drafts@.len(),
    ensures
        result.is_ok() == model::draft_valid(source, &drafts@[index as int], index as int, limits),
        result.is_err() ==> !model::extraction_inputs_valid(source, drafts@, limits),
        match result {
            Ok(()) => true,
            Err(error) => admission::draft_error(source, &drafts@[index as int], index as int, limits, &error),
        },
{
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
        return Err(ObligationError::requirement(
            ObligationErrorKind::LimitExceeded,
            draft.id(),
        ));
    }
    draft.specification().validate()?;
    crate::path::validate_paths(draft.paths(), limits.max_paths_per_requirement())?;
    Ok(())
}

pub(super) fn validate_drafts(
    source: &PublicTaskSource,
    drafts: &[RequirementDraft],
    limits: ObligationLimits,
) -> (result: Result<(), ObligationError>)
    ensures
        result.is_ok() == model::extraction_inputs_valid(source, drafts@, limits),
        match result {
            Ok(()) => true,
            Err(error) => admission::extraction_error(source, drafts@, limits, &error),
        },
{
    if drafts.is_empty() || drafts.len() > limits.max_requirements() {
        assert(!model::extraction_inputs_valid(source, drafts@, limits));
        return Err(ObligationError::numbers(
            ObligationErrorKind::LimitExceeded,
            limits.max_requirements() as u64,
            drafts.len() as u64,
        ));
    }
    let mut index = 0;
    while index < drafts.len()
        invariant
            0 < drafts@.len() <= limits.spec_max_requirements(),
            index <= drafts@.len(),
            admission::valid_prefix(source, drafts@, index as int, limits),
            forall |prior: int| 0 <= prior < index ==>
                model::draft_valid(source, &drafts@[prior], prior, limits),
            forall |prior: int| 1 <= prior < index ==>
                crate::order::byte_order(
                    #[trigger] model::draft_keys(drafts@)[prior - 1],
                    #[trigger] model::draft_keys(drafts@)[prior],
                ) == Ordering::Less,
        decreases drafts.len() - index,
    {
        if index > 0 {
            if let Err(error) = validate_pair(source, drafts, index, limits) {
                assert(admission::first_draft_error(source, drafts@, index as int, limits, &error));
                return Err(error);
            }
            assert(crate::order::byte_order(
                model::draft_keys(drafts@)[index as int - 1],
                model::draft_keys(drafts@)[index as int],
            ) == Ordering::Less);
        }
        if let Err(error) = validate_draft(source, drafts, index, limits) {
            assert(admission::first_draft_error(source, drafts@, index as int, limits, &error));
            return Err(error);
        }
        assert(model::draft_valid(source, &drafts@[index as int], index as int, limits));
        index += 1;
    }
    assert(crate::order::ordered(model::draft_keys(drafts@)));
    validate_alternatives(drafts)?;
    assert(model::extraction_inputs_valid(source, drafts@, limits));
    Ok(())
}

fn validate_alternatives(
    drafts: &[RequirementDraft],
) -> (result: Result<(), ObligationError>)
    ensures
        result.is_ok() == model::draft_alternatives_valid(drafts@),
        match result {
            Ok(()) => true,
            Err(error) => error.spec_plain(ObligationErrorKind::InvalidAlternative),
        },
{
    let mut index = 0;
    while index < drafts.len()
        invariant
            index <= drafts@.len(),
            forall |prior: int| 0 <= prior < index ==>
                match #[trigger] drafts@[prior].spec_specification().spec_alternative() {
                    Some((group, branch)) => exists |other: int| 0 <= other < drafts@.len()
                        && match #[trigger] drafts@[other].spec_specification().spec_alternative() {
                            Some((other_group, other_branch)) =>
                                model::same_group(group, other_group)
                                    && !model::same_branch(branch, other_branch),
                            None => false,
                        },
                    None => true,
                },
        decreases drafts.len() - index,
    {
        if let Some((group, branch)) = drafts[index].specification().alternative() {
            let mut other = 0;
            let mut found = false;
            let ghost mut witness_index = 0usize;
            while other < drafts.len() && !found
                invariant
                    other <= drafts@.len(),
                    found ==> witness_index < other && match
                        #[trigger] drafts@[witness_index as int]
                            .spec_specification().spec_alternative()
                    {
                        Some((other_group, other_branch)) =>
                            model::same_group(group, other_group)
                                && !model::same_branch(branch, other_branch),
                        None => false,
                    },
                    !found ==> forall |prior: int| 0 <= prior < other ==>
                        match #[trigger] drafts@[prior].spec_specification().spec_alternative() {
                            Some((other_group, other_branch)) =>
                                !model::same_group(group, other_group)
                                    || model::same_branch(branch, other_branch),
                            None => true,
                        },
                decreases drafts.len() - other,
            {
                match drafts[other].specification().alternative() {
                    Some((other_group, other_branch))
                        if crate::matching::group_ids_match(group, other_group)
                            && !crate::matching::branch_ids_match(branch, other_branch) =>
                    {
                        proof { witness_index = other; }
                        found = true;
                    },
                    _ => {},
                }
                other += 1;
            }
            if found {
                assert(witness_index < drafts@.len());
            }
            if !found {
                assert(!model::draft_alternatives_valid(drafts@)) by {
                    if model::draft_alternatives_valid(drafts@) {
                        let witness = index as int;
                    }
                }
                return Err(ObligationError::plain(ObligationErrorKind::InvalidAlternative));
            }
            assert(match drafts@[witness_index as int]
                .spec_specification().spec_alternative()
            {
                Some((other_group, other_branch)) => model::same_group(group, other_group)
                    && !model::same_branch(branch, other_branch),
                None => false,
            });
            assert(exists |other: int| 0 <= other < drafts@.len()
                && match #[trigger] drafts@[other].spec_specification().spec_alternative() {
                    Some((other_group, other_branch)) => model::same_group(group, other_group)
                        && !model::same_branch(branch, other_branch),
                    None => false,
                });
        }
        index += 1;
    }
    assert(model::draft_alternatives_valid(drafts@));
    Ok(())
}

} // verus!
