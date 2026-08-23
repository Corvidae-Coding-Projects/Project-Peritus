//! Canonical inert budget snapshots.

#![allow(
    clippy::missing_errors_doc,
    reason = "budget snapshot codecs use the shared CodecError vocabulary"
)]

use super::command::{finality_tag, read_budget_request, read_finality, write_budget_request};
use super::{read_amounts, read_option_amounts, write_amounts, write_option_amounts};
use crate::SCHEMA_V1;
use crate::primitive::{
    read_id, read_option_digest, read_option_id, read_revision, write_id, write_option_digest,
    write_option_id, write_revision,
};
use peritus_budget::{
    BudgetAccountPhase, BudgetAmounts, BudgetLimits, BudgetRequest, BudgetSnapshot,
    ReservationPhase, ReservationSnapshot, UsageFinality,
};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{BudgetId, Sha256Digest};

/// Account snapshot decoded as read-only data, never as budget authority.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetSnapshotDto {
    /// Account identity.
    pub id: BudgetId,
    /// Direct parent, absent for a root account.
    pub parent_id: Option<BudgetId>,
    /// Exact revision binding.
    pub revision: peritus_types::RevisionTuple,
    /// Immutable account ceiling.
    pub limits: BudgetLimits,
    /// Monotonic consumption.
    pub consumed: BudgetAmounts,
    /// Capacity held by direct reservations.
    pub operation_reserved: BudgetAmounts,
    /// Remaining capacity delegated to descendants.
    pub child_delegated_remaining: BudgetAmounts,
    /// Currently uncommitted capacity.
    pub available: BudgetAmounts,
    /// Account lifecycle phase.
    pub phase: BudgetAccountPhase,
}

impl From<BudgetSnapshot> for BudgetSnapshotDto {
    fn from(snapshot: BudgetSnapshot) -> Self {
        Self {
            id: snapshot.id(),
            parent_id: snapshot.parent_id(),
            revision: snapshot.revision(),
            limits: snapshot.limits(),
            consumed: snapshot.consumed(),
            operation_reserved: snapshot.operation_reserved(),
            child_delegated_remaining: snapshot.child_delegated_remaining(),
            available: snapshot.available(),
            phase: snapshot.phase(),
        }
    }
}

impl CanonicalEncode for BudgetSnapshotDto {
    const FAMILY: u16 = 12;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_id(writer, self.id.as_bytes())?;
        write_option_id(writer, self.parent_id, BudgetId::into_bytes)?;
        write_revision(writer, &self.revision)?;
        write_amounts(writer, self.limits.amounts())?;
        write_amounts(writer, self.consumed)?;
        write_amounts(writer, self.operation_reserved)?;
        write_amounts(writer, self.child_delegated_remaining)?;
        write_amounts(writer, self.available)?;
        writer.write_u16(account_phase_tag(self.phase))
    }
}

impl CanonicalDecode for BudgetSnapshotDto {
    const FAMILY: u16 = 12;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            id: read_id(reader, BudgetId::new)?,
            parent_id: read_option_id(reader, BudgetId::new)?,
            revision: read_revision(reader)?,
            limits: BudgetLimits::new(read_amounts(reader)?),
            consumed: read_amounts(reader)?,
            operation_reserved: read_amounts(reader)?,
            child_delegated_remaining: read_amounts(reader)?,
            available: read_amounts(reader)?,
            phase: read_account_phase(reader)?,
        })
    }
}

/// Reservation snapshot decoded as read-only data, never as execution evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReservationSnapshotDto {
    /// Immutable begin request.
    pub request: BudgetRequest,
    /// Accepted cumulative high-water use.
    pub observed: BudgetAmounts,
    /// Remaining reserved ceiling.
    pub outstanding: BudgetAmounts,
    /// Reservation lifecycle phase.
    pub phase: ReservationPhase,
    /// Accepted activation evidence digest.
    pub activation_evidence: Option<Sha256Digest>,
    /// Latest observation evidence digest.
    pub observation_evidence: Option<Sha256Digest>,
    /// Final evidence digest.
    pub final_evidence: Option<Sha256Digest>,
    /// Raw final cumulative report.
    pub final_reported: Option<BudgetAmounts>,
    /// Finality of the terminal report.
    pub finality: Option<UsageFinality>,
}

