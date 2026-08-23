//! Canonical inert lifecycle event record.

#![allow(
    clippy::missing_errors_doc,
    reason = "canonical event failures use the shared CodecError vocabulary"
)]

use crate::SCHEMA_V1;
use crate::primitive::{
    read_id, read_option_id, read_revision, write_id, write_option_id, write_revision,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_kernel::{KernelEvent, KernelEventKind, KernelSubject};
use peritus_types::{
    ActionId, AttemptId, CommandId, EventId, EventSequence, FindingId, ReviewCycleId,
    RevisionTuple, RunId, SessionId, TurnId,
};

/// Typed subject of an inert decoded lifecycle event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KernelSubjectDto {
    /// Session identity.
    Session(SessionId),
    /// Run identity.
    Run(RunId),
    /// Attempt identity.
    Attempt(AttemptId),
    /// Turn identity.
    Turn(TurnId),
    /// Action identity.
    Action(ActionId),
    /// Review-cycle identity.
    Review(ReviewCycleId),
    /// Finding targeted by a waiver.
    Waiver(FindingId),
    /// Run targeted by acceptance.
    Acceptance(RunId),
}

impl From<KernelSubject> for KernelSubjectDto {
    fn from(subject: KernelSubject) -> Self {
        match subject {
            KernelSubject::Session(id) => Self::Session(id),
            KernelSubject::Run(id) => Self::Run(id),
            KernelSubject::Attempt(id) => Self::Attempt(id),
            KernelSubject::Turn(id) => Self::Turn(id),
            KernelSubject::Action(id) => Self::Action(id),
            KernelSubject::Review(id) => Self::Review(id),
            KernelSubject::Waiver(id) => Self::Waiver(id),
            KernelSubject::Acceptance(id) => Self::Acceptance(id),
        }
    }
}

/// Immutable event bytes decoded as data, never as a durable commit receipt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KernelEventDto {
    /// Event identity.
    pub id: EventId,
    /// Causative command identity.
    pub command_id: CommandId,
    /// One-based aggregate sequence.
    pub sequence: EventSequence,
    /// Exact causal predecessor.
    pub previous_event_id: Option<EventId>,
    /// Exact authority and evidence revision.
    pub revision: RevisionTuple,
    /// Stable lifecycle event kind.
    pub kind: KernelEventKind,
    /// Typed aggregate subject.
    pub subject: KernelSubjectDto,
}

impl From<KernelEvent> for KernelEventDto {
    fn from(event: KernelEvent) -> Self {
        Self {
            id: event.id(),
            command_id: event.command_id(),
            sequence: event.sequence(),
            previous_event_id: event.previous_event_id(),
            revision: event.revision(),
            kind: event.kind(),
            subject: event.subject().into(),
        }
    }
}

impl CanonicalEncode for KernelEventDto {
    const FAMILY: u16 = 3;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_id(writer, self.id.as_bytes())?;
        write_id(writer, self.command_id.as_bytes())?;
        writer.write_u64(self.sequence.get())?;
        write_option_id(writer, self.previous_event_id, EventId::into_bytes)?;
        write_revision(writer, &self.revision)?;
        writer.write_u16(event_tag(self.kind))?;
        write_subject(writer, self.subject)
    }
}

impl CanonicalDecode for KernelEventDto {
    const FAMILY: u16 = 3;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let id = read_id(reader, EventId::new)?;
        let command_id = read_id(reader, CommandId::new)?;
        let sequence_offset = reader.offset();
        let sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset))?;
        Ok(Self {
            id,
            command_id,
            sequence,
            previous_event_id: read_option_id(reader, EventId::new)?,
            revision: read_revision(reader)?,
            kind: read_event_kind(reader)?,
            subject: read_subject(reader)?,
        })
    }
}

const fn event_tag(kind: KernelEventKind) -> u16 {
    use KernelEventKind as K;
    match kind {
        K::SessionOpened => 1,
        K::SessionPaused => 2,
        K::SessionResumed => 3,
        K::SessionClosed => 4,
        K::RunStarted => 5,
        K::RunPaused => 6,
        K::RunResumed => 7,
        K::RunCancelled => 8,
        K::RunFailed => 9,
        K::RunExhausted => 10,
        K::RunRejected => 11,
        K::AttemptStarted => 12,
        K::AttemptResumed => 13,
        K::AttemptSubmitted => 14,
        K::AttemptFailed => 15,
        K::AttemptExhausted => 16,
        K::TurnStarted => 17,
        K::TurnCompleted => 18,
        K::TurnFailed => 19,
        K::TurnCancelled => 20,
        K::ActionProposed => 21,
        K::ActionAuthorized => 22,
        K::ActionDispatched => 23,
        K::ActionCompleted => 24,
        K::ActionFailed => 25,
        K::ActionCancelled => 26,
        K::ReviewRequested => 27,
        K::ReviewBegun => 28,
        K::ReviewSubmitted => 29,
        K::ReviewInvalidated => 30,
        K::WaiverRequested => 31,
        K::WaiverGranted => 32,
        K::WaiverDenied => 33,
        K::WaiverInvalidated => 34,
        K::AcceptanceBegun => 35,
        K::AcceptanceAccepted => 36,
        K::AcceptanceNeedsChanges => 37,
    }
}

