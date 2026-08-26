//! Complete bounded debugger aggregate checkpoint.

mod codec;

use peritus_types::{EventId, RevisionTuple, Sha256Digest};

use crate::{DebuggerError, DebuggerJobId};

use super::{
    AnalysisCounts, DebuggerPhase, JobFailure, ModelAttemptObservation, ModelProgress,
    PublicationRecord, ReportRecord, SelectionRecord,
};

/// Complete authoritative E2 job state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebuggerState {
    pub(crate) job_id: DebuggerJobId,
    pub(crate) revision: RevisionTuple,
    pub(crate) query_digest: Sha256Digest,
    pub(crate) limits_digest: Sha256Digest,
    pub(crate) model_plan_digest: Option<Sha256Digest>,
    pub(crate) sequence: u64,
    pub(crate) last_event_id: EventId,
    pub(crate) state_digest: Sha256Digest,
    pub(crate) phase: DebuggerPhase,
    pub(crate) selection: Option<SelectionRecord>,
    pub(crate) deterministic_digest: Option<Sha256Digest>,
    pub(crate) analysis_counts: Option<AnalysisCounts>,
    pub(crate) model: Option<ModelProgress>,
    pub(crate) model_attempts: Vec<ModelAttemptObservation>,
    pub(crate) report: Option<ReportRecord>,
    pub(crate) publication: Option<PublicationRecord>,
    pub(crate) failure: Option<JobFailure>,
    pub(crate) cancellation_reason_digest: Option<Sha256Digest>,
}

impl DebuggerState {
    /// Debugger job identity.
    #[must_use]
    pub const fn job_id(&self) -> DebuggerJobId {
        self.job_id
    }
    /// Exact cross-slice revision binding.
    #[must_use]
    pub const fn revision(&self) -> &RevisionTuple {
        &self.revision
    }
    /// Immutable query digest.
    #[must_use]
    pub const fn query_digest(&self) -> Sha256Digest {
        self.query_digest
    }
    /// Frozen resource-limit digest.
    #[must_use]
    pub const fn limits_digest(&self) -> Sha256Digest {
        self.limits_digest
    }
    /// Optional frozen model-plan digest.
    #[must_use]
    pub const fn model_plan_digest(&self) -> Option<Sha256Digest> {
        self.model_plan_digest
    }
    /// Applied event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Current event head.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    /// Digest of every complete state field.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Current durable phase.
    #[must_use]
    pub const fn phase(&self) -> DebuggerPhase {
        self.phase
    }
    /// Exact immutable selection observation.
    #[must_use]
    pub const fn selection(&self) -> Option<SelectionRecord> {
        self.selection
    }
    /// Canonical deterministic draft digest.
    #[must_use]
    pub const fn deterministic_digest(&self) -> Option<Sha256Digest> {
        self.deterministic_digest
    }
    /// Deterministic analysis counts.
    #[must_use]
    pub const fn analysis_counts(&self) -> Option<AnalysisCounts> {
        self.analysis_counts
    }
    /// Optional model-analysis state.
    #[must_use]
    pub const fn model(&self) -> Option<ModelProgress> {
        self.model
    }
    /// Complete settled model-attempt history in attempt order.
    #[must_use]
    pub fn model_attempts(&self) -> &[ModelAttemptObservation] {
        &self.model_attempts
    }
    /// Validated report state.
    #[must_use]
    pub const fn report(&self) -> Option<ReportRecord> {
        self.report
    }
    /// Completed publication state.
    #[must_use]
    pub const fn publication(&self) -> Option<PublicationRecord> {
        self.publication
    }
    /// Terminal failure, when present.
    #[must_use]
    pub const fn failure(&self) -> Option<JobFailure> {
        self.failure
    }
    /// Digest of redaction-safe cancellation metadata.
    #[must_use]
    pub const fn cancellation_reason_digest(&self) -> Option<Sha256Digest> {
        self.cancellation_reason_digest
    }

    /// Canonically encodes the complete state and its advertised digest.
    ///
    /// # Errors
    ///
    /// Returns an encoding error when the complete bounded state violates canonical codec limits.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, DebuggerError> {
        codec::encode(self)
    }

    /// Decodes and revalidates one complete canonical state.
    pub(crate) fn decode_canonical(bytes: &[u8]) -> Result<Self, DebuggerError> {
        codec::decode(bytes)
    }

    pub(crate) fn refresh_digest(&mut self) -> Result<(), DebuggerError> {
        self.state_digest = peritus_codec::sha256(&codec::identity_bytes(self)?);
        Ok(())
    }
}
