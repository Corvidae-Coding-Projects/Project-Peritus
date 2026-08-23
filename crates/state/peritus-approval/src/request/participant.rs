//! Canonical bounded participant-set construction and membership.

use core::cmp::Ordering;
use peritus_types::ActorId;
use vstd::prelude::*;

verus! {

const fn compare_actor_bytes_from(
    left: &[u8; 16],
    right: &[u8; 16],
    index: usize,
) -> (result: Ordering)
    requires index <= 16,
    decreases 16 - index,
{
    if index == 16 {
        Ordering::Equal
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        compare_actor_bytes_from(left, right, index + 1)
    }
}

const fn compare_actor_bytes(left: &ActorId, right: &ActorId) -> Ordering {
    compare_actor_bytes_from(left.as_bytes(), right.as_bytes(), 0)
}

impl super::ParticipantSet {
    /// Returns the exact canonical participant sequence.
    pub closed spec fn spec_values(&self) -> Seq<ActorId> { self.values@ }

    /// Returns exact participant membership.
    pub closed spec fn spec_contains(&self, actor: ActorId) -> bool {
        super::specification::contains(self.spec_values(), actor)
    }

    fn checked(
        values: Vec<ActorId>,
        collection: crate::CanonicalCollection,
        maximum: usize,
    ) -> Result<Self, crate::ApprovalError> {
        if values.len() > maximum {
            return Err(crate::ApprovalError::CollectionTooLarge(collection));
        }
        let mut index = 0;
        while index < values.len()
            invariant 0 <= index <= values.len(),
            decreases values.len() - index,
        {
            if index > 0 {
                match compare_actor_bytes(&values[index - 1], &values[index]) {
                    Ordering::Less => {},
                    Ordering::Equal => {
                        return Err(crate::ApprovalError::DuplicateCanonicalValue(collection));
                    }
                    Ordering::Greater => {
                        return Err(crate::ApprovalError::NonCanonicalOrder(collection));
                    }
                }
            }
            index += 1;
        }
        Ok(Self { values })
    }

    /// Validates producing-attempt participants in strict actor-byte order.
    ///
    /// # Errors
    ///
    /// Rejects over-limit, duplicate, or noncanonically ordered actor IDs.
    pub fn producing(values: Vec<ActorId>) -> Result<Self, crate::ApprovalError> {
        Self::checked(
            values,
            crate::CanonicalCollection::ProducingParticipants,
            super::MAX_PRODUCING_PARTICIPANTS,
        )
    }

    /// Validates review participants in strict actor-byte order.
    ///
    /// # Errors
    ///
    /// Rejects over-limit, duplicate, or noncanonically ordered actor IDs.
    pub fn review(values: Vec<ActorId>) -> Result<Self, crate::ApprovalError> {
        Self::checked(
            values,
            crate::CanonicalCollection::ReviewParticipants,
            super::MAX_REVIEW_PARTICIPANTS,
        )
    }

    /// Borrows canonical participant actors.
    #[must_use]
    pub const fn as_slice(&self) -> &[ActorId] { self.values.as_slice() }

    /// Returns whether one actor occurs in the canonical set.
    #[must_use]
    pub fn contains(&self, actor: ActorId) -> (result: bool)
        ensures result == self.spec_contains(actor),
    {
        proof {
            reveal(super::ParticipantSet::spec_contains);
            reveal(super::ParticipantSet::spec_values);
        }
        let target = *actor.as_bytes();
        let mut index = 0;
        while index < self.values.len()
            invariant
                0 <= index <= self.values.len(),
                target == actor.spec_bytes(),
                forall |prior: int| 0 <= prior < index ==>
                    !#[trigger] crate::state::exact::same_identifier_from(
                        self.values@[prior].spec_bytes(),
                        actor.spec_bytes(),
                        0,
                    ),
            decreases self.values.len() - index,
        {
            let candidate = *self.values[index].as_bytes();
            if crate::state::exact::identifier_bytes_equal(candidate, target) {
                assert(super::specification::contains(self.values@, actor)) by {
                    assert(exists |found: int| found == index
                        && 0 <= found < self.values@.len()
                        && #[trigger] crate::state::exact::same_identifier_from(
                            self.values@[found].spec_bytes(),
                            actor.spec_bytes(),
                            0,
                        ));
                }
                return true;
            }
            index += 1;
        }
        false
    }
}

} // verus!
