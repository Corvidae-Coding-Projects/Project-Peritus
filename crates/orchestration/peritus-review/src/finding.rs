//! Checked structured review submissions and complete finding history.

mod location;
mod submission;

use peritus_evidence::EvidenceId;
use peritus_spec::{FindingSeverity, RequirementId, ReviewCategory};
use peritus_types::{ActorId, FindingId, ReviewCycleId, RevisionTuple, Sha256Digest};

use crate::binding::{canonical, canonical_nonempty};
use crate::disposition::{DispositionKind, DispositionRecord, validate_evidence};
use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::{Confidence, ReviewLimits};

pub use location::FindingLocation;
pub use submission::ReviewSubmission;

/// Immutable reviewer/cycle provenance retained through reconciliation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FindingSource {
    cycle_id: ReviewCycleId,
    reviewer: ActorId,
}

impl FindingSource {
    /// Creates one exact source record.
    #[must_use]
    pub const fn new(cycle_id: ReviewCycleId, reviewer: ActorId) -> Self {
        Self { cycle_id, reviewer }
    }
    /// Returns the source review cycle.
    #[must_use]
    pub const fn cycle_id(self) -> ReviewCycleId {
        self.cycle_id
    }
    /// Returns the source reviewer.
    #[must_use]
    pub const fn reviewer(self) -> ActorId {
        self.reviewer
    }
}

/// One stable structured finding with append-only provenance and disposition history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    id: FindingId,
    origin: FindingSource,
    pub(super) sources: Vec<FindingSource>,
    category: ReviewCategory,
    severity: FindingSeverity,
    blocking: bool,
    confidence: Confidence,
    requirements: Vec<RequirementId>,
    locations: Vec<FindingLocation>,
    pub(super) evidence: Vec<EvidenceId>,
    description: String,
    reproduction: String,
    expected_behavior: String,
    remediation: String,
    revision: RevisionTuple,
    normalized_digest: Sha256Digest,
    pub(super) dispositions: Vec<DispositionRecord>,
    pub(super) superseded_by: Option<FindingId>,
}

impl Finding {
    /// Creates a completely checked reviewer finding.
    ///
    /// # Errors
    /// Rejects malformed canonical collections, missing text, invalid source locations, or bounds.
    #[allow(clippy::too_many_arguments, reason = "the immutable normalized body stays explicit")]
    pub fn new(
        id: FindingId,
        source: FindingSource,
        category: ReviewCategory,
        severity: FindingSeverity,
        blocking_severity: FindingSeverity,
        confidence: Confidence,
        requirements: Vec<RequirementId>,
        locations: Vec<FindingLocation>,
        evidence: Vec<EvidenceId>,
        description: String,
        reproduction: String,
        expected_behavior: String,
        remediation: String,
        revision: RevisionTuple,
        limits: ReviewLimits,
    ) -> Result<Self, ReviewError> {
        let finding = Self::from_wire(
            id,
            source,
            vec![source],
            category,
            severity,
            severity >= blocking_severity,
            confidence,
            requirements,
            locations,
            evidence,
            description,
            reproduction,
            expected_behavior,
            remediation,
            revision,
            Sha256Digest::new([0; 32]),
            Vec::new(),
            None,
        );
        let mut finding = finding;
        finding.normalized_digest = crate::canonical::finding_digest(&finding);
        finding.validate(blocking_severity, limits)?;
        Ok(finding)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        id: FindingId,
        origin: FindingSource,
        sources: Vec<FindingSource>,
        category: ReviewCategory,
        severity: FindingSeverity,
        blocking: bool,
        confidence: Confidence,
        requirements: Vec<RequirementId>,
        locations: Vec<FindingLocation>,
        evidence: Vec<EvidenceId>,
        description: String,
        reproduction: String,
        expected_behavior: String,
        remediation: String,
        revision: RevisionTuple,
        normalized_digest: Sha256Digest,
        dispositions: Vec<DispositionRecord>,
        superseded_by: Option<FindingId>,
    ) -> Self {
        Self {
            id,
            origin,
            sources,
            category,
            severity,
            blocking,
            confidence,
            requirements,
            locations,
            evidence,
            description,
            reproduction,
            expected_behavior,
            remediation,
            revision,
            normalized_digest,
            dispositions,
            superseded_by,
        }
    }

