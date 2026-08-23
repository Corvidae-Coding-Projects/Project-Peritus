//! Canonical fixed-dimensional budget values.

#![allow(
    clippy::missing_errors_doc,
    reason = "budget amount codecs use the shared CodecError vocabulary"
)]

use crate::SCHEMA_V1;
use peritus_budget::{BudgetAmounts, BudgetDimension};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError,
};

/// Complete five-dimensional budget amount vector.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetAmountsDto(BudgetAmounts);

impl BudgetAmountsDto {
    /// Returns the checked domain amount.
    #[must_use]
    pub const fn into_domain(self) -> BudgetAmounts {
        self.0
    }
}

impl From<BudgetAmounts> for BudgetAmountsDto {
    fn from(value: BudgetAmounts) -> Self {
        Self(value)
    }
}

impl CanonicalEncode for BudgetAmountsDto {
    const FAMILY: u16 = 11;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_amounts(writer, self.0)
    }
}

impl CanonicalDecode for BudgetAmountsDto {
    const FAMILY: u16 = 11;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_amounts(reader).map(Self)
    }
}

pub fn write_amounts(writer: &mut CanonicalWriter, value: BudgetAmounts) -> Result<(), CodecError> {
    writer.write_u64(value.get(BudgetDimension::ModelTokens).get())?;
    writer.write_u64(value.get(BudgetDimension::ProviderCostMicrounits).get())?;
    writer.write_u64(value.get(BudgetDimension::ActiveEffectMilliseconds).get())?;
    writer.write_u64(value.get(BudgetDimension::Attempts).get())?;
    writer.write_u64(value.get(BudgetDimension::Retries).get())
}

pub fn read_amounts(reader: &mut CanonicalReader<'_>) -> Result<BudgetAmounts, CodecError> {
    Ok(BudgetAmounts::from_units(
        reader.read_u64()?,
        reader.read_u64()?,
        reader.read_u64()?,
        reader.read_u64()?,
        reader.read_u64()?,
    ))
}

pub fn write_option_amounts(
    writer: &mut CanonicalWriter,
    value: Option<BudgetAmounts>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        write_amounts(writer, value)?;
    }
    Ok(())
}

pub fn read_option_amounts(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<BudgetAmounts>, CodecError> {
    if reader.read_option_tag()? { read_amounts(reader).map(Some) } else { Ok(None) }
}
