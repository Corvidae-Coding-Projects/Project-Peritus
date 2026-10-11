//! Complete executable material comparisons for delta-packet reuse.

use crate::{KnowledgeSection, KnowledgeSectionId, SourceDigest};
use peritus_run_settlement::CandidateIdentity;

pub(super) fn candidates_match(left: &CandidateIdentity, right: &CandidateIdentity) -> bool {
    left.same_candidate(right) && left.checkpoint_sequence() == right.checkpoint_sequence()
}
pub(super) fn same_material(previous: &KnowledgeSection, current: &KnowledgeSection) -> bool {
    previous.kind().matches(current.kind())
        && crate::identity::bytes_equal(
            previous.section_digest().as_bytes(),
            current.section_digest().as_bytes(),
        )
        && sources_match(previous.binding().sources(), current.binding().sources())
        && dependencies_match(previous.dependencies(), current.dependencies())
}
fn sources_match(left: &[SourceDigest], right: &[SourceDigest]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if !left[index].matches(&right[index]) {
            return false;
        }
        index += 1;
    }
    true
}
fn dependencies_match(left: &[KnowledgeSectionId], right: &[KnowledgeSectionId]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if !left[index].matches(&right[index]) {
            return false;
        }
        index += 1;
    }
    true
}
