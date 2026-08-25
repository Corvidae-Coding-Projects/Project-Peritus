//! Closed D0 phase vocabulary carried by inert protocol records.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

/// A nonterminal phase to which a paused turn may return.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AgentResumablePhaseDto {
    /// Role, memory, and context selection are being prepared.
    PreparingContext,
    /// A committed request is ready to start or retry.
    RequestingModel,
    /// Normalized provider events are being observed.
    StreamingResponse,
    /// Complete model tool calls have been recorded as inert proposals.
    ProposedToolCalls,
    /// Independent B0/B1/C0 authority is being assembled.
    AwaitingAuthorization,
    /// Authorized calls are being observed through C4.
    ExecutingTools,
    /// Terminal tool observations are being ordered for the model.
    RecordingResults,
    /// A completion proposal has been formed but is not acceptance.
    ProposedCompletion,
}

/// Complete durable D0 phase, including explicit control and terminal states.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AgentPhaseDto {
    /// An ordinary resumable phase.
    Active(AgentResumablePhaseDto),
    /// Explicit pause retaining the exact prior phase.
    Paused(AgentResumablePhaseDto),
    /// Cancellation has been requested while owned effects are reconciled.
    Cancelling,
    /// The inner turn produced a durable proposal and completed its B0 turn.
    Completed,
    /// The turn terminated with failure.
    Failed,
    /// The turn terminated by cancellation.
    Cancelled,
}

pub(super) fn write_phase(
    writer: &mut CanonicalWriter,
    phase: AgentPhaseDto,
) -> Result<(), CodecError> {
    match phase {
        AgentPhaseDto::Active(value) => {
            writer.write_u16(resumable_tag(value))?;
            writer.write_u16(0)
        }
        AgentPhaseDto::Paused(value) => {
            writer.write_u16(9)?;
            writer.write_u16(resumable_tag(value))
        }
        AgentPhaseDto::Cancelling => tagged_terminal(writer, 10),
        AgentPhaseDto::Completed => tagged_terminal(writer, 11),
        AgentPhaseDto::Failed => tagged_terminal(writer, 12),
        AgentPhaseDto::Cancelled => tagged_terminal(writer, 13),
    }
}

pub(super) fn read_phase(reader: &mut CanonicalReader<'_>) -> Result<AgentPhaseDto, CodecError> {
    let offset = reader.offset();
    let tag = reader.read_u16()?;
    let nested_offset = reader.offset();
    let nested = reader.read_u16()?;
    match tag {
        1..=8 if nested == 0 => read_resumable(tag, offset).map(AgentPhaseDto::Active),
        9 => read_resumable(nested, nested_offset).map(AgentPhaseDto::Paused),
        10 if nested == 0 => Ok(AgentPhaseDto::Cancelling),
        11 if nested == 0 => Ok(AgentPhaseDto::Completed),
        12 if nested == 0 => Ok(AgentPhaseDto::Failed),
        13 if nested == 0 => Ok(AgentPhaseDto::Cancelled),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn resumable_tag(phase: AgentResumablePhaseDto) -> u16 {
    match phase {
        AgentResumablePhaseDto::PreparingContext => 1,
        AgentResumablePhaseDto::RequestingModel => 2,
        AgentResumablePhaseDto::StreamingResponse => 3,
        AgentResumablePhaseDto::ProposedToolCalls => 4,
        AgentResumablePhaseDto::AwaitingAuthorization => 5,
        AgentResumablePhaseDto::ExecutingTools => 6,
        AgentResumablePhaseDto::RecordingResults => 7,
        AgentResumablePhaseDto::ProposedCompletion => 8,
    }
}

const fn read_resumable(tag: u16, offset: usize) -> Result<AgentResumablePhaseDto, CodecError> {
    match tag {
        1 => Ok(AgentResumablePhaseDto::PreparingContext),
        2 => Ok(AgentResumablePhaseDto::RequestingModel),
        3 => Ok(AgentResumablePhaseDto::StreamingResponse),
        4 => Ok(AgentResumablePhaseDto::ProposedToolCalls),
        5 => Ok(AgentResumablePhaseDto::AwaitingAuthorization),
        6 => Ok(AgentResumablePhaseDto::ExecutingTools),
        7 => Ok(AgentResumablePhaseDto::RecordingResults),
        8 => Ok(AgentResumablePhaseDto::ProposedCompletion),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

fn tagged_terminal(writer: &mut CanonicalWriter, tag: u16) -> Result<(), CodecError> {
    writer.write_u16(tag)?;
    writer.write_u16(0)
}
