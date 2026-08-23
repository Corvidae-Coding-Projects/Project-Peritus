//! Canonical lifecycle command request.

#![allow(
    clippy::missing_errors_doc,
    reason = "canonical command failures use the shared CodecError vocabulary"
)]

use crate::SCHEMA_V1;
use crate::primitive::{read_digest, read_id, read_role, write_digest, write_id, write_role};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_kernel::KernelCommand;
use peritus_types::{
    ActionId, ActorId, AttemptId, EnvironmentId, FindingId, ReviewCycleId, RunId, TurnId,
};

/// Canonical, authority-neutral lifecycle command DTO.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KernelCommandDto(KernelCommand);

impl KernelCommandDto {
    /// Borrows the checked reducer request.
    #[must_use]
    pub const fn as_domain(&self) -> &KernelCommand {
        &self.0
    }

    /// Consumes the DTO as a checked reducer request.
    #[must_use]
    pub const fn into_domain(self) -> KernelCommand {
        self.0
    }
}

impl From<KernelCommand> for KernelCommandDto {
    fn from(command: KernelCommand) -> Self {
        Self(command)
    }
}

impl CanonicalEncode for KernelCommandDto {
    const FAMILY: u16 = 1;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        use KernelCommand as C;
        match &self.0 {
            C::PauseSession => writer.write_u16(1),
            C::ResumeSession => writer.write_u16(2),
            C::CloseSession => writer.write_u16(3),
            C::StartRun { run_id } => tagged_id(writer, 4, run_id.as_bytes()),
            C::PauseRun { run_id } => tagged_id(writer, 5, run_id.as_bytes()),
            C::ResumeRun { run_id } => tagged_id(writer, 6, run_id.as_bytes()),
            C::CancelRun { run_id } => tagged_id(writer, 7, run_id.as_bytes()),
            C::FailRun { run_id } => tagged_id(writer, 8, run_id.as_bytes()),
            C::ExhaustRun { run_id } => tagged_id(writer, 9, run_id.as_bytes()),
            C::RejectRun { run_id } => tagged_id(writer, 10, run_id.as_bytes()),
            C::StartAttempt { run_id, attempt_id } => {
                tagged_two_ids(writer, 11, run_id.as_bytes(), attempt_id.as_bytes())
            }
            C::ResumeAttempt { run_id, attempt_id } => {
                tagged_two_ids(writer, 12, run_id.as_bytes(), attempt_id.as_bytes())
            }
            C::SubmitAttempt { run_id, attempt_id } => {
                tagged_two_ids(writer, 13, run_id.as_bytes(), attempt_id.as_bytes())
            }
            C::FailAttempt { run_id, attempt_id } => {
                tagged_two_ids(writer, 14, run_id.as_bytes(), attempt_id.as_bytes())
            }
            C::ExhaustAttempt { run_id, attempt_id } => {
                tagged_two_ids(writer, 15, run_id.as_bytes(), attempt_id.as_bytes())
            }
            C::StartTurn { attempt_id, turn_id } => {
                tagged_two_ids(writer, 16, attempt_id.as_bytes(), turn_id.as_bytes())
            }
            C::CompleteTurn { attempt_id, turn_id } => {
                tagged_two_ids(writer, 17, attempt_id.as_bytes(), turn_id.as_bytes())
            }
            C::FailTurn { attempt_id, turn_id } => {
                tagged_two_ids(writer, 18, attempt_id.as_bytes(), turn_id.as_bytes())
            }
            C::CancelTurn { attempt_id, turn_id } => {
                tagged_two_ids(writer, 19, attempt_id.as_bytes(), turn_id.as_bytes())
            }
            C::ProposeAction { turn_id, action_id, digest, actor_id, role, environment_id } => {
                writer.write_u16(20)?;
                write_id(writer, turn_id.as_bytes())?;
                write_id(writer, action_id.as_bytes())?;
                write_digest(writer, digest)?;
                write_id(writer, actor_id.as_bytes())?;
                write_role(writer, *role)?;
                write_id(writer, environment_id.as_bytes())
            }
            C::AuthorizeAction { action_id } => tagged_id(writer, 21, action_id.as_bytes()),
            C::DispatchAction { action_id } => tagged_id(writer, 22, action_id.as_bytes()),
            C::CompleteAction { action_id } => tagged_id(writer, 23, action_id.as_bytes()),
            C::FailAction { action_id } => tagged_id(writer, 24, action_id.as_bytes()),
            C::CancelAction { action_id } => tagged_id(writer, 25, action_id.as_bytes()),
            C::RequestReview { run_id, attempt_id, review_id } => {
                writer.write_u16(26)?;
                write_id(writer, run_id.as_bytes())?;
                write_id(writer, attempt_id.as_bytes())?;
                write_id(writer, review_id.as_bytes())
            }
            C::BeginReview { review_id } => tagged_id(writer, 27, review_id.as_bytes()),
            C::SubmitReview { review_id } => tagged_id(writer, 28, review_id.as_bytes()),
            C::InvalidateReview { review_id } => tagged_id(writer, 29, review_id.as_bytes()),
            C::RequestWaiver { run_id, review_id, finding_id } => {
                writer.write_u16(30)?;
                write_id(writer, run_id.as_bytes())?;
                write_id(writer, review_id.as_bytes())?;
                write_id(writer, finding_id.as_bytes())
            }
            C::GrantWaiver { finding_id } => tagged_id(writer, 31, finding_id.as_bytes()),
            C::DenyWaiver { finding_id } => tagged_id(writer, 32, finding_id.as_bytes()),
            C::InvalidateWaiver { finding_id } => tagged_id(writer, 33, finding_id.as_bytes()),
            C::BeginAcceptance { run_id } => tagged_id(writer, 34, run_id.as_bytes()),
            C::EvaluateAcceptance { run_id } => tagged_id(writer, 35, run_id.as_bytes()),
        }
    }
}

