//! Independent review observations.

use crate::{ConstructionError, EvidenceBinding, PrincipalId, ReviewId};
use peritus_types::Sha256Digest;

#[doc = " Independent review outcome."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewOutcome {
    #[doc = " Reviewer approves the exact candidate for production evaluation."]
    Approved,
    #[doc = " Reviewer requires changes before production evaluation."]
    ChangesRequired,
}
#[doc = " One signed independent review observation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReviewObservation {
    id: ReviewId,
    binding: EvidenceBinding,
    reviewer: PrincipalId,
    producer: PrincipalId,
    context_digest: Sha256Digest,
    review_digest: Sha256Digest,
    outcome: ReviewOutcome,
    independent_from_producer: bool,
}
impl ReviewObservation {
    #[doc = " Creates one signed review observation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for a placeholder context or review digest."]
    #[allow(
        clippy::too_many_arguments,
        reason = "review identity, independence, and outcome stay explicit"
    )]
    pub fn new(
        id: ReviewId,
        binding: EvidenceBinding,
        reviewer: PrincipalId,
        producer: PrincipalId,
        context_digest: Sha256Digest,
        review_digest: Sha256Digest,
        outcome: ReviewOutcome,
        independent_from_producer: bool,
    ) -> Result<Self, ConstructionError> {
        crate::validation::require_digest(context_digest)?;
        crate::validation::require_digest(review_digest)?;
        Ok(Self {
            id,
            binding,
            reviewer,
            producer,
            context_digest,
            review_digest,
            outcome,
            independent_from_producer,
        })
    }
    #[doc = " Returns the stable review identity."]
    #[must_use]
    pub const fn id(&self) -> ReviewId {
        self.id
    }
    #[doc = " Returns the exact evidence binding."]
    #[must_use]
    pub const fn binding(&self) -> EvidenceBinding {
        self.binding
    }
    #[doc = " Returns the reviewer identity."]
    #[must_use]
    pub const fn reviewer(&self) -> PrincipalId {
        self.reviewer
    }
    #[doc = " Returns the candidate producer identity."]
    #[must_use]
    pub const fn producer(&self) -> PrincipalId {
        self.producer
    }
    #[doc = " Returns the fresh-context digest."]
    #[must_use]
    pub const fn context_digest(&self) -> Sha256Digest {
        self.context_digest
    }
    #[doc = " Returns the signed review digest."]
    #[must_use]
    pub const fn review_digest(&self) -> Sha256Digest {
        self.review_digest
    }
    #[doc = " Returns the explicit review outcome."]
    #[must_use]
    pub const fn outcome(&self) -> ReviewOutcome {
        self.outcome
    }
    #[doc = " Returns the independently attested producer-separation status."]
    #[must_use]
    pub const fn independent_from_producer(&self) -> bool {
        self.independent_from_producer
    }
}
#[doc = " Executable equality of every supplied review field."]
pub const fn reviews_equal(left: &ReviewObservation, right: &ReviewObservation) -> bool {
    crate::identity::review_ids_equal(left.id(), right.id())
        && crate::evidence::bindings_equal(&left.binding(), &right.binding())
        && crate::identity::principal_ids_equal(left.reviewer(), right.reviewer())
        && crate::identity::principal_ids_equal(left.producer(), right.producer())
        && crate::candidate::equality::digests_equal(left.context_digest(), right.context_digest())
        && crate::candidate::equality::digests_equal(left.review_digest(), right.review_digest())
        && review_outcomes_equal(left.outcome(), right.outcome())
        && left.independent_from_producer() == right.independent_from_producer()
}
#[doc = " Executable equality for independent-review outcomes."]
pub const fn review_outcomes_equal(left: ReviewOutcome, right: ReviewOutcome) -> bool {
    matches!(
        (left, right),
        (ReviewOutcome::Approved, ReviewOutcome::Approved)
            | (ReviewOutcome::ChangesRequired, ReviewOutcome::ChangesRequired)
    )
}
