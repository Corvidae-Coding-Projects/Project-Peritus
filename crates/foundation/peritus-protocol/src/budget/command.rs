//! Canonical budget reducer requests and unprivileged evidence claims.

#![allow(
    clippy::missing_errors_doc,
    reason = "budget command codecs use the shared CodecError vocabulary"
)]

use super::{read_amounts, write_amounts};
use crate::SCHEMA_V1;
use crate::primitive::{
    read_digest, read_id, read_revision, write_digest, write_id, write_revision,
};
use peritus_budget::{
    Activation, AmbiguousFinalization, BudgetCommand, BudgetLimits, BudgetRequest,
    ChildBudgetRequest, ReservationReference, UsageFinality, UsageObservation,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{ActionId, BudgetId, BudgetReservationId};

/// Complete closed B1 budget command DTO.
///
/// Decoded evidence fields remain caller claims. The budget reducer and C0 commit boundary decide
/// whether they match authoritative state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetCommandDto(BudgetCommand);

impl BudgetCommandDto {
    /// Returns the checked, still-unprivileged reducer request.
    #[must_use]
    pub const fn into_domain(self) -> BudgetCommand {
        self.0
    }
}

impl From<BudgetCommand> for BudgetCommandDto {
    fn from(command: BudgetCommand) -> Self {
        Self(command)
    }
}

impl CanonicalEncode for BudgetCommandDto {
    const FAMILY: u16 = 10;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        match self.0 {
            BudgetCommand::AllocateChild(request) => {
                writer.write_u16(1)?;
                write_id(writer, request.child_id().as_bytes())?;
                write_id(writer, request.parent_id().as_bytes())?;
                write_revision(writer, &request.revision())?;
                write_amounts(writer, request.limits().amounts())
            }
            BudgetCommand::Begin(request) => {
                writer.write_u16(2)?;
                write_budget_request(writer, request)
            }
            BudgetCommand::Activate(activation) => {
                writer.write_u16(3)?;
                write_reference_fields(
                    writer,
                    activation.reservation_id(),
                    activation.action_id(),
                    activation.action_digest(),
                    activation.evidence_digest(),
                )
            }
            BudgetCommand::ObserveUsage(observation) => {
                writer.write_u16(4)?;
                write_reference_fields(
                    writer,
                    observation.reservation_id(),
                    observation.action_id(),
                    observation.action_digest(),
                    observation.evidence_digest(),
                )?;
                write_amounts(writer, observation.cumulative())?;
                writer.write_u16(finality_tag(observation.finality()))
            }
            BudgetCommand::SettleExact(reference) => {
                writer.write_u16(5)?;
                write_reference(writer, reference)
            }
            BudgetCommand::CancelHeld(reference) => {
                writer.write_u16(6)?;
                write_reference(writer, reference)
            }
            BudgetCommand::FinalizeAmbiguous(finalization) => {
                writer.write_u16(7)?;
                write_reference(writer, finalization.reference())
            }
            BudgetCommand::Seal(id) => tagged_budget_id(writer, 8, id),
            BudgetCommand::Close(id) => tagged_budget_id(writer, 9, id),
        }
    }
}

