//! Exact production checks and lookups for reusable context admission.

use super::KnowledgeContextLink;

use crate::ContextErrorKind;
use core::cmp::Ordering;
use peritus_run_knowledge::{
    CandidateIdentity, DeltaDelivery, KnowledgeAuthority, KnowledgeSection, KnowledgeSectionId,
};

pub(super) const fn roles_match(
    left: peritus_role::HarnessRole,
    right: peritus_role::HarnessRole,
) -> bool {
    matches!(
        (left, right),
        (peritus_role::HarnessRole::Writer, peritus_role::HarnessRole::Writer)
            | (peritus_role::HarnessRole::Reviewer, peritus_role::HarnessRole::Reviewer)
            | (peritus_role::HarnessRole::Fixer, peritus_role::HarnessRole::Fixer)
            | (peritus_role::HarnessRole::Evaluator, peritus_role::HarnessRole::Evaluator)
            | (peritus_role::HarnessRole::Evolver, peritus_role::HarnessRole::Evolver)
    )
}
pub(super) fn candidates_match(left: &CandidateIdentity, right: &CandidateIdentity) -> bool {
    left.same_candidate(right) && left.checkpoint_sequence() == right.checkpoint_sequence()
}
pub(super) fn validate_link_order(links: &[KnowledgeContextLink]) -> Result<(), ContextErrorKind> {
    if links.len() < 2 {
        return Ok(());
    }
    let mut index = 1;
    while index < links.len() {
        let previous_id = links[index - 1].section_id();
        let current_id = links[index].section_id();
        let order = compare_section_ids(&previous_id, &current_id);
        match order {
            Ordering::Equal => {
                return Err(ContextErrorKind::DuplicateValue);
            }
            Ordering::Greater => {
                return Err(ContextErrorKind::NonCanonicalOrder);
            }
            Ordering::Less => {}
        }
        index += 1;
    }
    Ok(())
}
pub(super) fn find_section_index(
    sections: &[KnowledgeSection],
    id: KnowledgeSectionId,
) -> Option<usize> {
    let mut index = 0;
    while index < sections.len() {
        if sections[index].id().matches(&id) {
            return Some(index);
        }
        index += 1;
    }
    None
}
pub(super) fn find_link_index(
    links: &[KnowledgeContextLink],
    id: KnowledgeSectionId,
) -> Option<usize> {
    let mut index = 0;
    while index < links.len() {
        if links[index].section_id().matches(&id) {
            return Some(index);
        }
        index += 1;
    }
    None
}
pub(super) fn digest_bytes_match(
    left: &peritus_types::Sha256Digest,
    right: &peritus_types::Sha256Digest,
) -> bool {
    bytes_equal(left.as_bytes(), right.as_bytes())
}
pub(super) const fn delivery_matches_authority(
    delivery: DeltaDelivery,
    authority: KnowledgeAuthority,
) -> bool {
    matches!(
        (delivery, authority),
        (
            DeltaDelivery::ChangedFact | DeltaDelivery::CurrentReference,
            KnowledgeAuthority::Authoritative
        ) | (DeltaDelivery::Navigation, KnowledgeAuthority::NavigationOnly)
    )
}
fn compare_section_ids(left: &KnowledgeSectionId, right: &KnowledgeSectionId) -> Ordering {
    compare_bytes_from(left.as_bytes(), right.as_bytes(), 0)
}
fn bytes_equal<const N: usize>(left: &[u8; N], right: &[u8; N]) -> bool {
    let mut index = 0;
    while index < N {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}
fn compare_bytes_from<const N: usize>(left: &[u8; N], right: &[u8; N], index: usize) -> Ordering {
    if index == N {
        Ordering::Equal
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        compare_bytes_from(left, right, index + 1)
    }
}
