//! Inert canonical family-83 debugger event frames.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError,
};
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{
    DebuggerCommandKind, DebuggerError, DebuggerEvent, DebuggerEventKind, DebuggerJobId,
    DebuggerState, apply_event,
};

/// Canonical inert family-83 schema-v1 semantic event frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebuggerEventFrame {
    event_id: EventId,
    command_id: CommandId,
    job_id: DebuggerJobId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    query_digest: Sha256Digest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    kind_bytes: Vec<u8>,
}

impl DebuggerEventFrame {
    /// Converts one accepted event to an inert canonical frame.
    ///
    /// # Errors
    ///
    /// Returns a codec error when the semantic event exceeds family-83 bounds.
    pub fn from_event(event: &DebuggerEvent) -> Result<Self, CodecError> {
        let command_kind = event_to_command(*event.kind(), event.query_digest());
        Ok(Self {
            event_id: event.id(),
            command_id: event.command_id(),
            job_id: event.job_id(),
            sequence: event.sequence(),
            previous_event: event.previous_event(),
            prior_state_digest: event.prior_state_digest(),
            query_digest: event.query_digest(),
            command_digest: event.command_digest(),
            successor_state_digest: event.successor_state_digest(),
            kind_bytes: super::semantic::encode(&command_kind).map_err(super::scalar::semantic)?,
        })
    }

    /// Activates inert event data through deterministic replay against exact prior state.
    ///
    /// # Errors
    ///
    /// Rejects invalid semantic payloads, broken fences, or successor-state disagreement.
    pub fn check(self, prior: Option<&DebuggerState>) -> Result<DebuggerEvent, DebuggerError> {
        let command_kind = super::semantic::decode(&self.kind_bytes)?;
        let kind = command_to_event(command_kind);
        let event = DebuggerEvent::new(
            self.event_id,
            self.command_id,
            self.job_id,
            self.sequence,
            self.previous_event,
            self.prior_state_digest,
            self.query_digest,
            self.command_digest,
            self.successor_state_digest,
            kind,
        );
        let _ = apply_event(prior, &event)?;
        Ok(event)
    }

    /// Event identity without activating semantic data.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Aggregate sequence without activating semantic data.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}

impl CanonicalEncode for DebuggerEventFrame {
    const FAMILY: u16 = 83;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_fixed(self.event_id.as_bytes())?;
        writer.write_fixed(self.command_id.as_bytes())?;
        writer.write_fixed(self.job_id.as_bytes())?;
        writer.write_u64(self.sequence)?;
        writer.write_option_tag(self.previous_event.is_some())?;
        if let Some(event) = self.previous_event {
            writer.write_fixed(event.as_bytes())?;
        }
        writer.write_fixed(self.prior_state_digest.as_bytes())?;
        writer.write_fixed(self.query_digest.as_bytes())?;
        writer.write_fixed(self.command_digest.as_bytes())?;
        writer.write_fixed(self.successor_state_digest.as_bytes())?;
        writer.write_bytes(&self.kind_bytes)
    }
}

impl CanonicalDecode for DebuggerEventFrame {
    const FAMILY: u16 = 83;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let event_id = super::scalar::event_id(reader)?;
        let command_id = super::scalar::command_id(reader)?;
        let job_id = super::scalar::job_id(reader)?;
        let sequence = reader.read_u64()?;
        let previous_event =
            reader.read_option_tag()?.then(|| super::scalar::event_id(reader)).transpose()?;
        if sequence == 0 || (sequence == 1) != previous_event.is_none() {
            return Err(super::scalar::invalid(reader));
        }
        let prior_state_digest = super::scalar::digest(reader)?;
        let query_digest = super::scalar::digest(reader)?;
        let command_digest = super::scalar::digest(reader)?;
        let successor_state_digest = super::scalar::digest(reader)?;
        let kind_bytes = reader.read_bytes_owned()?;
        let _ = super::semantic::decode(&kind_bytes).map_err(super::scalar::semantic)?;
        Ok(Self {
            event_id,
            command_id,
            job_id,
            sequence,
            previous_event,
            prior_state_digest,
            query_digest,
            command_digest,
            successor_state_digest,
            kind_bytes,
        })
    }
}

