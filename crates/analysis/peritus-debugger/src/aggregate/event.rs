//! Accepted debugger events and exact successor-state transitions.

use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{DebuggerJobId, ModelAnalysisId};

use super::{
    AnalysisCounts, DebuggerState, JobFailure, ModelAttemptFailure, ModelBudget, ModelRetryPolicy,
    PublicationRecord, ReportRecord, SelectionRecord,
};

/// Closed family-83 semantic event vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebuggerEventKind {
    /// Immutable job input was registered.
    JobCreated {
        /// Exact cross-slice revision binding.
        revision: peritus_types::RevisionTuple,
        /// Canonical limit-policy digest.
        limits_digest: Sha256Digest,
        /// Optional frozen model-plan digest.
        model_plan_digest: Option<Sha256Digest>,
    },
    /// Exact trace selection completed.
    SelectionRecorded {
        /// Frozen selection identity, digest, provenance, and retained counts.
        selection: SelectionRecord,
    },
    /// Deterministic analysis completed.
    DeterministicAnalysisRecorded {
        /// Canonical deterministic draft digest.
        analysis_digest: Sha256Digest,
        /// Exact retained counts.
        counts: AnalysisCounts,
    },
    /// Optional model work and first directive were committed.
    ModelAnalysisRequested {
        /// Stable model-analysis identity.
        model_id: ModelAnalysisId,
        /// Frozen plan digest.
        plan_digest: Sha256Digest,
        /// C5 semantic request digest.
        request_digest: Sha256Digest,
        /// Frozen model budget.
        budget: ModelBudget,
        /// Frozen retry policy.
        retry_policy: ModelRetryPolicy,
    },
    /// An exact claimed attempt was committed before C5 I/O.
    ModelAttemptStarted {
        /// Stable model-analysis identity.
        model_id: ModelAnalysisId,
        /// Exact one-based attempt.
        attempt: u16,
        /// Positive caller monotonic tick at attempt start.
        started_at_tick: u64,
    },
    /// One strict proposal passed complete validation.
    ModelProposalRecorded {
        /// Stable model-analysis identity.
        model_id: ModelAnalysisId,
        /// Exact attempt number.
        attempt: u16,
        /// Canonical checked proposal digest.
        proposal_digest: Sha256Digest,
        /// Canonical provider structured-item digest.
        output_digest: Sha256Digest,
        /// Canonical structured-item byte count.
        output_bytes: u64,
        /// Normalized event count.
        event_count: u64,
        /// Input token high water.
        input_tokens: u64,
        /// Output token high water.
        output_tokens: u64,
        /// Total token high water.
        total_tokens: u64,
    },
    /// One model attempt failed without admitting output into the report.
    ModelFailureRecorded {
        /// Redaction-safe failure classification and accounting.
        failure: ModelAttemptFailure,
    },
    /// The next model attempt and stable directive were scheduled.
    ModelRetryScheduled {
        /// Stable model-analysis identity.
        model_id: ModelAnalysisId,
        /// Exact next one-based attempt.
        next_attempt: u16,
        /// Caller monotonic tick before which the attempt is ineligible.
        not_before_tick: u64,
    },
    /// Durable cancellation won.
    JobCancelled {
        /// Digest of bounded redaction-safe cancellation metadata.
        reason_digest: Sha256Digest,
    },
    /// Validated report bytes were staged and then committed before evidence publication.
    ReportCompleted {
        /// Validated report identity, digest, artifact dependency, and retained counts.
        report: ReportRecord,
    },
    /// Finalized artifact and evidence identities were reconciled and recorded.
    PublicationRecorded {
        /// Exact artifact and evidence identities produced for the report.
        publication: PublicationRecord,
    },
    /// Typed terminal failure won.
    JobFailed {
        /// Redaction-safe terminal failure classification.
        failure: JobFailure,
    },
}

/// One fully bound accepted debugger event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebuggerEvent {
    id: EventId,
    command_id: CommandId,
    job_id: DebuggerJobId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    query_digest: Sha256Digest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind: DebuggerEventKind,
}

impl DebuggerEvent {
    #[allow(clippy::too_many_arguments, reason = "event integrity bindings remain explicit")]
    pub(crate) const fn new(
        id: EventId,
        command_id: CommandId,
        job_id: DebuggerJobId,
        sequence: u64,
        previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        query_digest: Sha256Digest,
        command_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        kind: DebuggerEventKind,
    ) -> Self {
        Self {
            id,
            command_id,
            job_id,
            sequence,
            previous_event,
            prior_state_digest,
            query_digest,
            command_digest,
            successor_state_digest,
            kind,
        }
    }
    /// Event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    /// Producing command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Debugger job identity.
    #[must_use]
    pub const fn job_id(&self) -> DebuggerJobId {
        self.job_id
    }
    /// Positive aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Exact predecessor event.
    #[must_use]
    pub const fn previous_event(&self) -> Option<EventId> {
        self.previous_event
    }
    /// Exact prior state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Immutable query digest.
    #[must_use]
    pub const fn query_digest(&self) -> Sha256Digest {
        self.query_digest
    }
    /// Complete producing-command digest.
    #[must_use]
    pub const fn command_digest(&self) -> Sha256Digest {
        self.command_digest
    }
    /// Complete successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Semantic event.
    #[must_use]
    pub const fn kind(&self) -> &DebuggerEventKind {
        &self.kind
    }
}

/// Accepted event paired with its complete successor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebuggerTransition {
    event: DebuggerEvent,
    state: DebuggerState,
}

impl DebuggerTransition {
    pub(crate) const fn new(event: DebuggerEvent, state: DebuggerState) -> Self {
        Self { event, state }
    }
    /// Accepted event.
    #[must_use]
    pub const fn event(&self) -> &DebuggerEvent {
        &self.event
    }
    /// Complete successor state.
    #[must_use]
    pub const fn state(&self) -> &DebuggerState {
        &self.state
    }
    /// Consumes the transition.
    #[must_use]
    pub fn into_parts(self) -> (DebuggerEvent, DebuggerState) {
        (self.event, self.state)
    }
}
