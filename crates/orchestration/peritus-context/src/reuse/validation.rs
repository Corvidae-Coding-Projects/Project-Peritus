//! Exact production checks and lookups for reusable context admission.

use super::KnowledgeContextLink;
#[cfg(verus_only)]
use super::model;
use crate::ContextErrorKind;
use core::cmp::Ordering;
use peritus_run_knowledge::{
    CandidateIdentity, DeltaDelivery, KnowledgeAuthority, KnowledgeSection, KnowledgeSectionId,
};
use vstd::prelude::*;

verus! {

pub(super) const fn roles_match(
    left: peritus_role::HarnessRole,
    right: peritus_role::HarnessRole,
) -> (same: bool)
    ensures same == (left == right),
{
    matches!(
        (left, right),
        (peritus_role::HarnessRole::Writer, peritus_role::HarnessRole::Writer)
            | (peritus_role::HarnessRole::Reviewer, peritus_role::HarnessRole::Reviewer)
            | (peritus_role::HarnessRole::Fixer, peritus_role::HarnessRole::Fixer)
            | (peritus_role::HarnessRole::Evaluator, peritus_role::HarnessRole::Evaluator)
            | (peritus_role::HarnessRole::Evolver, peritus_role::HarnessRole::Evolver)
    )
}

pub(super) fn candidates_match(
    left: &CandidateIdentity,
    right: &CandidateIdentity,
) -> (same: bool)
    ensures same == peritus_run_knowledge::model::candidates_match(left, right),
{
    left.same_candidate(right) && left.checkpoint_sequence() == right.checkpoint_sequence()
}

pub(super) fn validate_link_order(
    links: &[KnowledgeContextLink],
) -> (result: Result<(), ContextErrorKind>)
    ensures
        result.is_ok() == model::first_link_order_error(links@, 1).is_none(),
        match result {
            Ok(()) => true,
            Err(kind) => model::first_link_order_error(links@, 1) == Some(kind),
        },
{
    if links.len() < 2 {
        assert(model::first_link_order_error(links@, 1).is_none());
        return Ok(());
    }
    let mut index = 1;
    while index < links.len()
        invariant
            1 <= index <= links.len(),
            model::first_link_order_error(links@, 1)
                == model::first_link_order_error(links@, index as nat),
        decreases links.len() - index,
    {
        let previous_id = links[index - 1].section_id();
        let current_id = links[index].section_id();
        let order = compare_section_ids(&previous_id, &current_id);
        assert(order == links@[index as int - 1].spec_section_id()
            .spec_order(&links@[index as int].spec_section_id()));
        match order {
            Ordering::Equal => {
                assert(model::first_link_order_error(links@, index as nat)
                    == Some(ContextErrorKind::DuplicateValue));
                return Err(ContextErrorKind::DuplicateValue);
            }
            Ordering::Greater => {
                assert(model::first_link_order_error(links@, index as nat)
                    == Some(ContextErrorKind::NonCanonicalOrder));
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
) -> (result: Option<usize>)
    ensures match result {
        Some(index) => index < sections.len()
            && model::first_section_index_from(sections@, id, 0) == Some(index as nat),
        None => model::first_section_index_from(sections@, id, 0).is_none(),
    },
{
    let mut index = 0;
    while index < sections.len()
        invariant
            index <= sections.len(),
            model::first_section_index_from(sections@, id, 0)
                == model::first_section_index_from(sections@, id, index as nat),
        decreases sections.len() - index,
    {
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
) -> (result: Option<usize>)
    ensures match result {
        Some(index) => index < links.len()
            && model::first_link_index_from(links@, id, 0) == Some(index as nat),
        None => model::first_link_index_from(links@, id, 0).is_none(),
    },
{
    let mut index = 0;
    while index < links.len()
        invariant
            index <= links.len(),
            model::first_link_index_from(links@, id, 0)
                == model::first_link_index_from(links@, id, index as nat),
        decreases links.len() - index,
    {
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
) -> (same: bool)
    ensures same == (left.spec_bytes() == right.spec_bytes()),
{
    bytes_equal(left.as_bytes(), right.as_bytes())
}

pub(super) const fn delivery_matches_authority(
    delivery: DeltaDelivery,
    authority: KnowledgeAuthority,
) -> (matches: bool)
    ensures matches == model::delivery_matches_authority(delivery, authority),
{
    matches!(
        (delivery, authority),
        (
            DeltaDelivery::ChangedFact | DeltaDelivery::CurrentReference,
            KnowledgeAuthority::Authoritative
        ) | (DeltaDelivery::Navigation, KnowledgeAuthority::NavigationOnly)
    )
}

fn compare_section_ids(
    left: &KnowledgeSectionId,
    right: &KnowledgeSectionId,
) -> (order: Ordering)
    ensures order == left.spec_order(right),
{
    compare_bytes_from(left.as_bytes(), right.as_bytes(), 0)
}

fn bytes_equal<const N: usize>(left: &[u8; N], right: &[u8; N]) -> (equal: bool)
    ensures equal == (*left == *right),
{
    let mut index = 0;
    while index < N
        invariant
            index <= N,
            forall |prior: int| 0 <= prior < index ==> left[prior] == right[prior],
        decreases N - index,
    {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    assert(*left =~= *right);
    true
}

fn compare_bytes_from<const N: usize>(
    left: &[u8; N],
    right: &[u8; N],
    index: usize,
) -> (order: Ordering)
    requires index <= N,
    ensures order == peritus_types::canonical_byte_order_from(left@, right@, index as nat),
    decreases N - index,
{
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

} // verus!