fn read_event_kind(reader: &mut CanonicalReader<'_>) -> Result<KernelEventKind, CodecError> {
    use KernelEventKind as K;
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(K::SessionOpened),
        2 => Ok(K::SessionPaused),
        3 => Ok(K::SessionResumed),
        4 => Ok(K::SessionClosed),
        5 => Ok(K::RunStarted),
        6 => Ok(K::RunPaused),
        7 => Ok(K::RunResumed),
        8 => Ok(K::RunCancelled),
        9 => Ok(K::RunFailed),
        10 => Ok(K::RunExhausted),
        11 => Ok(K::RunRejected),
        12 => Ok(K::AttemptStarted),
        13 => Ok(K::AttemptResumed),
        14 => Ok(K::AttemptSubmitted),
        15 => Ok(K::AttemptFailed),
        16 => Ok(K::AttemptExhausted),
        17 => Ok(K::TurnStarted),
        18 => Ok(K::TurnCompleted),
        19 => Ok(K::TurnFailed),
        20 => Ok(K::TurnCancelled),
        21 => Ok(K::ActionProposed),
        22 => Ok(K::ActionAuthorized),
        23 => Ok(K::ActionDispatched),
        24 => Ok(K::ActionCompleted),
        25 => Ok(K::ActionFailed),
        26 => Ok(K::ActionCancelled),
        27 => Ok(K::ReviewRequested),
        28 => Ok(K::ReviewBegun),
        29 => Ok(K::ReviewSubmitted),
        30 => Ok(K::ReviewInvalidated),
        31 => Ok(K::WaiverRequested),
        32 => Ok(K::WaiverGranted),
        33 => Ok(K::WaiverDenied),
        34 => Ok(K::WaiverInvalidated),
        35 => Ok(K::AcceptanceBegun),
        36 => Ok(K::AcceptanceAccepted),
        37 => Ok(K::AcceptanceNeedsChanges),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

fn write_subject(
    writer: &mut CanonicalWriter,
    subject: KernelSubjectDto,
) -> Result<(), CodecError> {
    match subject {
        KernelSubjectDto::Session(id) => tagged_subject(writer, 1, id.as_bytes()),
        KernelSubjectDto::Run(id) => tagged_subject(writer, 2, id.as_bytes()),
        KernelSubjectDto::Attempt(id) => tagged_subject(writer, 3, id.as_bytes()),
        KernelSubjectDto::Turn(id) => tagged_subject(writer, 4, id.as_bytes()),
        KernelSubjectDto::Action(id) => tagged_subject(writer, 5, id.as_bytes()),
        KernelSubjectDto::Review(id) => tagged_subject(writer, 6, id.as_bytes()),
        KernelSubjectDto::Waiver(id) => tagged_subject(writer, 7, id.as_bytes()),
        KernelSubjectDto::Acceptance(id) => tagged_subject(writer, 8, id.as_bytes()),
    }
}

fn tagged_subject(
    writer: &mut CanonicalWriter,
    tag: u16,
    bytes: &[u8; 16],
) -> Result<(), CodecError> {
    writer.write_u16(tag)?;
    write_id(writer, bytes)
}

fn read_subject(reader: &mut CanonicalReader<'_>) -> Result<KernelSubjectDto, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => read_id(reader, SessionId::new).map(KernelSubjectDto::Session),
        2 => read_id(reader, RunId::new).map(KernelSubjectDto::Run),
        3 => read_id(reader, AttemptId::new).map(KernelSubjectDto::Attempt),
        4 => read_id(reader, TurnId::new).map(KernelSubjectDto::Turn),
        5 => read_id(reader, ActionId::new).map(KernelSubjectDto::Action),
        6 => read_id(reader, ReviewCycleId::new).map(KernelSubjectDto::Review),
        7 => read_id(reader, FindingId::new).map(KernelSubjectDto::Waiver),
        8 => read_id(reader, RunId::new).map(KernelSubjectDto::Acceptance),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
