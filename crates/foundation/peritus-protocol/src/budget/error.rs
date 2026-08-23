//! Canonical inert budget failures.

#![allow(
    clippy::missing_errors_doc,
    reason = "budget error codecs use the shared CodecError vocabulary"
)]

use crate::SCHEMA_V1;
use crate::primitive::{read_option_id, write_option_id};
use peritus_budget::{ArithmeticKind, BudgetDimension, BudgetError, BudgetErrorKind};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{BudgetId, BudgetReservationId};

/// Arithmetic context carried by an inert budget failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AmountArithmeticErrorDto {
    /// Overflow or underflow.
    pub kind: ArithmeticKind,
    /// Failed budget dimension.
    pub dimension: BudgetDimension,
}

/// Budget failure decoded as diagnostic data, never as reducer provenance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetErrorDto {
    /// Stable rejection category.
    pub kind: BudgetErrorKind,
    /// Affected account, when known.
    pub budget_id: Option<BudgetId>,
    /// Affected reservation, when known.
    pub reservation_id: Option<BudgetReservationId>,
    /// Closed dimension-membership vector.
    pub limiting_dimensions: [bool; 5],
    /// Arithmetic context, when applicable.
    pub arithmetic: Option<AmountArithmeticErrorDto>,
}

impl From<BudgetError> for BudgetErrorDto {
    fn from(error: BudgetError) -> Self {
        let dimensions = error.limiting_dimensions();
        Self {
            kind: error.kind(),
            budget_id: error.budget_id(),
            reservation_id: error.reservation_id(),
            limiting_dimensions: [
                dimensions.contains(BudgetDimension::ModelTokens),
                dimensions.contains(BudgetDimension::ProviderCostMicrounits),
                dimensions.contains(BudgetDimension::ActiveEffectMilliseconds),
                dimensions.contains(BudgetDimension::Attempts),
                dimensions.contains(BudgetDimension::Retries),
            ],
            arithmetic: error.arithmetic_error().map(|value| AmountArithmeticErrorDto {
                kind: value.kind(),
                dimension: value.dimension(),
            }),
        }
    }
}

impl CanonicalEncode for BudgetErrorDto {
    const FAMILY: u16 = 15;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_u16(error_tag(self.kind))?;
        write_option_id(writer, self.budget_id, BudgetId::into_bytes)?;
        write_option_id(writer, self.reservation_id, BudgetReservationId::into_bytes)?;
        for present in self.limiting_dimensions {
            writer.write_bool(present)?;
        }
        writer.write_option_tag(self.arithmetic.is_some())?;
        if let Some(arithmetic) = self.arithmetic {
            writer.write_u16(arithmetic_kind_tag(arithmetic.kind))?;
            writer.write_u16(dimension_tag(arithmetic.dimension))?;
        }
        Ok(())
    }
}

impl CanonicalDecode for BudgetErrorDto {
    const FAMILY: u16 = 15;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let kind = read_error_kind(reader)?;
        let budget_id = read_option_id(reader, BudgetId::new)?;
        let reservation_id = read_option_id(reader, BudgetReservationId::new)?;
        let limiting_dimensions = [
            reader.read_bool()?,
            reader.read_bool()?,
            reader.read_bool()?,
            reader.read_bool()?,
            reader.read_bool()?,
        ];
        let arithmetic = if reader.read_option_tag()? {
            Some(AmountArithmeticErrorDto {
                kind: read_arithmetic_kind(reader)?,
                dimension: read_dimension(reader)?,
            })
        } else {
            None
        };
        Ok(Self { kind, budget_id, reservation_id, limiting_dimensions, arithmetic })
    }
}

const fn error_tag(kind: BudgetErrorKind) -> u16 {
    match kind {
        BudgetErrorKind::UnknownBudget => 1,
        BudgetErrorKind::UnknownReservation => 2,
        BudgetErrorKind::DuplicateBudgetConflict => 3,
        BudgetErrorKind::DuplicateReservationConflict => 4,
        BudgetErrorKind::EmptyRequest => 5,
        BudgetErrorKind::InvalidAttemptAccounting => 6,
        BudgetErrorKind::AccountNotOpen => 7,
        BudgetErrorKind::InsufficientBudget => 8,
        BudgetErrorKind::InvalidReservationPhase => 9,
        BudgetErrorKind::InvalidAccountPhase => 10,
        BudgetErrorKind::PriorAttemptUnresolved => 11,
        BudgetErrorKind::BindingMismatch => 12,
        BudgetErrorKind::NonmonotonicObservation => 13,
        BudgetErrorKind::OutstandingWork => 14,
        BudgetErrorKind::Arithmetic => 15,
        BudgetErrorKind::CorruptState => 16,
    }
}

fn read_error_kind(reader: &mut CanonicalReader<'_>) -> Result<BudgetErrorKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(BudgetErrorKind::UnknownBudget),
        2 => Ok(BudgetErrorKind::UnknownReservation),
        3 => Ok(BudgetErrorKind::DuplicateBudgetConflict),
        4 => Ok(BudgetErrorKind::DuplicateReservationConflict),
        5 => Ok(BudgetErrorKind::EmptyRequest),
        6 => Ok(BudgetErrorKind::InvalidAttemptAccounting),
        7 => Ok(BudgetErrorKind::AccountNotOpen),
        8 => Ok(BudgetErrorKind::InsufficientBudget),
        9 => Ok(BudgetErrorKind::InvalidReservationPhase),
        10 => Ok(BudgetErrorKind::InvalidAccountPhase),
        11 => Ok(BudgetErrorKind::PriorAttemptUnresolved),
        12 => Ok(BudgetErrorKind::BindingMismatch),
        13 => Ok(BudgetErrorKind::NonmonotonicObservation),
        14 => Ok(BudgetErrorKind::OutstandingWork),
        15 => Ok(BudgetErrorKind::Arithmetic),
        16 => Ok(BudgetErrorKind::CorruptState),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn arithmetic_kind_tag(kind: ArithmeticKind) -> u16 {
    match kind {
        ArithmeticKind::Overflow => 1,
        ArithmeticKind::Underflow => 2,
    }
}
fn read_arithmetic_kind(reader: &mut CanonicalReader<'_>) -> Result<ArithmeticKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(ArithmeticKind::Overflow),
        2 => Ok(ArithmeticKind::Underflow),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
const fn dimension_tag(dimension: BudgetDimension) -> u16 {
    match dimension {
        BudgetDimension::ModelTokens => 1,
        BudgetDimension::ProviderCostMicrounits => 2,
        BudgetDimension::ActiveEffectMilliseconds => 3,
        BudgetDimension::Attempts => 4,
        BudgetDimension::Retries => 5,
    }
}
fn read_dimension(reader: &mut CanonicalReader<'_>) -> Result<BudgetDimension, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(BudgetDimension::ModelTokens),
        2 => Ok(BudgetDimension::ProviderCostMicrounits),
        3 => Ok(BudgetDimension::ActiveEffectMilliseconds),
        4 => Ok(BudgetDimension::Attempts),
        5 => Ok(BudgetDimension::Retries),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
