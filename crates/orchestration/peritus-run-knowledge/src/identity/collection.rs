//! Production validation of canonical section identity collections.

use crate::{KnowledgeError, KnowledgeErrorKind, KnowledgeSectionId};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

pub fn validate_section_ids(
    ids: &[KnowledgeSectionId], excluded: Option<KnowledgeSectionId>,
) -> (result: Result<(), KnowledgeError>)
    ensures result.is_ok() == crate::model::id_members_valid(ids@, excluded),
        match result {
            Ok(()) => true,
            Err(error) => crate::model::id_collection_error(ids@, excluded, error),
        },
{
    let mut index = 0;
    while index < ids.len()
        invariant index <= ids.len(),
            crate::model::id_members_valid_through(ids@, excluded, index as nat),
        decreases ids.len() - index,
    {
        match excluded {
            Some(id) if ids[index].matches(&id) => {
                assert(!crate::model::id_members_valid(ids@, excluded));
                let error = KnowledgeError::section(KnowledgeErrorKind::SelfDependency, id);
                assert(crate::model::id_collection_error(ids@, excluded, error)) by {
                    assert(crate::model::id_first_error(ids@, excluded, index as int, error));
                }
                return Err(error);
            }
            _ => {}
        }
        if index > 0 {
            match ids[index - 1].canonical_order(&ids[index]) {
                Ordering::Equal => {
                    assert(!crate::model::id_members_valid(ids@, excluded));
                    let error = KnowledgeError::section(KnowledgeErrorKind::DuplicateValue, ids[index]);
                    assert(crate::model::id_collection_error(ids@, excluded, error)) by {
                        assert(crate::model::id_first_error(ids@, excluded, index as int, error));
                    }
                    return Err(error);
                }
                Ordering::Greater => {
                    assert(!crate::model::id_members_valid(ids@, excluded));
                    let error = KnowledgeError::section(KnowledgeErrorKind::NonCanonicalOrder, ids[index]);
                    assert(crate::model::id_collection_error(ids@, excluded, error)) by {
                        assert(crate::model::id_first_error(ids@, excluded, index as int, error));
                    }
                    return Err(error);
                }
                Ordering::Less => {}
            }
        }
        assert(crate::model::id_members_valid_through(ids@, excluded, index as nat + 1));
        index += 1;
    }
    Ok(())
}

} // verus!