impl CanonicalDecode for KernelCommandDto {
    const FAMILY: u16 = 1;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let offset = reader.offset();
        let command = match reader.read_u16()? {
            1 => KernelCommand::PauseSession,
            2 => KernelCommand::ResumeSession,
            3 => KernelCommand::CloseSession,
            4 => KernelCommand::StartRun { run_id: run(reader)? },
            5 => KernelCommand::PauseRun { run_id: run(reader)? },
            6 => KernelCommand::ResumeRun { run_id: run(reader)? },
            7 => KernelCommand::CancelRun { run_id: run(reader)? },
            8 => KernelCommand::FailRun { run_id: run(reader)? },
            9 => KernelCommand::ExhaustRun { run_id: run(reader)? },
            10 => KernelCommand::RejectRun { run_id: run(reader)? },
            11 => {
                let (run_id, attempt_id) = attempt_ids(reader)?;
                KernelCommand::StartAttempt { run_id, attempt_id }
            }
            12 => {
                let (run_id, attempt_id) = attempt_ids(reader)?;
                KernelCommand::ResumeAttempt { run_id, attempt_id }
            }
            13 => {
                let (run_id, attempt_id) = attempt_ids(reader)?;
                KernelCommand::SubmitAttempt { run_id, attempt_id }
            }
            14 => {
                let (run_id, attempt_id) = attempt_ids(reader)?;
                KernelCommand::FailAttempt { run_id, attempt_id }
            }
            15 => {
                let (run_id, attempt_id) = attempt_ids(reader)?;
                KernelCommand::ExhaustAttempt { run_id, attempt_id }
            }
            16 => {
                let (attempt_id, turn_id) = turn_ids(reader)?;
                KernelCommand::StartTurn { attempt_id, turn_id }
            }
            17 => {
                let (attempt_id, turn_id) = turn_ids(reader)?;
                KernelCommand::CompleteTurn { attempt_id, turn_id }
            }
            18 => {
                let (attempt_id, turn_id) = turn_ids(reader)?;
                KernelCommand::FailTurn { attempt_id, turn_id }
            }
            19 => {
                let (attempt_id, turn_id) = turn_ids(reader)?;
                KernelCommand::CancelTurn { attempt_id, turn_id }
            }
            20 => KernelCommand::ProposeAction {
                turn_id: read_id(reader, TurnId::new)?,
                action_id: action(reader)?,
                digest: read_digest(reader)?,
                actor_id: read_id(reader, ActorId::new)?,
                role: read_role(reader)?,
                environment_id: read_id(reader, EnvironmentId::new)?,
            },
            21 => KernelCommand::AuthorizeAction { action_id: action(reader)? },
            22 => KernelCommand::DispatchAction { action_id: action(reader)? },
            23 => KernelCommand::CompleteAction { action_id: action(reader)? },
            24 => KernelCommand::FailAction { action_id: action(reader)? },
            25 => KernelCommand::CancelAction { action_id: action(reader)? },
            26 => KernelCommand::RequestReview {
                run_id: run(reader)?,
                attempt_id: read_id(reader, AttemptId::new)?,
                review_id: review(reader)?,
            },
            27 => KernelCommand::BeginReview { review_id: review(reader)? },
            28 => KernelCommand::SubmitReview { review_id: review(reader)? },
            29 => KernelCommand::InvalidateReview { review_id: review(reader)? },
            30 => KernelCommand::RequestWaiver {
                run_id: run(reader)?,
                review_id: review(reader)?,
                finding_id: finding(reader)?,
            },
            31 => KernelCommand::GrantWaiver { finding_id: finding(reader)? },
            32 => KernelCommand::DenyWaiver { finding_id: finding(reader)? },
            33 => KernelCommand::InvalidateWaiver { finding_id: finding(reader)? },
            34 => KernelCommand::BeginAcceptance { run_id: run(reader)? },
            35 => KernelCommand::EvaluateAcceptance { run_id: run(reader)? },
            _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
        };
        Ok(Self(command))
    }
}

fn tagged_id(writer: &mut CanonicalWriter, tag: u16, id: &[u8; 16]) -> Result<(), CodecError> {
    writer.write_u16(tag)?;
    write_id(writer, id)
}

fn tagged_two_ids(
    writer: &mut CanonicalWriter,
    tag: u16,
    first: &[u8; 16],
    second: &[u8; 16],
) -> Result<(), CodecError> {
    writer.write_u16(tag)?;
    write_id(writer, first)?;
    write_id(writer, second)
}

fn run(reader: &mut CanonicalReader<'_>) -> Result<RunId, CodecError> {
    read_id(reader, RunId::new)
}

fn action(reader: &mut CanonicalReader<'_>) -> Result<ActionId, CodecError> {
    read_id(reader, ActionId::new)
}

fn review(reader: &mut CanonicalReader<'_>) -> Result<ReviewCycleId, CodecError> {
    read_id(reader, ReviewCycleId::new)
}

fn finding(reader: &mut CanonicalReader<'_>) -> Result<FindingId, CodecError> {
    read_id(reader, FindingId::new)
}

fn attempt_ids(reader: &mut CanonicalReader<'_>) -> Result<(RunId, AttemptId), CodecError> {
    Ok((run(reader)?, read_id(reader, AttemptId::new)?))
}

fn turn_ids(reader: &mut CanonicalReader<'_>) -> Result<(AttemptId, TurnId), CodecError> {
    Ok((read_id(reader, AttemptId::new)?, read_id(reader, TurnId::new)?))
}
