//! Checked source admission with exact size, order, and first-error contracts.

use crate::{KnowledgeError, KnowledgeErrorKind, SourceDigest};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

pub fn validate_sources(
    sources: &[SourceDigest],
    maximum: usize,
    allow_empty: bool,
) -> (result: Result<(), KnowledgeError>)
    ensures result.is_ok() == crate::model::sources_admitted(sources@, maximum, allow_empty),
        match result {
            Ok(()) => true,
            Err(error) => crate::model::sources_validation_error(sources@, maximum, allow_empty, error),
        },
{
    if !allow_empty && sources.is_empty() {
        return Err(KnowledgeError::plain(KnowledgeErrorKind::EmptyCollection));
    }
    if sources.len() > maximum {
        return Err(KnowledgeError::numbers(
            KnowledgeErrorKind::LimitExceeded, maximum as u64, sources.len() as u64));
    }
    let mut index = 0;
    while index < sources.len()
        invariant index <= sources.len(), sources.len() <= maximum,
            allow_empty || sources.len() > 0,
            crate::model::sources_canonical_through(sources@, index as nat),
        decreases sources.len() - index,
    {
        if index > 0 {
            let previous = sources[index - 1].source_id();
            let current = sources[index].source_id();
            match previous.canonical_order(&current) {
                Ordering::Equal => {
                    assert(!crate::model::sources_canonical(sources@));
                    let error = KnowledgeError::source(KnowledgeErrorKind::DuplicateValue, current);
                    assert(crate::model::sources_validation_error(sources@, maximum, allow_empty, error)) by {
                        assert(crate::model::sources_first_bad_pair(sources@, index as int, error));
                    }
                    return Err(error);
                }
                Ordering::Greater => {
                    assert(!crate::model::sources_canonical(sources@));
                    let error = KnowledgeError::source(KnowledgeErrorKind::NonCanonicalOrder, current);
                    assert(crate::model::sources_validation_error(sources@, maximum, allow_empty, error)) by {
                        assert(crate::model::sources_first_bad_pair(sources@, index as int, error));
                    }
                    return Err(error);
                }
                Ordering::Less => {}
            }
        }
        assert(crate::model::sources_canonical_through(sources@, index as nat + 1));
        index += 1;
    }
    Ok(())
}

} // verus!