    /// Returns the stable finding identity.
    #[must_use]
    pub const fn id(&self) -> FindingId {
        self.id
    }
    /// Returns the originating cycle/reviewer.
    #[must_use]
    pub const fn origin(&self) -> FindingSource {
        self.origin
    }
    /// Returns all canonical reconciled sources.
    #[must_use]
    pub const fn sources(&self) -> &[FindingSource] {
        self.sources.as_slice()
    }
    /// Returns the canonical review category.
    #[must_use]
    pub const fn category(&self) -> ReviewCategory {
        self.category
    }
    /// Returns finding severity.
    #[must_use]
    pub const fn severity(&self) -> FindingSeverity {
        self.severity
    }
    /// Returns the contract-derived blocking flag.
    #[must_use]
    pub const fn blocking(&self) -> bool {
        self.blocking
    }
    /// Returns fixed-point confidence.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }
    /// Returns canonical affected requirements.
    #[must_use]
    pub const fn requirements(&self) -> &[RequirementId] {
        self.requirements.as_slice()
    }
    /// Returns canonical source locations.
    #[must_use]
    pub const fn locations(&self) -> &[FindingLocation] {
        self.locations.as_slice()
    }
    /// Returns all canonical evidence, including reconciled evidence.
    #[must_use]
    pub const fn evidence(&self) -> &[EvidenceId] {
        self.evidence.as_slice()
    }
    /// Borrows the required description.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }
    /// Borrows bounded reproduction steps.
    #[must_use]
    pub fn reproduction(&self) -> &str {
        &self.reproduction
    }
    /// Borrows expected behavior.
    #[must_use]
    pub fn expected_behavior(&self) -> &str {
        &self.expected_behavior
    }
    /// Borrows recommended remediation.
    #[must_use]
    pub fn remediation(&self) -> &str {
        &self.remediation
    }
    /// Returns the exact affected revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the normalized finding fingerprint.
    #[must_use]
    pub const fn normalized_digest(&self) -> Sha256Digest {
        self.normalized_digest
    }
    /// Returns complete append-only disposition history.
    #[must_use]
    pub const fn dispositions(&self) -> &[DispositionRecord] {
        self.dispositions.as_slice()
    }
    /// Returns the canonical replacement after confirmed supersession.
    #[must_use]
    pub const fn superseded_by(&self) -> Option<FindingId> {
        self.superseded_by
    }

    /// Derives the last current disposition; a pre-admission body is open.
    #[must_use]
    pub fn current_disposition(&self) -> DispositionKind {
        self.dispositions.last().map_or(DispositionKind::Open, DispositionRecord::kind)
    }

    /// Returns whether the identity is retained history rather than a current projection.
    #[must_use]
    pub fn is_superseded_or_invalidated(&self) -> bool {
        self.superseded_by.is_some()
            || self.current_disposition() == DispositionKind::InvalidationConfirmed
    }

    /// Returns whether one permitted current disposition conserves this finding.
    #[must_use]
    pub fn is_conserved(&self) -> bool {
        matches!(
            self.current_disposition(),
            DispositionKind::ResolutionConfirmed
                | DispositionKind::InvalidationConfirmed
                | DispositionKind::Superseded
                | DispositionKind::Waived
        )
    }

    pub(super) fn validate(
        &self,
        blocking_severity: FindingSeverity,
        limits: ReviewLimits,
    ) -> Result<(), ReviewError> {
        if self.blocking != (self.severity >= blocking_severity)
            || self.confidence.get() > Confidence::MAXIMUM
            || self.sources.len() > usize::from(limits.provenance_sources())
            || self.requirements.len() > usize::from(limits.requirements())
            || self.locations.len() > usize::from(limits.locations())
            || self.dispositions.len() > usize::from(limits.disposition_records())
        {
            return Err(reject(
                ReviewErrorKind::LimitExceeded,
                "finding derived fields or collection bounds are invalid",
            ));
        }
        canonical_nonempty(&self.sources, "finding sources are not canonical")?;
        if !self.sources.contains(&self.origin) {
            return Err(reject(
                ReviewErrorKind::InvalidInput,
                "finding provenance omits its originating source",
            ));
        }
        canonical(&self.requirements, "finding requirements are not canonical")?;
        canonical(&self.locations, "finding locations are not canonical")?;
        validate_evidence(&self.evidence, limits)?;
        for location in &self.locations {
            location.validate(limits)?;
        }
        for text in
            [&self.description, &self.reproduction, &self.expected_behavior, &self.remediation]
        {
            if text.is_empty() || text.len() > limits.text_bytes() as usize {
                return Err(reject(
                    ReviewErrorKind::LimitExceeded,
                    "required finding text is empty or exceeds its byte limit",
                ));
            }
        }
        for disposition in &self.dispositions {
            disposition.validate(limits)?;
            if disposition.revision() != self.revision {
                return Err(reject(
                    ReviewErrorKind::BindingMismatch,
                    "finding disposition is bound to another revision",
                ));
            }
        }
        if crate::canonical::finding_digest(self) != self.normalized_digest {
            return Err(reject(
                ReviewErrorKind::InvalidInput,
                "finding digest differs from its normalized semantic body",
            ));
        }
        Ok(())
    }
}
