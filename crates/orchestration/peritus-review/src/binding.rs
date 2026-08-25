//! Immutable B2 contract and exact candidate-revision binding.

use peritus_role::ReviewIndependenceView;
use peritus_spec::{AcceptanceContract, FindingSeverity, ReviewCategory, WaiverPolicy};
use peritus_types::{AcceptanceSpecId, ActorId, RevisionTuple, Sha256Digest};

use crate::ReviewLimits;
use crate::error::{ReviewError, ReviewErrorKind, reject};

/// Complete immutable contract and candidate identity used by every D2 transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewBinding {
    contract_id: AcceptanceSpecId,
    contract_digest: Sha256Digest,
    revision: RevisionTuple,
    required_categories: Vec<ReviewCategory>,
    reviewer_quorum: u16,
    independence: ReviewIndependenceView,
    blocking_severity: FindingSeverity,
    maximum_cycles: u16,
    waiver_policy: WaiverPolicy,
    candidate_digest: Sha256Digest,
    tree_digest: Sha256Digest,
    producer_actors: Vec<ActorId>,
    producer_ancestries: Vec<Sha256Digest>,
    digest: Sha256Digest,
}

impl ReviewBinding {
    /// Binds the checked contract and copies its complete review/waiver policy snapshot.
    ///
    /// # Errors
    /// Rejects a tuple naming another contract, an unrepresentable contract policy, or empty,
    /// duplicated, or noncanonical producer identity/ancestry sets.
    pub fn from_contract(
        contract: &AcceptanceContract,
        revision: RevisionTuple,
        candidate_digest: Sha256Digest,
        tree_digest: Sha256Digest,
        producer_actors: Vec<ActorId>,
        producer_ancestries: Vec<Sha256Digest>,
        limits: ReviewLimits,
    ) -> Result<Self, ReviewError> {
        let bound = contract.bind(revision).map_err(|_| {
            ReviewError::new(
                ReviewErrorKind::BindingMismatch,
                crate::ReviewRecoveryAction::CorrectInput,
                "acceptance contract does not bind the requested revision tuple",
            )
        })?;
        let policy = contract.review_policy();
        if policy.required_categories().len() > usize::from(limits.categories())
            || contract.completion_policy().max_review_cycles() > limits.cycles()
            || contract.completion_policy().max_review_cycles() > limits.assignments()
        {
            return Err(reject(
                ReviewErrorKind::LimitExceeded,
                "contract review policy exceeds the selected D2 limits",
            ));
        }
        canonical_nonempty(&producer_actors, "producer actor identities are not canonical")?;
        canonical_nonempty(&producer_ancestries, "producer ancestries are not canonical")?;
        let mut binding = Self::from_wire(
            bound.contract_id(),
            bound.contract_digest(),
            bound.revision(),
            policy.required_categories().to_vec(),
            policy.reviewer_quorum(),
            ReviewIndependenceView::from_contract(policy.independence()),
            policy.blocking_severity(),
            contract.completion_policy().max_review_cycles(),
            contract.waiver_policy(),
            candidate_digest,
            tree_digest,
            producer_actors,
            producer_ancestries,
            Sha256Digest::new([0; 32]),
        );
        binding.digest = crate::canonical::binding_digest(&binding);
        Ok(binding)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        contract_id: AcceptanceSpecId,
        contract_digest: Sha256Digest,
        revision: RevisionTuple,
        required_categories: Vec<ReviewCategory>,
        reviewer_quorum: u16,
        independence: ReviewIndependenceView,
        blocking_severity: FindingSeverity,
        maximum_cycles: u16,
        waiver_policy: WaiverPolicy,
        candidate_digest: Sha256Digest,
        tree_digest: Sha256Digest,
        producer_actors: Vec<ActorId>,
        producer_ancestries: Vec<Sha256Digest>,
        digest: Sha256Digest,
    ) -> Self {
        Self {
            contract_id,
            contract_digest,
            revision,
            required_categories,
            reviewer_quorum,
            independence,
            blocking_severity,
            maximum_cycles,
            waiver_policy,
            candidate_digest,
            tree_digest,
            producer_actors,
            producer_ancestries,
            digest,
        }
    }

