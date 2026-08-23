//! Canonical inert budget transition receipts.

#![allow(
    clippy::missing_errors_doc,
    reason = "budget receipt codecs use the shared CodecError vocabulary"
)]

use super::{read_amounts, read_option_amounts, write_amounts, write_option_amounts};
use crate::SCHEMA_V1;
use crate::primitive::{
    read_id, read_option_digest, read_option_id, write_id, write_option_digest, write_option_id,
};
use peritus_budget::{BudgetAmounts, BudgetOperation, BudgetReceipt, BudgetReceiptKind};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{BudgetId, BudgetReservationId, Sha256Digest};

/// Logical budget receipt decoded as data, never as proof of durable commit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetReceiptDto {
    /// Logical operation.
    pub operation: BudgetOperation,
    /// Accepted outcome class.
    pub kind: BudgetReceiptKind,
    /// Affected account.
    pub budget_id: BudgetId,
    /// Reservation identity, when applicable.
    pub reservation_id: Option<BudgetReservationId>,
    /// Newly authoritative logical consumption.
    pub charged: BudgetAmounts,
    /// Capacity released without reducing consumption.
    pub released: BudgetAmounts,
    /// Raw cumulative report, when applicable.
    pub reported: Option<BudgetAmounts>,
    /// Correlated evidence digest, when applicable.
    pub evidence_digest: Option<Sha256Digest>,
}

impl From<BudgetReceipt> for BudgetReceiptDto {
    fn from(receipt: BudgetReceipt) -> Self {
        Self {
            operation: receipt.operation(),
            kind: receipt.kind(),
            budget_id: receipt.budget_id(),
            reservation_id: receipt.reservation_id(),
            charged: receipt.charged(),
            released: receipt.released(),
            reported: receipt.reported(),
            evidence_digest: receipt.evidence_digest(),
        }
    }
}

impl CanonicalEncode for BudgetReceiptDto {
    const FAMILY: u16 = 14;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_u16(operation_tag(self.operation))?;
        writer.write_u16(kind_tag(self.kind))?;
        write_id(writer, self.budget_id.as_bytes())?;
        write_option_id(writer, self.reservation_id, BudgetReservationId::into_bytes)?;
        write_amounts(writer, self.charged)?;
        write_amounts(writer, self.released)?;
        write_option_amounts(writer, self.reported)?;
        write_option_digest(writer, self.evidence_digest)
    }
}

impl CanonicalDecode for BudgetReceiptDto {
    const FAMILY: u16 = 14;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            operation: read_operation(reader)?,
            kind: read_kind(reader)?,
            budget_id: read_id(reader, BudgetId::new)?,
            reservation_id: read_option_id(reader, BudgetReservationId::new)?,
            charged: read_amounts(reader)?,
            released: read_amounts(reader)?,
            reported: read_option_amounts(reader)?,
            evidence_digest: read_option_digest(reader)?,
        })
    }
}

const fn operation_tag(operation: BudgetOperation) -> u16 {
    match operation {
        BudgetOperation::AllocateChild => 1,
        BudgetOperation::Begin => 2,
        BudgetOperation::Activate => 3,
        BudgetOperation::ObserveUsage => 4,
        BudgetOperation::SettleExact => 5,
        BudgetOperation::CancelHeld => 6,
        BudgetOperation::FinalizeAmbiguous => 7,
        BudgetOperation::Seal => 8,
        BudgetOperation::Close => 9,
    }
}

fn read_operation(reader: &mut CanonicalReader<'_>) -> Result<BudgetOperation, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(BudgetOperation::AllocateChild),
        2 => Ok(BudgetOperation::Begin),
        3 => Ok(BudgetOperation::Activate),
        4 => Ok(BudgetOperation::ObserveUsage),
        5 => Ok(BudgetOperation::SettleExact),
        6 => Ok(BudgetOperation::CancelHeld),
        7 => Ok(BudgetOperation::FinalizeAmbiguous),
        8 => Ok(BudgetOperation::Seal),
        9 => Ok(BudgetOperation::Close),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn kind_tag(kind: BudgetReceiptKind) -> u16 {
    match kind {
        BudgetReceiptKind::Applied => 1,
        BudgetReceiptKind::Idempotent => 2,
        BudgetReceiptKind::OverrunFaulted => 3,
    }
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<BudgetReceiptKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(BudgetReceiptKind::Applied),
        2 => Ok(BudgetReceiptKind::Idempotent),
        3 => Ok(BudgetReceiptKind::OverrunFaulted),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
