//! Complete executable material comparisons for delta-packet reuse.

use crate::{KnowledgeSection, KnowledgeSectionId, SourceDigest};
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

pub(super) fn candidates_match(left: &CandidateIdentity, right: &CandidateIdentity) -> (same: bool)
    ensures same == crate::model::candidates_match(left, right),
{
    left.same_candidate(right) && left.checkpoint_sequence() == right.checkpoint_sequence()
}

pub(super) fn same_material(previous: &KnowledgeSection, current: &KnowledgeSection) -> (same: bool)
    ensures same == crate::model::section_material_matches(previous, current),
{
    previous.kind().matches(current.kind())
        && crate::identity::bytes_equal(
            previous.section_digest().as_bytes(), current.section_digest().as_bytes())
        && sources_match(previous.binding().sources(), current.binding().sources())
        && dependencies_match(previous.dependencies(), current.dependencies())
}

fn sources_match(left: &[SourceDigest], right: &[SourceDigest]) -> (same: bool)
    ensures same == crate::model::sources_match(left@, right@),
{
    if left.len() != right.len() { return false; }
    let mut index = 0;
    while index < left.len()
        invariant
            left.len() == right.len(),
            index <= left.len(),
            forall |prior: int| 0 <= prior < index ==> left@[prior].spec_matches(&right@[prior]),
        decreases left.len() - index,
    {
        if !left[index].matches(&right[index]) { return false; }
        index += 1;
    }
    true
}

fn dependencies_match(left: &[KnowledgeSectionId], right: &[KnowledgeSectionId]) -> (same: bool)
    ensures same == crate::model::dependencies_match(left@, right@),
{
    if left.len() != right.len() { return false; }
    let mut index = 0;
    while index < left.len()
        invariant
            left.len() == right.len(),
            index <= left.len(),
            forall |prior: int| 0 <= prior < index ==> left@[prior].spec_matches(&right@[prior]),
        decreases left.len() - index,
    {
        if !left[index].matches(&right[index]) { return false; }
        index += 1;
    }
    true
}

} // verus!