    /// Returns the checked B2 contract identity copied from `AcceptanceContract::bind`.
    #[must_use]
    pub const fn contract_id(&self) -> AcceptanceSpecId {
        self.contract_id
    }
    /// Returns the immutable B2 contract digest copied from `AcceptanceContract::bind`.
    #[must_use]
    pub const fn contract_digest(&self) -> Sha256Digest {
        self.contract_digest
    }
    /// Returns the exact revision tuple.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns required review categories in canonical order.
    #[must_use]
    pub const fn required_categories(&self) -> &[ReviewCategory] {
        self.required_categories.as_slice()
    }
    /// Returns the current submitted-review count threshold.
    #[must_use]
    pub const fn reviewer_quorum(&self) -> u16 {
        self.reviewer_quorum
    }
    /// Returns all independent C6/B2 quorum requirements.
    #[must_use]
    pub const fn independence(&self) -> ReviewIndependenceView {
        self.independence
    }
    /// Returns the lowest blocking severity.
    #[must_use]
    pub const fn blocking_severity(&self) -> FindingSeverity {
        self.blocking_severity
    }
    /// Returns the immutable contract cycle cap.
    #[must_use]
    pub const fn maximum_cycles(&self) -> u16 {
        self.maximum_cycles
    }
    /// Returns the immutable external waiver declaration.
    #[must_use]
    pub const fn waiver_policy(&self) -> WaiverPolicy {
        self.waiver_policy
    }
    /// Returns the exact candidate digest.
    #[must_use]
    pub const fn candidate_digest(&self) -> Sha256Digest {
        self.candidate_digest
    }
    /// Returns the exact candidate tree digest.
    #[must_use]
    pub const fn tree_digest(&self) -> Sha256Digest {
        self.tree_digest
    }
    /// Returns canonical producing actor identities.
    #[must_use]
    pub const fn producer_actors(&self) -> &[ActorId] {
        self.producer_actors.as_slice()
    }
    /// Returns canonical producer ancestry digests.
    #[must_use]
    pub const fn producer_ancestries(&self) -> &[Sha256Digest] {
        self.producer_ancestries.as_slice()
    }
    /// Returns the canonical complete binding digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns whether every immutable freshness component is exactly equal.
    #[must_use]
    pub fn same_candidate(&self, other: &Self) -> bool {
        self.revision() == other.revision()
            && self.candidate_digest == other.candidate_digest
            && self.tree_digest == other.tree_digest
    }

    pub(super) fn validate(&self, limits: ReviewLimits) -> Result<(), ReviewError> {
        if self.contract_id != self.revision.acceptance_spec_id()
            || self.required_categories.is_empty()
            || self.required_categories.len() > usize::from(limits.categories())
            || self.reviewer_quorum == 0
            || self.maximum_cycles == 0
            || self.maximum_cycles > limits.cycles()
            || self.maximum_cycles > limits.assignments()
        {
            return Err(reject(
                ReviewErrorKind::BindingMismatch,
                "decoded review binding contains invalid policy bounds",
            ));
        }
        canonical(&self.required_categories, "binding categories are not canonical")?;
        canonical_nonempty(&self.producer_actors, "producer actor identities are not canonical")?;
        canonical_nonempty(
            &self.producer_ancestries,
            "producer ancestry identities are not canonical",
        )?;
        if crate::canonical::binding_digest(self) != self.digest {
            return Err(reject(
                ReviewErrorKind::BindingMismatch,
                "binding digest differs from its canonical fields",
            ));
        }
        Ok(())
    }
}

pub fn canonical<T: Ord>(values: &[T], detail: &'static str) -> Result<(), ReviewError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        Err(reject(ReviewErrorKind::NonCanonical, detail))
    } else {
        Ok(())
    }
}

pub fn canonical_nonempty<T: Ord>(values: &[T], detail: &'static str) -> Result<(), ReviewError> {
    if values.is_empty() {
        Err(reject(ReviewErrorKind::InvalidInput, detail))
    } else {
        canonical(values, detail)
    }
}
