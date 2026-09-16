//! Canonical path validation with exact failure precedence.

use super::PathMention;
use crate::{ObligationError, ObligationErrorKind};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// Every adjacent path identity in the checked prefix is strictly increasing.
pub open spec fn ordered_through(paths: Seq<PathMention>, end: nat) -> bool {
    end <= paths.len() && forall |index: int| 1 <= index < end ==>
        crate::order::byte_order(#[trigger] PathMention::keys(paths)[index - 1],
            #[trigger] PathMention::keys(paths)[index]) == Ordering::Less
}

/// Exact plain error for the first noncanonical adjacent path identity pair.
pub open spec fn first_bad_pair(
    paths: Seq<PathMention>, index: int, error: ObligationError,
) -> bool {
    1 <= index < paths.len() && ordered_through(paths, index as nat)
        && match crate::order::byte_order(
            PathMention::keys(paths)[index - 1], PathMention::keys(paths)[index]) {
            Ordering::Equal => error.spec_plain(ObligationErrorKind::DuplicateValue),
            Ordering::Greater => error.spec_plain(ObligationErrorKind::NonCanonicalOrder),
            Ordering::Less => false,
        }
}

/// The size error or first ordering error, with every stored payload specified.
pub open spec fn validation_error(
    paths: Seq<PathMention>, maximum: usize, error: ObligationError,
) -> bool {
    if paths.len() > maximum {
        error.spec_numbers(ObligationErrorKind::LimitExceeded, maximum as u64, paths.len() as u64)
    } else {
        exists |index: int| #[trigger] first_bad_pair(paths, index, error)
    }
}

pub(super) fn validate(paths: &[PathMention], maximum: usize) -> (result: Result<(), ObligationError>)
    ensures result.is_ok() == (paths@.len() <= maximum
        && crate::order::ordered(PathMention::keys(paths@))),
        result.is_ok() ==> crate::order::unique(PathMention::keys(paths@)),
        match result {
            Err(error) => validation_error(paths@, maximum, error),
            Ok(()) => true,
        },
{
    if paths.len() > maximum {
        return Err(ObligationError::numbers(
            ObligationErrorKind::LimitExceeded, maximum as u64, paths.len() as u64));
    }
    let mut index = 0;
    while index < paths.len()
        invariant index <= paths.len(), paths.len() <= maximum,
            ordered_through(paths@, index as nat),
        decreases paths.len() - index,
    {
        if index > 0 {
            let previous = paths[index - 1].id();
            let current = paths[index].id();
            let order = crate::order::compare(previous.digest().as_bytes(), current.digest().as_bytes());
            assert(order == crate::order::byte_order(
                PathMention::keys(paths@)[index as int - 1], PathMention::keys(paths@)[index as int]));
            match order {
                Ordering::Equal => {
                    assert(!crate::order::ordered(PathMention::keys(paths@)));
                    let error = ObligationError::plain(ObligationErrorKind::DuplicateValue);
                    assert(validation_error(paths@, maximum, error)) by {
                        assert(first_bad_pair(paths@, index as int, error));
                    }
                    return Err(error);
                }
                Ordering::Greater => {
                    assert(!crate::order::ordered(PathMention::keys(paths@)));
                    let error = ObligationError::plain(ObligationErrorKind::NonCanonicalOrder);
                    assert(validation_error(paths@, maximum, error)) by {
                        assert(first_bad_pair(paths@, index as int, error));
                    }
                    return Err(error);
                }
                Ordering::Less => {}
            }
        }
        assert(ordered_through(paths@, index as nat + 1));
        index += 1;
    }
    assert(crate::order::ordered(PathMention::keys(paths@)));
    proof {
        assert forall |i: int| 0 <= i < PathMention::keys(paths@).len() implies
            #[trigger] PathMention::keys(paths@)[i].len() == 32 by {
            assert(PathMention::keys(paths@)[i] == paths@[i].spec_id().spec_digest().spec_bytes()@);
        }
        crate::order::ordered_implies_unique(PathMention::keys(paths@), 32);
    }
    Ok(())
}

} // verus!