impl CanonicalDecode for BudgetCommandDto {
    const FAMILY: u16 = 10;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let offset = reader.offset();
        let command = match reader.read_u16()? {
            1 => BudgetCommand::AllocateChild(ChildBudgetRequest::new(
                read_id(reader, BudgetId::new)?,
                read_id(reader, BudgetId::new)?,
                read_revision(reader)?,
                BudgetLimits::new(read_amounts(reader)?),
            )),
            2 => BudgetCommand::Begin(read_budget_request(reader)?),
            3 => {
                let (reservation_id, action_id, action_digest, evidence_digest) =
                    read_reference_fields(reader)?;
                BudgetCommand::Activate(Activation::new(
                    reservation_id,
                    action_id,
                    action_digest,
                    evidence_digest,
                ))
            }
            4 => {
                let (reservation_id, action_id, action_digest, evidence_digest) =
                    read_reference_fields(reader)?;
                let cumulative = read_amounts(reader)?;
                let finality = read_finality(reader)?;
                BudgetCommand::ObserveUsage(UsageObservation::new(
                    reservation_id,
                    action_id,
                    action_digest,
                    evidence_digest,
                    cumulative,
                    finality,
                ))
            }
            5 => BudgetCommand::SettleExact(read_reference(reader)?),
            6 => BudgetCommand::CancelHeld(read_reference(reader)?),
            7 => BudgetCommand::FinalizeAmbiguous(AmbiguousFinalization::new(read_reference(
                reader,
            )?)),
            8 => BudgetCommand::Seal(read_id(reader, BudgetId::new)?),
            9 => BudgetCommand::Close(read_id(reader, BudgetId::new)?),
            _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
        };
        Ok(Self(command))
    }
}

pub fn write_budget_request(
    writer: &mut CanonicalWriter,
    request: BudgetRequest,
) -> Result<(), CodecError> {
    write_id(writer, request.reservation_id().as_bytes())?;
    write_id(writer, request.budget_id().as_bytes())?;
    write_revision(writer, &request.revision())?;
    write_id(writer, request.action_id().as_bytes())?;
    write_digest(writer, &request.action_digest())?;
    write_amounts(writer, request.consume_now())?;
    write_amounts(writer, request.reserve())
}

pub fn read_budget_request(reader: &mut CanonicalReader<'_>) -> Result<BudgetRequest, CodecError> {
    Ok(BudgetRequest::new(
        read_id(reader, BudgetReservationId::new)?,
        read_id(reader, BudgetId::new)?,
        read_revision(reader)?,
        read_id(reader, ActionId::new)?,
        read_digest(reader)?,
        read_amounts(reader)?,
        read_amounts(reader)?,
    ))
}

fn write_reference(
    writer: &mut CanonicalWriter,
    reference: ReservationReference,
) -> Result<(), CodecError> {
    write_reference_fields(
        writer,
        reference.reservation_id(),
        reference.action_id(),
        reference.action_digest(),
        reference.evidence_digest(),
    )
}

fn write_reference_fields(
    writer: &mut CanonicalWriter,
    reservation_id: BudgetReservationId,
    action_id: ActionId,
    action_digest: peritus_types::Sha256Digest,
    evidence_digest: peritus_types::Sha256Digest,
) -> Result<(), CodecError> {
    write_id(writer, reservation_id.as_bytes())?;
    write_id(writer, action_id.as_bytes())?;
    write_digest(writer, &action_digest)?;
    write_digest(writer, &evidence_digest)
}

fn read_reference_fields(
    reader: &mut CanonicalReader<'_>,
) -> Result<
    (BudgetReservationId, ActionId, peritus_types::Sha256Digest, peritus_types::Sha256Digest),
    CodecError,
> {
    Ok((
        read_id(reader, BudgetReservationId::new)?,
        read_id(reader, ActionId::new)?,
        read_digest(reader)?,
        read_digest(reader)?,
    ))
}

fn read_reference(reader: &mut CanonicalReader<'_>) -> Result<ReservationReference, CodecError> {
    let (reservation_id, action_id, action_digest, evidence_digest) =
        read_reference_fields(reader)?;
    Ok(ReservationReference::new(reservation_id, action_id, action_digest, evidence_digest))
}

fn tagged_budget_id(
    writer: &mut CanonicalWriter,
    tag: u16,
    id: BudgetId,
) -> Result<(), CodecError> {
    writer.write_u16(tag)?;
    write_id(writer, id.as_bytes())
}

pub const fn finality_tag(finality: UsageFinality) -> u16 {
    match finality {
        UsageFinality::Interim => 1,
        UsageFinality::Final => 2,
    }
}

pub fn read_finality(reader: &mut CanonicalReader<'_>) -> Result<UsageFinality, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(UsageFinality::Interim),
        2 => Ok(UsageFinality::Final),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
