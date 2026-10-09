//! Checked manifest collection decoding for preserved legacy and scalable encodings.

use super::{
    ArtifactManifestEntry, JournalManifestEntry, LEGACY_MAX_MANIFEST_ENTRIES, RecordManifestEntry,
    invalid,
};
use crate::canonical::Reader;
use crate::{EvidenceError, EvidenceId};
use peritus_artifact_store::ArtifactDigest;

pub(super) fn decode_records(
    reader: &mut Reader<'_>,
    legacy: bool,
) -> Result<Vec<RecordManifestEntry>, EvidenceError> {
    let value = reader.u64()?;
    let count = count(reader, value, 48, legacy)?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| invalid("manifest record allocation failed"))?;
    for _ in 0..count {
        values.push(RecordManifestEntry::new(reader.evidence_id()?, reader.digest()?));
    }
    Ok(values)
}
pub(super) fn decode_journal(
    reader: &mut Reader<'_>,
    legacy: bool,
) -> Result<Vec<JournalManifestEntry>, EvidenceError> {
    let value = reader.u64()?;
    let count = count(reader, value, 128, legacy)?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| invalid("manifest journal allocation failed"))?;
    for _ in 0..count {
        values.push(JournalManifestEntry::new(
            reader.u64()?,
            reader.event_id()?,
            reader.digest()?,
            reader.digest()?,
            reader.digest()?,
            reader.u64()?,
        ));
    }
    Ok(values)
}
pub(super) fn decode_artifacts(
    reader: &mut Reader<'_>,
    legacy: bool,
) -> Result<Vec<ArtifactManifestEntry>, EvidenceError> {
    let value = reader.u64()?;
    let count = count(reader, value, 40, legacy)?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| invalid("manifest artifact allocation failed"))?;
    for _ in 0..count {
        values.push(ArtifactManifestEntry::new(
            ArtifactDigest::from_sha256(reader.digest()?),
            reader.u64()?,
        ));
    }
    Ok(values)
}
pub(super) fn decode_authority(reader: &mut Reader<'_>) -> Result<Vec<EvidenceId>, EvidenceError> {
    let value = reader.u64()?;
    let count = count(reader, value, 16, false)?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| invalid("manifest authority allocation failed"))?;
    for _ in 0..count {
        values.push(reader.evidence_id()?);
    }
    Ok(values)
}
fn count(
    reader: &Reader<'_>,
    value: u64,
    entry_bytes: u64,
    legacy: bool,
) -> Result<usize, EvidenceError> {
    let count = usize::try_from(value).map_err(|_| invalid("manifest count overflows"))?;
    if legacy && count > LEGACY_MAX_MANIFEST_ENTRIES {
        return Err(invalid("legacy manifest count exceeds bound"));
    }
    let required = value
        .checked_mul(entry_bytes)
        .ok_or_else(|| invalid("manifest collection size overflows"))?;
    let remaining = u64::try_from(reader.remaining())
        .map_err(|_| invalid("manifest remaining bytes exceed u64"))?;
    if required > remaining {
        return Err(invalid("manifest count exceeds remaining bytes"));
    }
    Ok(count)
}
