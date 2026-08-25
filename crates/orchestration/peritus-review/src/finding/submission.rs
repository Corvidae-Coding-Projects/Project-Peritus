//! Checked structured submission aggregate.

use peritus_quality_policy::ReviewerIdentity;
use peritus_spec::{FindingSeverity, ReviewCategory};
use peritus_types::{ReviewCycleId, RevisionTuple, Sha256Digest};

use super::Finding;
use crate::ReviewLimits;
use crate::binding::canonical;
use crate::error::{ReviewError, ReviewErrorKind, reject};

/// One completely validated structured reviewer submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewSubmission {
    cycle_id: ReviewCycleId,
    revision: RevisionTuple,
    categories: Vec<ReviewCategory>,
    findings: Vec<Finding>,
    review_digest: Sha256Digest,
}

impl ReviewSubmission {
    /// Creates a bounded canonical submission and checks its normalized digest.
    ///
    /// # Errors
    /// Rejects empty/duplicate categories, duplicate finding identities, stale findings, or an
    /// incorrect canonical submission digest.
    pub fn new(
        cycle_id: ReviewCycleId,
        revision: RevisionTuple,
        categories: Vec<ReviewCategory>,
        findings: Vec<Finding>,
        blocking_severity: FindingSeverity,
        limits: ReviewLimits,
    ) -> Result<Self, ReviewError> {
        let mut submission =
            Self::from_wire(cycle_id, revision, categories, findings, Sha256Digest::new([0; 32]));
        submission.review_digest = crate::canonical::submission_digest(&submission);
        submission.validate(blocking_severity, limits)?;
        Ok(submission)
    }

    pub(crate) const fn from_wire(
        cycle_id: ReviewCycleId,
        revision: RevisionTuple,
        categories: Vec<ReviewCategory>,
        findings: Vec<Finding>,
        review_digest: Sha256Digest,
    ) -> Self {
        Self { cycle_id, revision, categories, findings, review_digest }
    }

    /// Returns the assigned cycle identity.
    #[must_use]
    pub const fn cycle_id(&self) -> ReviewCycleId {
        self.cycle_id
    }

    /// Returns the exact reviewed revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }

    /// Returns submitted categories in canonical order.
    #[must_use]
    pub const fn categories(&self) -> &[ReviewCategory] {
        self.categories.as_slice()
    }

    /// Returns findings in canonical identity order.
    #[must_use]
    pub const fn findings(&self) -> &[Finding] {
        self.findings.as_slice()
    }

    /// Returns the canonical normalized review digest.
    #[must_use]
    pub const fn review_digest(&self) -> Sha256Digest {
        self.review_digest
    }

    pub(crate) fn validate(
        &self,
        blocking_severity: FindingSeverity,
        limits: ReviewLimits,
    ) -> Result<(), ReviewError> {
        if self.categories.is_empty()
            || self.categories.len() > usize::from(limits.categories())
            || self.findings.len() > limits.findings() as usize
        {
            return Err(reject(
                ReviewErrorKind::LimitExceeded,
                "submission category or finding bounds are invalid",
            ));
        }
        canonical(&self.categories, "submission categories are not canonical")?;
        if self.findings.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
            return Err(reject(
                ReviewErrorKind::NonCanonical,
                "submission findings are duplicated or not in identity order",
            ));
        }
        for finding in &self.findings {
            finding.validate(blocking_severity, limits)?;
            if finding.revision != self.revision
                || finding.origin.cycle_id != self.cycle_id
                || self.categories.binary_search(&finding.category).is_err()
                || finding.sources.as_slice() != [finding.origin]
                || !finding.dispositions.is_empty()
                || finding.superseded_by.is_some()
            {
                return Err(reject(
                    ReviewErrorKind::BindingMismatch,
                    "submission finding differs from its cycle/body or contains fabricated history",
                ));
            }
        }
        if crate::canonical::submission_digest(self) != self.review_digest {
            return Err(reject(
                ReviewErrorKind::InvalidInput,
                "review digest differs from the normalized submission",
            ));
        }
        Ok(())
    }

    pub(crate) fn reviewer_matches(&self, reviewer: &ReviewerIdentity) -> bool {
        self.findings.iter().all(|finding| finding.origin.reviewer == reviewer.actor_id())
    }
}