impl From<ReservationSnapshot> for ReservationSnapshotDto {
    fn from(snapshot: ReservationSnapshot) -> Self {
        Self {
            request: snapshot.request(),
            observed: snapshot.observed(),
            outstanding: snapshot.outstanding(),
            phase: snapshot.phase(),
            activation_evidence: snapshot.activation_evidence(),
            observation_evidence: snapshot.observation_evidence(),
            final_evidence: snapshot.final_evidence(),
            final_reported: snapshot.final_reported(),
            finality: snapshot.finality(),
        }
    }
}

impl CanonicalEncode for ReservationSnapshotDto {
    const FAMILY: u16 = 13;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_budget_request(writer, self.request)?;
        write_amounts(writer, self.observed)?;
        write_amounts(writer, self.outstanding)?;
        writer.write_u16(reservation_phase_tag(self.phase))?;
        write_option_digest(writer, self.activation_evidence)?;
        write_option_digest(writer, self.observation_evidence)?;
        write_option_digest(writer, self.final_evidence)?;
        write_option_amounts(writer, self.final_reported)?;
        writer.write_option_tag(self.finality.is_some())?;
        if let Some(finality) = self.finality {
            writer.write_u16(finality_tag(finality))?;
        }
        Ok(())
    }
}

impl CanonicalDecode for ReservationSnapshotDto {
    const FAMILY: u16 = 13;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let request = read_budget_request(reader)?;
        let observed = read_amounts(reader)?;
        let outstanding = read_amounts(reader)?;
        let phase = read_reservation_phase(reader)?;
        let activation_evidence = read_option_digest(reader)?;
        let observation_evidence = read_option_digest(reader)?;
        let final_evidence = read_option_digest(reader)?;
        let final_reported = read_option_amounts(reader)?;
        let finality = if reader.read_option_tag()? { Some(read_finality(reader)?) } else { None };
        Ok(Self {
            request,
            observed,
            outstanding,
            phase,
            activation_evidence,
            observation_evidence,
            final_evidence,
            final_reported,
            finality,
        })
    }
}

const fn account_phase_tag(phase: BudgetAccountPhase) -> u16 {
    match phase {
        BudgetAccountPhase::Open => 1,
        BudgetAccountPhase::Draining => 2,
        BudgetAccountPhase::Faulted => 3,
        BudgetAccountPhase::Closed => 4,
    }
}

fn read_account_phase(reader: &mut CanonicalReader<'_>) -> Result<BudgetAccountPhase, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(BudgetAccountPhase::Open),
        2 => Ok(BudgetAccountPhase::Draining),
        3 => Ok(BudgetAccountPhase::Faulted),
        4 => Ok(BudgetAccountPhase::Closed),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn reservation_phase_tag(phase: ReservationPhase) -> u16 {
    match phase {
        ReservationPhase::Held => 1,
        ReservationPhase::Active => 2,
        ReservationPhase::SettledExact => 3,
        ReservationPhase::SettledFinal => 4,
        ReservationPhase::CancelledHeld => 5,
        ReservationPhase::SettledAmbiguous => 6,
        ReservationPhase::OverrunFaulted => 7,
    }
}

fn read_reservation_phase(
    reader: &mut CanonicalReader<'_>,
) -> Result<ReservationPhase, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(ReservationPhase::Held),
        2 => Ok(ReservationPhase::Active),
        3 => Ok(ReservationPhase::SettledExact),
        4 => Ok(ReservationPhase::SettledFinal),
        5 => Ok(ReservationPhase::CancelledHeld),
        6 => Ok(ReservationPhase::SettledAmbiguous),
        7 => Ok(ReservationPhase::OverrunFaulted),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
