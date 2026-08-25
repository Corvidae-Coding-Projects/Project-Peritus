//! Immutable reviewer assignments and one-shot cycle submissions.

use peritus_context::ContextPlanId;
use peritus_quality_policy::{ReviewCycleOrdinal, ReviewerIdentity};
use peritus_role::ReviewIndependenceView;
use peritus_spec::ReviewCategory;
use peritus_types::{ReviewCycleId, RevisionTuple, Sha256Digest};

use crate::binding::canonical_nonempty;
use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::{ReviewBinding, ReviewLimits, ReviewSubmission};

/// Immutable exact assignment of one reviewer to one review cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewAssignment {
    cycle_id: ReviewCycleId,
    ordinal: ReviewCycleOrdinal,
    binding_digest: Sha256Digest,
    revision: RevisionTuple,
    reviewer: ReviewerIdentity,
    categories: Vec<ReviewCategory>,
    context_plan_id: ContextPlanId,
    fresh_context: bool,
    independence: ReviewIndependenceView,
}

impl ReviewAssignment {
    /// Creates a checked assignment under the exact current binding.
    ///
    /// # Errors
    /// Rejects empty/noncanonical/out-of-contract categories, stale bindings, or ordinals above
    /// either the contract or caller cap.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cycle_id: ReviewCycleId,
        ordinal: ReviewCycleOrdinal,
        binding: &ReviewBinding,
        reviewer: ReviewerIdentity,
        categories: Vec<ReviewCategory>,
        context_plan_id: ContextPlanId,
        fresh_context: bool,
        limits: ReviewLimits,
    ) -> Result<Self, ReviewError> {
        let assignment = Self::from_wire(
            cycle_id,
            ordinal,
            binding.digest(),
            binding.revision(),
            reviewer,
            categories,
            context_plan_id,
            fresh_context,
            binding.independence(),
        );
        assignment.validate(binding, limits)?;
        Ok(assignment)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        cycle_id: ReviewCycleId,
        ordinal: ReviewCycleOrdinal,
        binding_digest: Sha256Digest,
        revision: RevisionTuple,
        reviewer: ReviewerIdentity,
        categories: Vec<ReviewCategory>,
        context_plan_id: ContextPlanId,
        fresh_context: bool,
        independence: ReviewIndependenceView,
    ) -> Self {
        Self {
            cycle_id,
            ordinal,
            binding_digest,
            revision,
            reviewer,
            categories,
            context_plan_id,
            fresh_context,
            independence,
        }
    }

    /// Returns the stable cycle identity.
    #[must_use]
    pub const fn cycle_id(&self) -> ReviewCycleId {
        self.cycle_id
    }
    /// Returns the one-based aggregate cycle ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> ReviewCycleOrdinal {
        self.ordinal
    }
    /// Returns the immutable binding digest.
    #[must_use]
    pub const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }
    /// Returns the exact assigned revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns reviewer identity and provenance facts.
    #[must_use]
    pub const fn reviewer(&self) -> &ReviewerIdentity {
        &self.reviewer
    }
    /// Returns assigned categories in canonical order.
    #[must_use]
    pub const fn categories(&self) -> &[ReviewCategory] {
        self.categories.as_slice()
    }
    /// Returns the exact caller-computed C6 plan identity.
    #[must_use]
    pub const fn context_plan_id(&self) -> ContextPlanId {
        self.context_plan_id
    }
    /// Returns the independently attested fresh-context fact.
    #[must_use]
    pub const fn fresh_context(&self) -> bool {
        self.fresh_context
    }
    /// Returns the complete copied C6/B2 independence policy view.
    #[must_use]
    pub const fn independence(&self) -> ReviewIndependenceView {
        self.independence
    }

    pub(super) fn validate(
        &self,
        binding: &ReviewBinding,
        limits: ReviewLimits,
    ) -> Result<(), ReviewError> {
        if self.binding_digest != binding.digest()
            || self.revision != binding.revision()
            || self.independence != binding.independence()
            || self.ordinal.get() > binding.maximum_cycles()
            || self.ordinal.get() > limits.cycles()
            || self.categories.len() > usize::from(limits.categories())
            || self.reviewer.context() != self.context_plan_id.digest()
        {
            return Err(reject(
                ReviewErrorKind::BindingMismatch,
                "assignment differs from the current review binding or limits",
            ));
        }
        canonical_nonempty(&self.categories, "assignment categories are not canonical")?;
        if self
            .categories
            .iter()
            .any(|category| binding.required_categories().binary_search(category).is_err())
        {
            return Err(reject(
                ReviewErrorKind::InvalidInput,
                "assignment contains a category absent from the contract",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_inert(
        &self,
        binding: &ReviewBinding,
        limits: ReviewLimits,
    ) -> Result<(), ReviewError> {
        if self.independence != binding.independence()
            || self.ordinal.get() > binding.maximum_cycles()
            || self.ordinal.get() > limits.cycles()
            || self.categories.len() > usize::from(limits.categories())
            || self.reviewer.context() != self.context_plan_id.digest()
        {
            return Err(reject(
                ReviewErrorKind::BindingMismatch,
                "decoded assignment policy, context, ordinal, or bounds differ",
            ));
        }
        canonical_nonempty(&self.categories, "assignment categories are not canonical")?;
        if self
            .categories
            .iter()
            .any(|category| binding.required_categories().binary_search(category).is_err())
        {
            return Err(reject(
                ReviewErrorKind::InvalidInput,
                "assignment contains a category absent from the contract",
            ));
        }
        Ok(())
    }
}

/// Closed lifecycle of one reviewer assignment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewCyclePhase {
    /// Assigned and eligible for exactly one submission.
    Assigned,
    /// One complete structured submission was accepted.
    Submitted,
    /// Explicitly cancelled before submission.
    Cancelled,
    /// Historical because a candidate freshness component changed.
    Invalidated,
}

/// One retained assignment and optional one-shot submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewCycle {
    assignment: ReviewAssignment,
    pub(super) phase: ReviewCyclePhase,
    pub(super) submission: Option<ReviewSubmission>,
}

impl ReviewCycle {
    pub(super) const fn assigned(assignment: ReviewAssignment) -> Self {
        Self { assignment, phase: ReviewCyclePhase::Assigned, submission: None }
    }

    pub(super) const fn from_wire(
        assignment: ReviewAssignment,
        phase: ReviewCyclePhase,
        submission: Option<ReviewSubmission>,
    ) -> Self {
        Self { assignment, phase, submission }
    }

    /// Returns the immutable assignment.
    #[must_use]
    pub const fn assignment(&self) -> &ReviewAssignment {
        &self.assignment
    }
    /// Returns the stable cycle identity.
    #[must_use]
    pub const fn id(&self) -> ReviewCycleId {
        self.assignment.cycle_id
    }
    /// Returns the one-based cycle ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> ReviewCycleOrdinal {
        self.assignment.ordinal
    }
    /// Returns the current closed phase.
    #[must_use]
    pub const fn phase(&self) -> ReviewCyclePhase {
        self.phase
    }
    /// Borrows the accepted submission, if any.
    #[must_use]
    pub const fn submission(&self) -> Option<&ReviewSubmission> {
        self.submission.as_ref()
    }

    pub(super) fn validate_inert(
        &self,
        binding: &ReviewBinding,
        limits: ReviewLimits,
    ) -> Result<(), ReviewError> {
        self.assignment.validate_inert(binding, limits)?;
        match (self.phase, &self.submission) {
            (
                ReviewCyclePhase::Assigned
                | ReviewCyclePhase::Cancelled
                | ReviewCyclePhase::Invalidated,
                None,
            ) => Ok(()),
            (ReviewCyclePhase::Submitted | ReviewCyclePhase::Invalidated, Some(submission)) => {
                submission.validate(binding.blocking_severity(), limits)?;
                if submission.cycle_id() == self.id()
                    && submission.revision() == self.assignment.revision
                    && submission.reviewer_matches(&self.assignment.reviewer)
                {
                    Ok(())
                } else {
                    Err(reject(
                        ReviewErrorKind::BindingMismatch,
                        "decoded cycle submission differs from its assignment",
                    ))
                }
            }
            _ => Err(reject(
                ReviewErrorKind::IllegalTransition,
                "decoded cycle phase and submission presence are contradictory",
            )),
        }
    }
}
