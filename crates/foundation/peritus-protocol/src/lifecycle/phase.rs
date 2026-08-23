//! Canonical lifecycle phase vocabulary.

#![allow(
    clippy::missing_errors_doc,
    reason = "canonical phase failures use the shared CodecError vocabulary"
)]

use crate::SCHEMA_V1;
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_kernel::{
    AcceptancePhase, ActionPhase, AttemptPhase, ReviewPhase, RunPhase, SessionPhase, TurnPhase,
    WaiverPhase,
};

/// Closed union of every B0 lifecycle phase.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LifecyclePhaseDto {
    /// Session phase.
    Session(SessionPhase),
    /// Run phase.
    Run(RunPhase),
    /// Attempt phase.
    Attempt(AttemptPhase),
    /// Turn phase.
    Turn(TurnPhase),
    /// Action phase.
    Action(ActionPhase),
    /// Review phase.
    Review(ReviewPhase),
    /// Waiver phase.
    Waiver(WaiverPhase),
    /// Acceptance phase.
    Acceptance(AcceptancePhase),
}

impl CanonicalEncode for LifecyclePhaseDto {
    const FAMILY: u16 = 5;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let (family, phase) = phase_tags(*self);
        writer.write_u16(family)?;
        writer.write_u16(phase)
    }
}

impl CanonicalDecode for LifecyclePhaseDto {
    const FAMILY: u16 = 5;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let family_offset = reader.offset();
        let family = reader.read_u16()?;
        let phase_offset = reader.offset();
        let phase = reader.read_u16()?;
        match family {
            1 => read_session(phase).map(Self::Session),
            2 => read_run(phase).map(Self::Run),
            3 => read_attempt(phase).map(Self::Attempt),
            4 => read_turn(phase).map(Self::Turn),
            5 => read_action(phase).map(Self::Action),
            6 => read_review(phase).map(Self::Review),
            7 => read_waiver(phase).map(Self::Waiver),
            8 => read_acceptance(phase).map(Self::Acceptance),
            _ => Err(CodecError::at(CodecErrorKind::UnknownTag, family_offset)),
        }
        .map_err(|error| {
            if error.offset() == 0 {
                CodecError::at(CodecErrorKind::UnknownTag, phase_offset)
            } else {
                error
            }
        })
    }
}

const fn phase_tags(phase: LifecyclePhaseDto) -> (u16, u16) {
    match phase {
        LifecyclePhaseDto::Session(value) => (
            1,
            match value {
                SessionPhase::Open => 1,
                SessionPhase::Paused => 2,
                SessionPhase::Closed => 3,
            },
        ),
        LifecyclePhaseDto::Run(value) => (
            2,
            match value {
                RunPhase::Pending => 1,
                RunPhase::Running => 2,
                RunPhase::Paused => 3,
                RunPhase::Reviewing => 4,
                RunPhase::Fixing => 5,
                RunPhase::Accepted => 6,
                RunPhase::Rejected => 7,
                RunPhase::Cancelled => 8,
                RunPhase::Failed => 9,
                RunPhase::Exhausted => 10,
            },
        ),
        LifecyclePhaseDto::Attempt(value) => (
            3,
            match value {
                AttemptPhase::Active => 1,
                AttemptPhase::Submitted => 2,
                AttemptPhase::Reviewing => 3,
                AttemptPhase::Fixing => 4,
                AttemptPhase::Accepted => 5,
                AttemptPhase::Failed => 6,
                AttemptPhase::Cancelled => 7,
                AttemptPhase::Exhausted => 8,
            },
        ),
        LifecyclePhaseDto::Turn(value) => (
            4,
            match value {
                TurnPhase::Active => 1,
                TurnPhase::Completed => 2,
                TurnPhase::Failed => 3,
                TurnPhase::Cancelled => 4,
            },
        ),
        LifecyclePhaseDto::Action(value) => (
            5,
            match value {
                ActionPhase::Proposed => 1,
                ActionPhase::Authorized => 2,
                ActionPhase::Dispatched => 3,
                ActionPhase::Succeeded => 4,
                ActionPhase::Failed => 5,
                ActionPhase::Cancelled => 6,
            },
        ),
        LifecyclePhaseDto::Review(value) => (
            6,
            match value {
                ReviewPhase::Requested => 1,
                ReviewPhase::Active => 2,
                ReviewPhase::Submitted => 3,
                ReviewPhase::Invalidated => 4,
            },
        ),
        LifecyclePhaseDto::Waiver(value) => (
            7,
            match value {
                WaiverPhase::Requested => 1,
                WaiverPhase::Granted => 2,
                WaiverPhase::Denied => 3,
                WaiverPhase::Invalidated => 4,
            },
        ),
        LifecyclePhaseDto::Acceptance(value) => (
            8,
            match value {
                AcceptancePhase::Pending => 1,
                AcceptancePhase::Evaluating => 2,
                AcceptancePhase::NeedsChanges => 3,
                AcceptancePhase::Accepted => 4,
                AcceptancePhase::Terminated => 5,
            },
        ),
    }
}

