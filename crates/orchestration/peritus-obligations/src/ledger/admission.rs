//! Exact extraction error precedence over the supplied draft sequence.

use super::{RequirementDraft, model};
use crate::{
    ObligationError, ObligationErrorKind, ObligationLimits, PathMention, PublicTaskSource,
};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// The current identity is strictly above its predecessor, or is the first identity.
pub open spec fn pair_valid(drafts: Seq<RequirementDraft>, index: int) -> bool {
    0 <= index < drafts.len() && (index == 0 || crate::order::byte_order(
        model::draft_keys(drafts)[index - 1], model::draft_keys(drafts)[index]) == Ordering::Less)
}

/// Complete payload of one duplicate or descending adjacent requirement identity.
pub open spec fn pair_error(
    drafts: Seq<RequirementDraft>, index: int, error: &ObligationError,
) -> bool {
    1 <= index < drafts.len() && match crate::order::byte_order(
        model::draft_keys(drafts)[index - 1], model::draft_keys(drafts)[index]) {
        Ordering::Equal => error.spec_requirement(ObligationErrorKind::DuplicateValue, drafts[index].spec_id()),
        Ordering::Greater => error.spec_requirement(ObligationErrorKind::NonCanonicalOrder, drafts[index].spec_id()),
        Ordering::Less => false,
    }
}

/// Complete single-draft error: span, ordinal, typed shape, then bounded canonical paths.
pub open spec fn draft_error(
    source: &PublicTaskSource,
    draft: &RequirementDraft,
    index: int,
    limits: ObligationLimits,
    error: &ObligationError,
) -> bool {
    if !model::span_valid(source, draft, limits) {
        error.spec_requirement(ObligationErrorKind::InvalidClauseSpan, draft.spec_id())
    } else if index > u32::MAX as int {
        error.spec_requirement(ObligationErrorKind::LimitExceeded, draft.spec_id())
    } else if !draft.spec_specification().spec_valid() {
        error.spec_plain(ObligationErrorKind::RequirementShapeMismatch)
    } else {
        PathMention::spec_validation_error(draft.spec_paths(), limits.spec_max_paths(), *error)
    }
}

/// Every earlier draft passed both its adjacent-identity and intrinsic validation.
pub open spec fn valid_prefix(
    source: &PublicTaskSource,
    drafts: Seq<RequirementDraft>,
    end: int,
    limits: ObligationLimits,
) -> bool {
    0 <= end <= drafts.len() && forall |index: int| #![trigger drafts[index]] 0 <= index < end ==>
        pair_valid(drafts, index) && model::draft_valid(source, &drafts[index], index, limits)
}

/// First draft rejected by the interleaved production identity-and-content traversal.
pub open spec fn first_draft_error(
    source: &PublicTaskSource,
    drafts: Seq<RequirementDraft>,
    index: int,
    limits: ObligationLimits,
    error: &ObligationError,
) -> bool {
    &&& 0 <= index < drafts.len()
    &&& valid_prefix(source, drafts, index, limits)
    &&& if !pair_valid(drafts, index) {
        pair_error(drafts, index, error)
    } else {
        draft_error(source, &drafts[index], index, limits, error)
    }
}

/// Complete extraction rejection, including all stored error fields and check precedence.
pub open spec fn extraction_error(
    source: &PublicTaskSource,
    drafts: Seq<RequirementDraft>,
    limits: ObligationLimits,
    error: &ObligationError,
) -> bool {
    if drafts.len() == 0 || drafts.len() > limits.spec_max_requirements() {
        error.spec_numbers(ObligationErrorKind::LimitExceeded,
            limits.spec_max_requirements() as u64, drafts.len() as u64)
    } else if !valid_prefix(source, drafts, drafts.len() as int, limits) {
        exists |index: int| #[trigger] first_draft_error(source, drafts, index, limits, error)
    } else {
        !model::draft_alternatives_valid(drafts)
            && error.spec_plain(ObligationErrorKind::InvalidAlternative)
    }
}

} // verus!
