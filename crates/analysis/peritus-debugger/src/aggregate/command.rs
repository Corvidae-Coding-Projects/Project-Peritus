//! Closed, fenced debugger command vocabulary.

pub(super) mod codec;

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_types::{CommandId, EventId, RevisionTuple, Sha256Digest};

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerJobId, DebuggerOperation, DebuggerRecovery,
    ModelAnalysisId,
};

use super::{
    AnalysisCounts, JobFailure, ModelAttemptFailure, ModelBudget, ModelRetryPolicy,
    PublicationRecord, ReportRecord, SelectionRecord,
};

const COMMAND_DOMAIN: &[u8] = b"peritus.debugger.command.v1\0";

/// Closed E2 command vocabulary. No variant carries mutation or acceptance authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebuggerCommandKind {
    /// Creates one immutable debugger job.
    CreateJob {
        /// Exact cross-slice revision binding.
        revision: RevisionTuple,
        /// Canonical query digest.
        query_digest: Sha256Digest,
        /// Canonical resource-limit digest.
        limits_digest: Sha256Digest,
        /// Optional frozen model plan digest.
        model_plan_digest: Option<Sha256Digest>,
    },
    /// Records an exact immutable evidence selection.
    RecordSelection {
        /// Frozen selection identity, digest, provenance, and retained counts.
        selection: SelectionRecord,
    },
    /// Records deterministic analysis output and accounting.
    RecordDeterministicAnalysis {
        /// Canonical deterministic draft digest.
        analysis_digest: Sha256Digest,
        /// Exact retained analysis counts.
        counts: AnalysisCounts,
    },
    /// Commits optional model work and its first outbox directive.
    RequestModelAnalysis {
        /// Stable model-analysis identity.
        model_id: ModelAnalysisId,
        /// Frozen complete plan digest.
        plan_digest: Sha256Digest,
        /// C5 semantic request digest.
        request_digest: Sha256Digest,
        /// Job-specific model budget.
        budget: ModelBudget,
        /// Bounded retry policy.
        retry_policy: ModelRetryPolicy,
    },
    /// Records that an exact claimed directive is about to call C5.
    MarkModelAttemptStarted {
        /// Stable model-analysis identity.
        model_id: ModelAnalysisId,
        /// Exact one-based attempt.
        attempt: u16,
        /// Positive caller monotonic tick observed after claiming the directive.
        started_at_tick: u64,
    },
    /// Records a proposal that passed strict E2 validation.
    RecordModelProposal {
        /// Stable model-analysis identity.
        model_id: ModelAnalysisId,
        /// Exact one-based attempt.
        attempt: u16,
        /// Canonical checked proposal digest.
        proposal_digest: Sha256Digest,
        /// Canonical structured item digest.
        output_digest: Sha256Digest,
        /// Canonical structured item byte count.
        output_bytes: u64,
        /// Normalized event count.
        event_count: u64,
        /// Observed input tokens.
        input_tokens: u64,
        /// Observed output tokens.
        output_tokens: u64,
        /// Observed total tokens.
        total_tokens: u64,
    },
    /// Records an inert typed model failure.
    RecordModelFailure {
        /// Redaction-safe failure classification and accounting.
        failure: ModelAttemptFailure,
    },
    /// Creates the next stable model outbox directive after a retryable failure.
    ScheduleModelRetry {
        /// Stable model-analysis identity.
        model_id: ModelAnalysisId,
        /// Exact next one-based attempt.
        next_attempt: u16,
        /// Caller monotonic tick before which the directive is ineligible.
        not_before_tick: u64,
    },
    /// Durably cancels a nonterminal job.
    CancelJob {
        /// Digest of bounded redaction-safe cancellation metadata.
        reason_digest: Sha256Digest,
    },
    /// Commits a fully validated report after its exact artifact bytes are staged and verified.
    CompleteReport {
        /// Validated report identity, digest, artifact dependency, and retained counts.
        report: ReportRecord,
    },
    /// Records finalized artifact and admitted evidence identities.
    RecordPublication {
        /// Exact artifact and evidence identities produced for the report.
        publication: PublicationRecord,
    },
    /// Terminates one nonterminal job with a typed safe failure.
    FailJob {
        /// Redaction-safe terminal failure classification.
        failure: JobFailure,
    },
}

