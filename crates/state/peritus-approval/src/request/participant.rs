//! Canonical bounded participant-set construction and membership.

use core::cmp::Ordering;
use peritus_types::ActorId;

const fn compare_actor_bytes_from(left: &[u8; 16], right: &[u8; 16], index: usize) -> Ordering {
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
    fn checked(
        values: Vec<ActorId>,
        collection: crate::CanonicalCollection,
        maximum: usize,
    ) -> Result<Self, crate::ApprovalError> {
        if values.len() > maximum {
            return Err(crate::ApprovalError::CollectionTooLarge(collection));
        }
        let mut index = 0;
        while index < values.len() {
            if index > 0 {
                match compare_actor_bytes(&values[index - 1], &values[index]) {
                    Ordering::Less => {}
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
    #[doc = " Validates producing-attempt participants in strict actor-byte order."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects over-limit, duplicate, or noncanonically ordered actor IDs."]
    pub fn producing(values: Vec<ActorId>) -> Result<Self, crate::ApprovalError> {
        Self::checked(
            values,
            crate::CanonicalCollection::ProducingParticipants,
            super::MAX_PRODUCING_PARTICIPANTS,
        )
    }
    #[doc = " Validates review participants in strict actor-byte order."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects over-limit, duplicate, or noncanonically ordered actor IDs."]
    pub fn review(values: Vec<ActorId>) -> Result<Self, crate::ApprovalError> {
        Self::checked(
            values,
            crate::CanonicalCollection::ReviewParticipants,
            super::MAX_REVIEW_PARTICIPANTS,
        )
    }
    #[doc = " Borrows canonical participant actors."]
    #[must_use]
    pub const fn as_slice(&self) -> &[ActorId] {
        self.values.as_slice()
    }
    #[doc = " Returns whether one actor occurs in the canonical set."]
    #[must_use]
    pub fn contains(&self, actor: ActorId) -> bool {
        let target = *actor.as_bytes();
        let mut index = 0;
        while index < self.values.len() {
            let candidate = *self.values[index].as_bytes();
            if crate::state::exact::identifier_bytes_equal(candidate, target) {
                return true;
            }
            index += 1;
        }
        false
    }
}