const fn unknown() -> CodecError {
    CodecError::at(CodecErrorKind::UnknownTag, 0)
}
const fn read_session(tag: u16) -> Result<SessionPhase, CodecError> {
    match tag {
        1 => Ok(SessionPhase::Open),
        2 => Ok(SessionPhase::Paused),
        3 => Ok(SessionPhase::Closed),
        _ => Err(unknown()),
    }
}
const fn read_run(tag: u16) -> Result<RunPhase, CodecError> {
    match tag {
        1 => Ok(RunPhase::Pending),
        2 => Ok(RunPhase::Running),
        3 => Ok(RunPhase::Paused),
        4 => Ok(RunPhase::Reviewing),
        5 => Ok(RunPhase::Fixing),
        6 => Ok(RunPhase::Accepted),
        7 => Ok(RunPhase::Rejected),
        8 => Ok(RunPhase::Cancelled),
        9 => Ok(RunPhase::Failed),
        10 => Ok(RunPhase::Exhausted),
        _ => Err(unknown()),
    }
}
const fn read_attempt(tag: u16) -> Result<AttemptPhase, CodecError> {
    match tag {
        1 => Ok(AttemptPhase::Active),
        2 => Ok(AttemptPhase::Submitted),
        3 => Ok(AttemptPhase::Reviewing),
        4 => Ok(AttemptPhase::Fixing),
        5 => Ok(AttemptPhase::Accepted),
        6 => Ok(AttemptPhase::Failed),
        7 => Ok(AttemptPhase::Cancelled),
        8 => Ok(AttemptPhase::Exhausted),
        _ => Err(unknown()),
    }
}
const fn read_turn(tag: u16) -> Result<TurnPhase, CodecError> {
    match tag {
        1 => Ok(TurnPhase::Active),
        2 => Ok(TurnPhase::Completed),
        3 => Ok(TurnPhase::Failed),
        4 => Ok(TurnPhase::Cancelled),
        _ => Err(unknown()),
    }
}
const fn read_action(tag: u16) -> Result<ActionPhase, CodecError> {
    match tag {
        1 => Ok(ActionPhase::Proposed),
        2 => Ok(ActionPhase::Authorized),
        3 => Ok(ActionPhase::Dispatched),
        4 => Ok(ActionPhase::Succeeded),
        5 => Ok(ActionPhase::Failed),
        6 => Ok(ActionPhase::Cancelled),
        _ => Err(unknown()),
    }
}
const fn read_review(tag: u16) -> Result<ReviewPhase, CodecError> {
    match tag {
        1 => Ok(ReviewPhase::Requested),
        2 => Ok(ReviewPhase::Active),
        3 => Ok(ReviewPhase::Submitted),
        4 => Ok(ReviewPhase::Invalidated),
        _ => Err(unknown()),
    }
}
const fn read_waiver(tag: u16) -> Result<WaiverPhase, CodecError> {
    match tag {
        1 => Ok(WaiverPhase::Requested),
        2 => Ok(WaiverPhase::Granted),
        3 => Ok(WaiverPhase::Denied),
        4 => Ok(WaiverPhase::Invalidated),
        _ => Err(unknown()),
    }
}
const fn read_acceptance(tag: u16) -> Result<AcceptancePhase, CodecError> {
    match tag {
        1 => Ok(AcceptancePhase::Pending),
        2 => Ok(AcceptancePhase::Evaluating),
        3 => Ok(AcceptancePhase::NeedsChanges),
        4 => Ok(AcceptancePhase::Accepted),
        5 => Ok(AcceptancePhase::Terminated),
        _ => Err(unknown()),
    }
}
