//! Exact immutable query and command-resolution paths.

mod aggregate;
mod command;
mod records;
mod state;

use peritus_types::{EventId, Sha256Digest};

use crate::{JournalError, JournalErrorKind};

pub use aggregate::parse_head;
pub use command::resolve_command;
pub use records::load_records_range;

pub fn digest_from_blob(bytes: &[u8], _field: &'static str) -> Result<Sha256Digest, JournalError> {
    Ok(Sha256Digest::new(array_from_blob(bytes, "digest")?))
}

pub fn event_id_from_blob(bytes: &[u8], _field: &'static str) -> Result<EventId, JournalError> {
    EventId::new(array_from_blob(bytes, "event identity")?)
        .map_err(|_| corrupt("stored event identity is invalid"))
}

pub fn causal_ids_from_blob(bytes: &[u8]) -> Result<Vec<EventId>, JournalError> {
    if !bytes.len().is_multiple_of(EventId::LENGTH) {
        return Err(corrupt("causal identity blob has an invalid length"));
    }
    bytes
        .chunks_exact(EventId::LENGTH)
        .map(|chunk| event_id_from_blob(chunk, "causal event identity"))
        .collect()
}

pub fn array_from_blob<const N: usize>(
    bytes: &[u8],
    _field: &'static str,
) -> Result<[u8; N], JournalError> {
    bytes.try_into().map_err(|_| corrupt("stored fixed-length field has an invalid length"))
}

pub fn positive_u64(value: i64, _field: &'static str) -> Result<u64, JournalError> {
    let converted =
        u64::try_from(value).map_err(|_| corrupt("stored positive integer is negative"))?;
    if converted == 0 { Err(corrupt("stored positive integer is zero")) } else { Ok(converted) }
}

pub const fn corrupt(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::CorruptJournal, "validate stored journal", detail)
}
