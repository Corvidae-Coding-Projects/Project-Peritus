//! Immutable report construction and canonical validation.

use peritus_types::Sha256Digest;

use crate::{
    DatasetDigest, EvaluationAnalysis, EvaluationCampaignId, EvaluationError, EvaluationErrorKind,
    EvaluationOperation, EvaluationPlanId, EvaluationRecovery, EvaluationReportId, PlanDigest,
    ProfileDigest, StabilityClass,
};

/// Closed inert constraint observation; it carries no acceptance authority.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConstraintViolationKind {
    /// Candidate produced valid safety failures.
    SafetyFailure,
    /// Candidate rollouts were observably unstable.
    Instability,
    /// One or more configured correctness metrics were unavailable.
    CorrectnessUnavailable,
    /// Required resource observations were unavailable.
    ResourceObservationUnavailable,
}

impl ConstraintViolationKind {
    pub(super) const fn tag(self) -> u8 {
        match self {
            Self::SafetyFailure => 1,
            Self::Instability => 2,
            Self::CorrectnessUnavailable => 3,
            Self::ResourceObservationUnavailable => 4,
        }
    }
}

/// Evidence-bound report constraint observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstraintViolation {
    pub(super) kind: ConstraintViolationKind,
    pub(super) affected: u32,
    pub(super) evidence_digest: Sha256Digest,
}

impl ConstraintViolation {
    /// Constraint class.
    #[must_use]
    pub const fn kind(self) -> ConstraintViolationKind {
        self.kind
    }
    /// Number of affected observations.
    #[must_use]
    pub const fn affected(self) -> u32 {
        self.affected
    }
    /// Exact analysis evidence digest.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
}

/// Complete inert report before byte/digest validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationReport {
    pub(super) id: EvaluationReportId,
    pub(super) campaign_id: EvaluationCampaignId,
    pub(super) dataset_digest: DatasetDigest,
    pub(super) profile_digest: ProfileDigest,
    pub(super) plan_id: EvaluationPlanId,
    pub(super) plan_digest: PlanDigest,
    pub(super) analysis: EvaluationAnalysis,
    pub(super) supersedes: Option<EvaluationReportId>,
    pub(super) violations: Vec<ConstraintViolation>,
}