#[allow(clippy::too_many_lines, reason = "closed command-event schema mapping stays exhaustive")]
const fn event_to_command(
    kind: DebuggerEventKind,
    query_digest: Sha256Digest,
) -> DebuggerCommandKind {
    match kind {
        DebuggerEventKind::JobCreated { revision, limits_digest, model_plan_digest } => {
            DebuggerCommandKind::CreateJob {
                revision,
                query_digest,
                limits_digest,
                model_plan_digest,
            }
        }
        DebuggerEventKind::SelectionRecorded { selection } => {
            DebuggerCommandKind::RecordSelection { selection }
        }
        DebuggerEventKind::DeterministicAnalysisRecorded { analysis_digest, counts } => {
            DebuggerCommandKind::RecordDeterministicAnalysis { analysis_digest, counts }
        }
        DebuggerEventKind::ModelAnalysisRequested {
            model_id,
            plan_digest,
            request_digest,
            budget,
            retry_policy,
        } => DebuggerCommandKind::RequestModelAnalysis {
            model_id,
            plan_digest,
            request_digest,
            budget,
            retry_policy,
        },
        DebuggerEventKind::ModelAttemptStarted { model_id, attempt, started_at_tick } => {
            DebuggerCommandKind::MarkModelAttemptStarted { model_id, attempt, started_at_tick }
        }
        DebuggerEventKind::ModelProposalRecorded {
            model_id,
            attempt,
            proposal_digest,
            output_digest,
            output_bytes,
            event_count,
            input_tokens,
            output_tokens,
            total_tokens,
        } => DebuggerCommandKind::RecordModelProposal {
            model_id,
            attempt,
            proposal_digest,
            output_digest,
            output_bytes,
            event_count,
            input_tokens,
            output_tokens,
            total_tokens,
        },
        DebuggerEventKind::ModelFailureRecorded { failure } => {
            DebuggerCommandKind::RecordModelFailure { failure }
        }
        DebuggerEventKind::ModelRetryScheduled { model_id, next_attempt, not_before_tick } => {
            DebuggerCommandKind::ScheduleModelRetry { model_id, next_attempt, not_before_tick }
        }
        DebuggerEventKind::JobCancelled { reason_digest } => {
            DebuggerCommandKind::CancelJob { reason_digest }
        }
        DebuggerEventKind::ReportCompleted { report } => {
            DebuggerCommandKind::CompleteReport { report }
        }
        DebuggerEventKind::PublicationRecorded { publication } => {
            DebuggerCommandKind::RecordPublication { publication }
        }
        DebuggerEventKind::JobFailed { failure } => DebuggerCommandKind::FailJob { failure },
    }
}

const fn command_to_event(kind: DebuggerCommandKind) -> DebuggerEventKind {
    match kind {
        DebuggerCommandKind::CreateJob { revision, limits_digest, model_plan_digest, .. } => {
            DebuggerEventKind::JobCreated { revision, limits_digest, model_plan_digest }
        }
        DebuggerCommandKind::RecordSelection { selection } => {
            DebuggerEventKind::SelectionRecorded { selection }
        }
        DebuggerCommandKind::RecordDeterministicAnalysis { analysis_digest, counts } => {
            DebuggerEventKind::DeterministicAnalysisRecorded { analysis_digest, counts }
        }
        DebuggerCommandKind::RequestModelAnalysis {
            model_id,
            plan_digest,
            request_digest,
            budget,
            retry_policy,
        } => DebuggerEventKind::ModelAnalysisRequested {
            model_id,
            plan_digest,
            request_digest,
            budget,
            retry_policy,
        },
        DebuggerCommandKind::MarkModelAttemptStarted { model_id, attempt, started_at_tick } => {
            DebuggerEventKind::ModelAttemptStarted { model_id, attempt, started_at_tick }
        }
        DebuggerCommandKind::RecordModelProposal {
            model_id,
            attempt,
            proposal_digest,
            output_digest,
            output_bytes,
            event_count,
            input_tokens,
            output_tokens,
            total_tokens,
        } => DebuggerEventKind::ModelProposalRecorded {
            model_id,
            attempt,
            proposal_digest,
            output_digest,
            output_bytes,
            event_count,
            input_tokens,
            output_tokens,
            total_tokens,
        },
        DebuggerCommandKind::RecordModelFailure { failure } => {
            DebuggerEventKind::ModelFailureRecorded { failure }
        }
        DebuggerCommandKind::ScheduleModelRetry { model_id, next_attempt, not_before_tick } => {
            DebuggerEventKind::ModelRetryScheduled { model_id, next_attempt, not_before_tick }
        }
        DebuggerCommandKind::CancelJob { reason_digest } => {
            DebuggerEventKind::JobCancelled { reason_digest }
        }
        DebuggerCommandKind::CompleteReport { report } => {
            DebuggerEventKind::ReportCompleted { report }
        }
        DebuggerCommandKind::RecordPublication { publication } => {
            DebuggerEventKind::PublicationRecorded { publication }
        }
        DebuggerCommandKind::FailJob { failure } => DebuggerEventKind::JobFailed { failure },
    }
}
