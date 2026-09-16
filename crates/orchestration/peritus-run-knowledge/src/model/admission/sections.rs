//! Exact canonical section-identity collection admission.

#[cfg(verus_only)]
use crate::{KnowledgeError, KnowledgeErrorKind, KnowledgeSectionId};
#[cfg(verus_only)]
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// Every examined identity avoids the optional excluded section and is strictly ordered.
pub open spec fn id_members_valid_through(
    ids: Seq<KnowledgeSectionId>, excluded: Option<KnowledgeSectionId>, end: nat,
) -> bool {
    end <= ids.len()
        && (forall |index: int| #![trigger ids[index]] 0 <= index < end ==> match excluded {
            Some(id) => !ids[index].spec_matches(&id), None => true,
        })
        && (forall |index: int| 1 <= index < end ==>
            #[trigger] ids[index - 1].spec_order(&ids[index]) == Ordering::Less)
}

/// Exact content admission independent of the caller's size and change-shape checks.
pub open spec fn id_members_valid(
    ids: Seq<KnowledgeSectionId>, excluded: Option<KnowledgeSectionId>,
) -> bool { id_members_valid_through(ids, excluded, ids.len()) }

/// The first rejected position, preserving self-reference before pair-order precedence.
pub open spec fn id_first_error(
    ids: Seq<KnowledgeSectionId>, excluded: Option<KnowledgeSectionId>, index: int,
    error: KnowledgeError,
) -> bool {
    0 <= index < ids.len() && id_members_valid_through(ids, excluded, index as nat)
        && if excluded.is_some() && ids[index].spec_matches(&excluded.unwrap()) {
            error.spec_section(KnowledgeErrorKind::SelfDependency, excluded.unwrap())
        } else {
            index > 0 && match ids[index - 1].spec_order(&ids[index]) {
                Ordering::Equal => error.spec_section(KnowledgeErrorKind::DuplicateValue, ids[index]),
                Ordering::Greater => error.spec_section(KnowledgeErrorKind::NonCanonicalOrder, ids[index]),
                Ordering::Less => false,
            }
        }
}

/// Exact first rejected member and its complete error detail.
pub open spec fn id_collection_error(
    ids: Seq<KnowledgeSectionId>, excluded: Option<KnowledgeSectionId>, error: KnowledgeError,
) -> bool {
    exists |index: int| #[trigger] id_first_error(ids, excluded, index, error)
}

} // verus!