impl EvaluationReport {
    /// Constructs a deterministic report and derives its identity from immutable inputs.
    ///
    /// # Errors
    /// Rejects canonical encoding overflow or an invalid derived identity.
    #[allow(clippy::too_many_arguments, reason = "every immutable report identity stays explicit")]
    pub fn new(
        campaign_id: EvaluationCampaignId,
        dataset_digest: DatasetDigest,
        profile_digest: ProfileDigest,
        plan_id: EvaluationPlanId,
        plan_digest: PlanDigest,
        analysis: EvaluationAnalysis,
        supersedes: Option<EvaluationReportId>,
    ) -> Result<Self, EvaluationError> {
        let placeholder = EvaluationReportId::new([1; 16])?;
        let violations = violations(&analysis);
        let mut report = Self {
            id: placeholder,
            campaign_id,
            dataset_digest,
            profile_digest,
            plan_id,
            plan_digest,
            analysis,
            supersedes,
            violations,
        };
        let identity = crate::identity::derived_identity(
            b"peritus.evaluation.report-id.v1\0",
            &[
                campaign_id.as_bytes(),
                dataset_digest.as_bytes(),
                profile_digest.as_bytes(),
                plan_id.as_bytes(),
                plan_digest.as_bytes(),
                report.analysis.digest().as_bytes(),
            ],
        );
        report.id = EvaluationReportId::new(identity)?;
        Ok(report)
    }
    /// Report identity.
    #[must_use]
    pub const fn id(&self) -> EvaluationReportId {
        self.id
    }
    /// Owning campaign.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Dataset manifest digest.
    #[must_use]
    pub const fn dataset_digest(&self) -> DatasetDigest {
        self.dataset_digest
    }
    /// Frozen profile digest.
    #[must_use]
    pub const fn profile_digest(&self) -> ProfileDigest {
        self.profile_digest
    }
    /// Complete plan identity.
    #[must_use]
    pub const fn plan_id(&self) -> EvaluationPlanId {
        self.plan_id
    }
    /// Complete plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> PlanDigest {
        self.plan_digest
    }
    /// Complete analysis.
    #[must_use]
    pub const fn analysis(&self) -> &EvaluationAnalysis {
        &self.analysis
    }
    /// Prior compatible report when this is a regression comparison.
    #[must_use]
    pub const fn supersedes(&self) -> Option<EvaluationReportId> {
        self.supersedes
    }
    /// Inert visible constraint observations.
    #[must_use]
    pub fn violations(&self) -> &[ConstraintViolation] {
        &self.violations
    }

    /// Validates and freezes exact canonical bytes.
    ///
    /// # Errors
    /// Rejects identity drift or canonical encoding overflow.
    pub fn validate(self) -> Result<ValidatedEvaluationReport, EvaluationError> {
        let expected = crate::identity::derived_identity(
            b"peritus.evaluation.report-id.v1\0",
            &[
                self.campaign_id.as_bytes(),
                self.dataset_digest.as_bytes(),
                self.profile_digest.as_bytes(),
                self.plan_id.as_bytes(),
                self.plan_digest.as_bytes(),
                self.analysis.digest().as_bytes(),
            ],
        );
        if self.id.as_bytes() != &expected {
            return Err(EvaluationError::new(
                EvaluationErrorKind::Corruption,
                EvaluationOperation::Analyze,
                EvaluationRecovery::Quarantine,
                "evaluation report identity differs from immutable inputs",
            ));
        }
        let bytes = super::canonical::report_bytes(&self)?;
        let digest = peritus_codec::sha256(&bytes);
        Ok(ValidatedEvaluationReport { report: self, bytes, digest })
    }
}

/// Validated immutable report plus exact canonical artifact bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedEvaluationReport {
    report: EvaluationReport,
    bytes: Vec<u8>,
    digest: Sha256Digest,
}

impl ValidatedEvaluationReport {
    /// Checked semantic report.
    #[must_use]
    pub const fn report(&self) -> &EvaluationReport {
        &self.report
    }
    /// Stable report identity.
    #[must_use]
    pub const fn id(&self) -> EvaluationReportId {
        self.report.id
    }
    /// Exact canonical artifact bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// SHA-256 of canonical artifact bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn violations(analysis: &EvaluationAnalysis) -> Vec<ConstraintViolation> {
    let mut values = Vec::new();
    let evidence_digest = analysis.digest().digest();
    if analysis.candidate().safety_failures() > 0 {
        values.push(ConstraintViolation {
            kind: ConstraintViolationKind::SafetyFailure,
            affected: analysis.candidate().safety_failures(),
            evidence_digest,
        });
    }
    if let crate::MetricAvailability::Available(stability) = analysis.candidate().stability() {
        let affected = u32::try_from(
            stability
                .iter()
                .filter(|value| value.summary().class() == StabilityClass::Unstable)
                .count(),
        )
        .unwrap_or(u32::MAX);
        if affected > 0 {
            values.push(ConstraintViolation {
                kind: ConstraintViolationKind::Instability,
                affected,
                evidence_digest,
            });
        }
    }
    if analysis.candidate().raw_success_interval().value().is_none()
        || analysis.paired().value().is_none()
    {
        values.push(ConstraintViolation {
            kind: ConstraintViolationKind::CorrectnessUnavailable,
            affected: 1,
            evidence_digest,
        });
    }
    let resources = analysis.candidate_resources();
    if resources.cost_microunits().value().is_none()
        || resources.input_tokens().value().is_none()
        || resources.output_tokens().value().is_none()
    {
        values.push(ConstraintViolation {
            kind: ConstraintViolationKind::ResourceObservationUnavailable,
            affected: 1,
            evidence_digest,
        });
    }
    values
}
