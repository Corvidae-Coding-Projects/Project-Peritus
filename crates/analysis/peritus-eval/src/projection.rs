//! Rebuildable read-only campaign progress projection.

use peritus_types::{EvidenceId, Sha256Digest};

use crate::{
    AnalysisSafePoint, EvaluationCampaignId, EvaluationPhase, EvaluationReportId, EvaluationState,
    LedgerCounts, ProfileDigest, PublicationCancellationRecord,
};

/// Inert query projection with no command, execution, or promotion capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationProjection {
    campaign_id: EvaluationCampaignId,
    profile_digest: ProfileDigest,
    phase: EvaluationPhase,
    sequence: u64,
    counts: LedgerCounts,
    analysis_digest: Option<Sha256Digest>,
    report_id: Option<EvaluationReportId>,
    evidence_id: Option<EvidenceId>,
    cancellation_reason: Option<Sha256Digest>,
    suspension_origin: Option<EvaluationPhase>,
    suspension_reason: Option<Sha256Digest>,
    analysis_safe_point: Option<AnalysisSafePoint>,
    publication_cancellation: Option<PublicationCancellationRecord>,
    failure_digest: Option<Sha256Digest>,
}

impl EvaluationProjection {
    /// Rebuilds a query projection from authoritative state.
    #[must_use]
    pub fn rebuild(state: &EvaluationState) -> Self {
        Self {
            campaign_id: state.campaign_id(),
            profile_digest: state.profile_digest(),
            phase: state.phase(),
            sequence: state.sequence(),
            counts: state.counts(),
            analysis_digest: state.analysis_digest().map(crate::ResultDigest::digest),
            report_id: state.report().map(crate::ReportRecord::id),
            evidence_id: state.publication().map(crate::PublicationRecord::evidence_id),
            cancellation_reason: state.cancellation_reason(),
            suspension_origin: state.suspension_origin(),
            suspension_reason: state.suspension_reason(),
            analysis_safe_point: state.analysis_safe_point(),
            publication_cancellation: state.publication_cancellation(),
            failure_digest: state.failure().map(crate::CampaignFailure::digest),
        }
    }
    /// Campaign identity.
    #[must_use]
    pub const fn campaign_id(self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Frozen profile digest.
    #[must_use]
    pub const fn profile_digest(self) -> ProfileDigest {
        self.profile_digest
    }
    /// Durable phase.
    #[must_use]
    pub const fn phase(self) -> EvaluationPhase {
        self.phase
    }
    /// Applied event sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Complete progress counts.
    #[must_use]
    pub const fn counts(self) -> LedgerCounts {
        self.counts
    }
    /// Deterministic analysis digest.
    #[must_use]
    pub const fn analysis_digest(self) -> Option<Sha256Digest> {
        self.analysis_digest
    }
    /// Canonical report identity.
    #[must_use]
    pub const fn report_id(self) -> Option<EvaluationReportId> {
        self.report_id
    }
    /// Admitted report evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> Option<EvidenceId> {
        self.evidence_id
    }
    /// Durable cancellation reason digest.
    #[must_use]
    pub const fn cancellation_reason(self) -> Option<Sha256Digest> {
        self.cancellation_reason
    }
    /// Phase retained by a durable suspension.
    #[must_use]
    pub const fn suspension_origin(self) -> Option<EvaluationPhase> {
        self.suspension_origin
    }
    /// Exact durable suspension reason.
    #[must_use]
    pub const fn suspension_reason(self) -> Option<Sha256Digest> {
        self.suspension_reason
    }
    /// Latest retained resumable analysis checkpoint.
    #[must_use]
    pub const fn analysis_safe_point(self) -> Option<AnalysisSafePoint> {
        self.analysis_safe_point
    }
    /// Retained result of cancelling a claimed publication.
    #[must_use]
    pub const fn publication_cancellation(self) -> Option<PublicationCancellationRecord> {
        self.publication_cancellation
    }
    /// Terminal failure digest.
    #[must_use]
    pub const fn failure_digest(self) -> Option<Sha256Digest> {
        self.failure_digest
    }
}