/// One exact fenced command for a debugger aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebuggerCommand {
    command_id: CommandId,
    event_id: EventId,
    job_id: DebuggerJobId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    query_digest: Sha256Digest,
    kind: DebuggerCommandKind,
    digest: Sha256Digest,
}

impl DebuggerCommand {
    /// Constructs and binds every command fence and semantic field.
    ///
    /// # Errors
    ///
    /// Rejects inconsistent aggregate fences, invalid command payloads, or canonical encoding
    /// that exceeds the debugger protocol limits.
    #[allow(clippy::too_many_arguments, reason = "all aggregate fence fields remain explicit")]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        job_id: DebuggerJobId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        query_digest: Sha256Digest,
        kind: DebuggerCommandKind,
    ) -> Result<Self, DebuggerError> {
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(invalid("command sequence and predecessor presence disagree"));
        }
        validate_kind(&kind)?;
        let mut command = Self {
            command_id,
            event_id,
            job_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            query_digest,
            kind,
            digest: Sha256Digest::new([0; 32]),
        };
        command.digest = peritus_codec::sha256(&command.canonical_identity_bytes()?);
        Ok(command)
    }

    /// Command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Reserved event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Debugger job identity.
    #[must_use]
    pub const fn job_id(&self) -> DebuggerJobId {
        self.job_id
    }
    /// Expected current sequence.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Expected current head.
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Expected complete prior-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Immutable query digest repeated on every command.
    #[must_use]
    pub const fn query_digest(&self) -> Sha256Digest {
        self.query_digest
    }
    /// Semantic command.
    #[must_use]
    pub const fn kind(&self) -> &DebuggerCommandKind {
        &self.kind
    }
    /// Digest of every command field.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    pub(crate) fn canonical_identity_bytes(&self) -> Result<Vec<u8>, DebuggerError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(COMMAND_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.command_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.event_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.job_id.as_bytes()).map_err(codec)?;
        writer.write_u64(self.expected_sequence).map_err(codec)?;
        writer.write_option_tag(self.expected_previous_event.is_some()).map_err(codec)?;
        if let Some(event) = self.expected_previous_event {
            writer.write_fixed(event.as_bytes()).map_err(codec)?;
        }
        writer.write_fixed(self.prior_state_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.query_digest.as_bytes()).map_err(codec)?;
        super::encode_kind(&mut writer, &self.kind)?;
        Ok(writer.into_bytes())
    }
}

fn validate_kind(kind: &DebuggerCommandKind) -> Result<(), DebuggerError> {
    match kind {
        DebuggerCommandKind::MarkModelAttemptStarted { attempt, started_at_tick, .. }
            if *attempt == 0 || *started_at_tick == 0 =>
        {
            Err(invalid("model attempt or start tick is zero"))
        }
        DebuggerCommandKind::RecordModelProposal { attempt, .. } if *attempt == 0 => {
            Err(invalid("model attempt is zero"))
        }
        DebuggerCommandKind::ScheduleModelRetry { next_attempt, not_before_tick, .. }
            if *next_attempt < 2 || *not_before_tick == 0 =>
        {
            Err(invalid("retry attempt or scheduling tick is invalid"))
        }
        _ => Ok(()),
    }
}

fn codec(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Budget,
        DebuggerOperation::ApplyTransition,
        DebuggerRecovery::CorrectInput,
        error.to_string(),
    )
}

fn invalid(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::InvalidInput,
        DebuggerOperation::ApplyTransition,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
