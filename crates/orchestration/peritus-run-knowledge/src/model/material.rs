//! Exact material equality used by production delta-packet planning.

#[cfg(verus_only)]
use crate::{KnowledgeSection, KnowledgeSectionId, SourceDigest};
use vstd::prelude::*;

verus! {

/// Same ordered source identities and all corresponding content-digest bytes.
pub open spec fn sources_match(left: Seq<SourceDigest>, right: Seq<SourceDigest>) -> bool {
    left.len() == right.len()
        && forall |index: int| 0 <= index < left.len() ==>
            #[trigger] left[index].spec_matches(&right[index])
}

/// Same ordered dependency identities, including every identity byte.
pub open spec fn dependencies_match(
    left: Seq<KnowledgeSectionId>, right: Seq<KnowledgeSectionId>,
) -> bool {
    left.len() == right.len()
        && forall |index: int| 0 <= index < left.len() ==>
            #[trigger] left[index].spec_matches(&right[index])
}

/// Same semantic content for reuse after separate identity and freshness checks.
pub open spec fn section_material_matches(previous: &KnowledgeSection, current: &KnowledgeSection) -> bool {
    &&& previous.spec_kind() == current.spec_kind()
    &&& previous.spec_section_digest().spec_bytes() == current.spec_section_digest().spec_bytes()
    &&& sources_match(previous.spec_binding().spec_sources(), current.spec_binding().spec_sources())
    &&& dependencies_match(previous.spec_dependencies(), current.spec_dependencies())
}

/// Exact result at the first matching section identity in the supplied snapshot.
pub open spec fn first_prior_material_matches(
    sections: Seq<KnowledgeSection>, current: &KnowledgeSection,
) -> bool {
    exists |index: int| 0 <= index < sections.len()
        && sections[index].spec_id().spec_matches(&current.spec_id())
        && (forall |prior: int| 0 <= prior < index ==>
            !#[trigger] sections[prior].spec_id().spec_matches(&current.spec_id()))
        && section_material_matches(&sections[index], current)
}

} // verus!
