//! Rebuildable read-only debugger job projection.

use peritus_types::{EvidenceId, RevisionTuple, Sha256Digest};

use crate::{
    AnalysisCounts, DebuggerJobId, DebuggerPhase, DebuggerState, JobFailureCode, ModelAnalysisId,
    ModelAttemptObservation, ModelWorkState, ReportId, SelectionManifestId,
};

/// Compact publication query row with no artifact or evidence mutation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectedPublication {
    report_id: ReportId,
    artifact_digest: Sha256Digest,
    evidence_id: EvidenceId,
    journal_position: u64,
}

impl ProjectedPublication {
    /// Published report identity.
    #[must_use]
    pub const fn report_id(self) -> ReportId {
        self.report_id
    }
    /// Finalized artifact digest.
    #[must_use]
    pub const fn artifact_digest(self) -> Sha256Digest {
        self.artifact_digest
    }
    /// Admitted evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
    /// Report event's one-based global C0 position.
    #[must_use]
    pub const fn journal_position(self) -> u64 {
        self.journal_position
    }
}

/// Complete authority-free job projection derived from an exact family-84 checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebuggerProjection {
    job_id: DebuggerJobId,
    revision: RevisionTuple,
    phase: DebuggerPhase,
    sequence: u64,
    state_digest: Sha256Digest,
    query_digest: Sha256Digest,
    selection_id: Option<SelectionManifestId>,
    selection_digest: Option<Sha256Digest>,
    deterministic_digest: Option<Sha256Digest>,
    analysis_counts: Option<AnalysisCounts>,
    model_id: Option<ModelAnalysisId>,
    model_state: Option<ModelWorkState>,
    model_attempts: Vec<ModelAttemptObservation>,
    report_id: Option<ReportId>,
    report_digest: Option<Sha256Digest>,
    publication: Option<ProjectedPublication>,
    failure: Option<JobFailureCode>,
    cancelled: bool,
}

impl DebuggerProjection {
    /// Projects a complete checked state deterministically.
    #[must_use]
    pub fn from_state(state: &DebuggerState) -> Self {
        let selection = state.selection();
        let model = state.model();
        let report = state.report();
        let publication = state.publication().map(|value| ProjectedPublication {
            report_id: value.report_id(),
            artifact_digest: value.artifact_digest(),
            evidence_id: value.evidence_id(),
            journal_position: value.journal_position(),
        });
        Self {
            job_id: state.job_id(),
            revision: *state.revision(),
            phase: state.phase(),
            sequence: state.sequence(),
            state_digest: state.state_digest(),
            query_digest: state.query_digest(),
            selection_id: selection.map(crate::SelectionRecord::id),
            selection_digest: selection.map(crate::SelectionRecord::digest),
            deterministic_digest: state.deterministic_digest(),
            analysis_counts: state.analysis_counts(),
            model_id: model.map(crate::ModelProgress::id),
            model_state: model.map(crate::ModelProgress::state),
            model_attempts: state.model_attempts().to_vec(),
            report_id: report.map(crate::ReportRecord::id),
            report_digest: report.map(crate::ReportRecord::digest),
            publication,
            failure: state.failure().map(crate::JobFailure::code),
            cancelled: state.phase() == DebuggerPhase::Cancelled,
        }
    }

    /// Job identity.
    #[must_use]
    pub const fn job_id(&self) -> DebuggerJobId {
        self.job_id
    }
    /// Exact revision binding.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Durable lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> DebuggerPhase {
        self.phase
    }
    /// Latest aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Complete authoritative state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Immutable query digest.
    #[must_use]
    pub const fn query_digest(&self) -> Sha256Digest {
        self.query_digest
    }
    /// Selected manifest identity and digest.
    #[must_use]
    pub const fn selection(&self) -> Option<(SelectionManifestId, Sha256Digest)> {
        match (self.selection_id, self.selection_digest) {
            (Some(id), Some(digest)) => Some((id, digest)),
            _ => None,
        }
    }
    /// Deterministic analysis digest.
    #[must_use]
    pub const fn deterministic_digest(&self) -> Option<Sha256Digest> {
        self.deterministic_digest
    }
    /// Deterministic result counts.
    #[must_use]
    pub const fn analysis_counts(&self) -> Option<AnalysisCounts> {
        self.analysis_counts
    }
    /// Optional model identity and exact work state.
    #[must_use]
    pub const fn model(&self) -> Option<(ModelAnalysisId, ModelWorkState)> {
        match (self.model_id, self.model_state) {
            (Some(id), Some(state)) => Some((id, state)),
            _ => None,
        }
    }
    /// Settled model attempt history.
    #[must_use]
    pub fn model_attempts(&self) -> &[ModelAttemptObservation] {
        &self.model_attempts
    }
    /// Validated report identity and digest.
    #[must_use]
    pub const fn report(&self) -> Option<(ReportId, Sha256Digest)> {
        match (self.report_id, self.report_digest) {
            (Some(id), Some(digest)) => Some((id, digest)),
            _ => None,
        }
    }
    /// Completed publication, if any.
    #[must_use]
    pub const fn publication(&self) -> Option<ProjectedPublication> {
        self.publication
    }
    /// Terminal failure code, if any.
    #[must_use]
    pub const fn failure(&self) -> Option<JobFailureCode> {
        self.failure
    }
    /// Whether cancellation won terminal-state arbitration.
    #[must_use]
    pub const fn cancelled(&self) -> bool {
        self.cancelled
    }
}
