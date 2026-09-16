//! Input-defined source collection admission and exact failure precedence.

#[cfg(verus_only)]
use crate::{KnowledgeError, KnowledgeErrorKind, SourceDigest};
#[cfg(verus_only)]
use core::cmp::Ordering;
mod sections;
mod snapshot;
#[cfg(verus_only)]
pub use snapshot::{binding_valid, section_bindings_valid_through, sections_valid,
    sections_valid_through, section_order_error, section_binding_error,
    section_dependency_error_at, section_dependency_error, section_validation_error_at,
    sections_validation_error};
#[cfg(verus_only)]
pub use sections::{id_members_valid, id_members_valid_through, id_first_error, id_collection_error};

use vstd::prelude::*;

verus! {

/// Every adjacent complete source identity is in strict canonical order.
pub open spec fn sources_canonical(sources: Seq<SourceDigest>) -> bool {
    sources_canonical_through(sources, sources.len())
}

/// Canonical ordering of the already examined prefix.
pub open spec fn sources_canonical_through(sources: Seq<SourceDigest>, end: nat) -> bool {
    end <= sources.len() && forall |index: int| 1 <= index < end ==>
        #[trigger] sources[index - 1].spec_source_id().spec_order(&sources[index].spec_source_id())
            == Ordering::Less
}

/// Exact admission rule, including the caller's allocation and emptiness policy.
pub open spec fn sources_admitted(
    sources: Seq<SourceDigest>, maximum: usize, allow_empty: bool,
) -> bool {
    (allow_empty || sources.len() > 0) && sources.len() <= maximum && sources_canonical(sources)
}

/// Exact identity detail and category at the first noncanonical adjacent pair.
pub open spec fn sources_first_bad_pair(
    sources: Seq<SourceDigest>, index: int, error: KnowledgeError,
) -> bool {
    1 <= index < sources.len() && sources_canonical_through(sources, index as nat)
        && match sources[index - 1].spec_source_id().spec_order(&sources[index].spec_source_id()) {
            Ordering::Equal => error.spec_source(
                KnowledgeErrorKind::DuplicateValue, sources[index].spec_source_id()),
            Ordering::Greater => error.spec_source(
                KnowledgeErrorKind::NonCanonicalOrder, sources[index].spec_source_id()),
            Ordering::Less => false,
        }
}

/// Exact error precedence: required emptiness, maximum length, then first bad pair.
pub open spec fn sources_validation_error(
    sources: Seq<SourceDigest>, maximum: usize, allow_empty: bool, error: KnowledgeError,
) -> bool {
    if !allow_empty && sources.len() == 0 {
        error.spec_plain(KnowledgeErrorKind::EmptyCollection)
    } else if sources.len() > maximum {
        error.spec_numbers(KnowledgeErrorKind::LimitExceeded, maximum as u64, sources.len() as u64)
    } else {
        exists |index: int| #[trigger] sources_first_bad_pair(sources, index, error)
    }
}

} // verus!
