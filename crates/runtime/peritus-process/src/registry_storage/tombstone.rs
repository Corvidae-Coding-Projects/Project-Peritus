//! Immutable exact consumption and terminal receipts, published before active-file retirement.

use std::{fs, io::Write as _, path::{Path, PathBuf}};

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{ProcessError, recovery::{claim::ConsumptionClaim, manifest::ExecutionManifest}};
use super::{hex, quarantine_path, store_error, sync_directory};
use crate::consumption::store_cause;

const MAGIC: &[u8] = b"PERITUS-PROCESS-TOMBSTONE-V1\0";

pub(crate) fn persist_tombstone(
    directory: &Path,
    claim: ConsumptionClaim,
    manifest: &ExecutionManifest,
) -> Result<(), ProcessError> {
    if !claim.matches_manifest(manifest) || !crate::consumption::retirable(manifest) {
        return Err(store_error("process retirement requires settled published ownership"));
    }
    let claim_bytes = claim.encode();
    let manifest_bytes = manifest.encode()?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    for part in [&claim_bytes, &manifest_bytes] {
        let length = u32::try_from(part.len())
            .map_err(|error| store_cause("process tombstone component is not representable", error))?;
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(part);
    }
    let checksum: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&checksum);
    let target = directory.join(format!("{}.tombstone", hex(claim.process_id().as_bytes())));
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| store_cause("process tombstone staging cannot be created", error))?;
    temporary.write_all(&bytes).and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| store_cause("process tombstone staging cannot be synchronized", error))?;
    if temporary.persist_noclobber(&target).is_err() {
        let existing_is_regular = fs::symlink_metadata(&target)
            .is_ok_and(|metadata| metadata.file_type().is_file());
        if !existing_is_regular || fs::read(&target).ok().as_deref() != Some(bytes.as_slice()) {
            return Err(store_error("process tombstone conflicts or cannot be published"));
        }
    }
    sync_directory(directory)
}

pub(crate) fn load_canonical_tombstone(
    directory: &Path,
    quarantine: &Path,
    quarantined_identities: &Path,
    quarantined: &mut Vec<PathBuf>,
    process_id: peritus_types::ProcessId,
) -> Result<Option<(ConsumptionClaim, ExecutionManifest)>, ProcessError> {
    match load_tombstone(directory, process_id) {
        Ok(tombstone) => Ok(tombstone),
        Err(error) => {
            let path = directory.join(format!("{}.tombstone", hex(process_id.as_bytes())));
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    quarantine_path(
                        &path,
                        quarantine,
                        quarantined_identities,
                        quarantined,
                    )?;
                    Ok(None)
                }
                Err(inspect_error) if inspect_error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(None)
                }
                Err(_) => Err(error),
            }
        }
    }
}

pub(crate) fn load_tombstone(
    directory: &Path,
    process_id: peritus_types::ProcessId,
) -> Result<Option<(ConsumptionClaim, ExecutionManifest)>, ProcessError> {
    let path = directory.join(format!("{}.tombstone", hex(process_id.as_bytes())));
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(store_cause("process tombstone cannot be inspected", error)),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(store_error("process tombstone is not a regular file"));
    }
    let bytes = fs::read(&path)
        .map_err(|error| store_cause("process tombstone cannot be read", error))?;
    let (claim, manifest) = decode(&bytes)?;
    if claim.process_id() != process_id || manifest.identity.process_id() != process_id {
        return Err(store_error("process tombstone identity differs from its path"));
    }
    Ok(Some((claim, manifest)))
}

fn decode(bytes: &[u8]) -> Result<(ConsumptionClaim, ExecutionManifest), ProcessError> {
    let payload_end = bytes.len().checked_sub(Sha256Digest::LENGTH)
        .ok_or_else(|| store_error("process tombstone is truncated"))?;
    let expected: [u8; 32] = Sha256::digest(&bytes[..payload_end]).into();
    if !bytes.starts_with(MAGIC) || payload_end < MAGIC.len() || bytes[payload_end..] != expected {
        return Err(store_error("process tombstone framing or checksum is invalid"));
    }
    let mut offset = MAGIC.len();
    let mut part = || -> Result<&[u8], ProcessError> {
        let length_end = offset.checked_add(4)
            .ok_or_else(|| store_error("process tombstone offset overflow"))?;
        let length: [u8; 4] = bytes.get(offset..length_end)
            .ok_or_else(|| store_error("process tombstone component is truncated"))?
            .try_into().map_err(|_| store_error("process tombstone length is invalid"))?;
        offset = length_end;
        let end = offset.checked_add(usize::try_from(u32::from_be_bytes(length))
            .map_err(|error| store_cause("process tombstone component is not representable", error))?)
            .ok_or_else(|| store_error("process tombstone offset overflow"))?;
        if end > payload_end { return Err(store_error("process tombstone component exceeds its payload")); }
        let value = &bytes[offset..end];
        offset = end;
        Ok(value)
    };
    let claim = ConsumptionClaim::decode(part()?)?;
    let manifest = ExecutionManifest::decode(part()?)?;
    if offset != payload_end || !claim.matches_manifest(&manifest)
        || !crate::consumption::retirable(&manifest)
    {
        return Err(store_error("process tombstone ownership or terminal binding is invalid"));
    }
    Ok((claim, manifest))
}
