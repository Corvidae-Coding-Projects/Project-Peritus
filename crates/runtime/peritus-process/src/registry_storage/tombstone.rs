//! Immutable exact consumption and terminal receipts, published before active-file retirement.

use std::{fs, io::Write as _, path::{Path, PathBuf}};

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{
    ErrorCode, ProcessError, ProcessOperation, RecoveryClass,
    recovery::{claim::ConsumptionClaim, manifest::ExecutionManifest},
};
use super::{
    decode_recovery_manifest, encode_recovery_manifest, hex, quarantine_path,
    read_bounded_canonical_file, store_error, sync_directory,
};
use crate::consumption::store_cause;

const MAGIC: &[u8] = b"PERITUS-PROCESS-TOMBSTONE-V1\0";
const MAX_TOMBSTONE_ROOT_BYTES: usize = 32 * 1_024;

pub(crate) fn persist_tombstone(
    directory: &Path,
    claim: ConsumptionClaim,
    manifest: &ExecutionManifest,
) -> Result<(), ProcessError> {
    if !claim.matches_manifest(manifest) || !crate::consumption::retirable(manifest) {
        return Err(store_error("process retirement requires settled published ownership"));
    }
    let claim_bytes = claim.encode();
    let manifest_bytes = encode_recovery_manifest(directory, manifest)?;
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
        let existing = read_bounded_canonical_file(
            &target,
            "process tombstone cannot be reread",
            MAX_TOMBSTONE_ROOT_BYTES,
        )?;
        if existing.as_deref() != Some(bytes.as_slice()) {
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
        Err(error) if error.code() == ErrorCode::CorruptRecovery => {
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
        Err(error) => Err(error),
    }
}

pub(crate) fn load_tombstone(
    directory: &Path,
    process_id: peritus_types::ProcessId,
) -> Result<Option<(ConsumptionClaim, ExecutionManifest)>, ProcessError> {
    let path = directory.join(format!("{}.tombstone", hex(process_id.as_bytes())));
    let Some(bytes) = read_bounded_canonical_file(
        &path,
        "process tombstone cannot be read",
        MAX_TOMBSTONE_ROOT_BYTES,
    )? else {
        return Ok(None);
    };
    let (claim, manifest) = decode(directory, &bytes)?;
    if claim.process_id() != process_id || manifest.identity.process_id() != process_id {
        return Err(store_error("process tombstone identity differs from its path"));
    }
    Ok(Some((claim, manifest)))
}

fn decode(
    directory: &Path,
    bytes: &[u8],
) -> Result<(ConsumptionClaim, ExecutionManifest), ProcessError> {
    let payload_end = bytes.len().checked_sub(Sha256Digest::LENGTH)
        .ok_or_else(|| tombstone_corrupt("process tombstone is truncated"))?;
    let expected: [u8; 32] = Sha256::digest(&bytes[..payload_end]).into();
    if !bytes.starts_with(MAGIC) || payload_end < MAGIC.len() || bytes[payload_end..] != expected {
        return Err(tombstone_corrupt("process tombstone framing or checksum is invalid"));
    }
    let mut offset = MAGIC.len();
    let mut part = || -> Result<&[u8], ProcessError> {
        let length_end = offset.checked_add(4)
            .ok_or_else(|| tombstone_corrupt("process tombstone offset overflow"))?;
        let length: [u8; 4] = bytes.get(offset..length_end)
            .ok_or_else(|| tombstone_corrupt("process tombstone component is truncated"))?
            .try_into().map_err(|_| tombstone_corrupt("process tombstone length is invalid"))?;
        offset = length_end;
        let end = offset.checked_add(usize::try_from(u32::from_be_bytes(length))
            .map_err(|error| store_cause("process tombstone component is not representable", error))?)
            .ok_or_else(|| tombstone_corrupt("process tombstone offset overflow"))?;
        if end > payload_end {
            return Err(tombstone_corrupt("process tombstone component exceeds its payload"));
        }
        let value = &bytes[offset..end];
        offset = end;
        Ok(value)
    };
    let claim = ConsumptionClaim::decode(part()?)?;
    let manifest = decode_recovery_manifest(directory, part()?)?;
    if offset != payload_end || !claim.matches_manifest(&manifest)
        || !crate::consumption::retirable(&manifest)
    {
        return Err(tombstone_corrupt(
            "process tombstone ownership or terminal binding is invalid",
        ));
    }
    Ok((claim, manifest))
}

const fn tombstone_corrupt(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}
