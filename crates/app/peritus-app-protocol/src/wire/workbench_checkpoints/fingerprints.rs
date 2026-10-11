//! Page-independent checkpoint and rewind fingerprints.

use super::{
    write_checkpoint_path, write_digest, write_id, write_references, write_request,
    write_rewind_path,
};
use crate::{WorkbenchCheckpointReceipt, WorkbenchRewindPath, WorkbenchRewindRequest};
use peritus_codec::{CanonicalWriter, CodecError, CodecErrorKind, CodecLimit, CodecLimits};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

pub fn preview_fingerprint(
    request: WorkbenchRewindRequest,
    paths: &[WorkbenchRewindPath],
    exclusions: &[String],
    external: &[String],
) -> Result<Sha256Digest, CodecError> {
    if paths.len() > usize::from(u16::MAX)
        || exclusions.len() > usize::from(u16::MAX)
        || external.len() > usize::from(u16::MAX)
    {
        return large_preview_fingerprint(request, paths, exclusions, external);
    }
    let legacy_digest = (|| -> Result<Sha256Digest, CodecError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(b"peritus-workbench-rewind-preview-v1")?;
        write_request(&mut writer, request)?;
        super::write_rewind_paths(&mut writer, paths)?;
        super::write_strings(&mut writer, exclusions)?;
        super::write_strings(&mut writer, external)?;
        Ok(peritus_codec::sha256(writer.as_slice()))
    })();
    match legacy_digest {
        Ok(digest) => Ok(digest),
        Err(error) if error.limit() == Some(CodecLimit::PayloadBytes) => {
            large_preview_fingerprint(request, paths, exclusions, external)
        }
        Err(error) => Err(error),
    }
}

/// Hashes every immutable checkpoint fact incrementally, without a whole-list count ceiling.
pub fn checkpoint_fingerprint(
    receipt: &WorkbenchCheckpointReceipt,
) -> Result<Sha256Digest, CodecError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-workbench-checkpoint-coverage-v1\0");
    let mut identity = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_id(&mut identity, receipt.checkpoint().as_bytes())?;
    super::super::workbench::write_query(&mut identity, receipt.query())?;
    identity.write_u64(receipt.accepted_revision())?;
    identity.write_str(receipt.name().as_str())?;
    write_references(&mut identity, receipt.references())?;
    let paths = count_u64(receipt.paths().len(), identity.len())?;
    let exclusions = count_u64(receipt.exclusions().len(), identity.len())?;
    let effects = count_u64(receipt.external_effects().len(), identity.len())?;
    identity.write_u64(paths)?;
    identity.write_u64(exclusions)?;
    identity.write_u64(effects)?;
    hasher.update(identity.as_slice());

    for path in receipt.paths() {
        let mut fact = CanonicalWriter::new(CodecLimits::PRODUCTION);
        fact.write_u16(1)?;
        write_checkpoint_path(&mut fact, path)?;
        hasher.update(fact.as_slice());
    }
    for exclusion in receipt.exclusions() {
        let mut fact = CanonicalWriter::new(CodecLimits::PRODUCTION);
        fact.write_u16(2)?;
        fact.write_str(exclusion)?;
        hasher.update(fact.as_slice());
    }
    for effect in receipt.external_effects() {
        let mut fact = CanonicalWriter::new(CodecLimits::PRODUCTION);
        fact.write_u16(3)?;
        fact.write_str(effect)?;
        hasher.update(fact.as_slice());
    }
    Ok(finish_fingerprint(hasher))
}

/// Binds a checkpoint coverage cursor to the selected conversation snapshot revision.
pub fn checkpoint_page_fingerprint(
    receipt: &WorkbenchCheckpointReceipt,
    selected_revision: u64,
) -> Result<Sha256Digest, CodecError> {
    let manifest = checkpoint_fingerprint(receipt)?;
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_fixed(b"peritus-workbench-checkpoint-page-v1\0")?;
    write_digest(&mut writer, manifest)?;
    writer.write_u64(selected_revision)?;
    Ok(peritus_codec::sha256(writer.as_slice()))
}

/// Binds one rewind selection to the complete immutable manifest fingerprint.
pub fn rewind_confirmation_fingerprint(
    request: WorkbenchRewindRequest,
    checkpoint_fingerprint: Sha256Digest,
    preview_fingerprint: Sha256Digest,
) -> Result<Sha256Digest, CodecError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_fixed(b"peritus-workbench-rewind-confirmation-v1\0")?;
    write_request(&mut writer, request)?;
    write_digest(&mut writer, checkpoint_fingerprint)?;
    write_digest(&mut writer, preview_fingerprint)?;
    Ok(peritus_codec::sha256(writer.as_slice()))
}

fn large_preview_fingerprint(
    request: WorkbenchRewindRequest,
    paths: &[WorkbenchRewindPath],
    exclusions: &[String],
    external: &[String],
) -> Result<Sha256Digest, CodecError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-workbench-rewind-preview-v2\0");
    let mut identity = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_request(&mut identity, request)?;
    let paths_count = count_u64(paths.len(), identity.len())?;
    let exclusions_count = count_u64(exclusions.len(), identity.len())?;
    let effects_count = count_u64(external.len(), identity.len())?;
    identity.write_u64(paths_count)?;
    identity.write_u64(exclusions_count)?;
    identity.write_u64(effects_count)?;
    hasher.update(identity.as_slice());
    for path in paths {
        let mut fact = CanonicalWriter::new(CodecLimits::PRODUCTION);
        fact.write_u16(1)?;
        write_rewind_path(&mut fact, path)?;
        hasher.update(fact.as_slice());
    }
    for exclusion in exclusions {
        let mut fact = CanonicalWriter::new(CodecLimits::PRODUCTION);
        fact.write_u16(2)?;
        fact.write_str(exclusion)?;
        hasher.update(fact.as_slice());
    }
    for effect in external {
        let mut fact = CanonicalWriter::new(CodecLimits::PRODUCTION);
        fact.write_u16(3)?;
        fact.write_str(effect)?;
        hasher.update(fact.as_slice());
    }
    Ok(finish_fingerprint(hasher))
}

fn count_u64(value: usize, offset: usize) -> Result<u64, CodecError> {
    u64::try_from(value).map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, offset))
}

fn finish_fingerprint(hasher: Sha256) -> Sha256Digest {
    let bytes = hasher.finalize();
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(&bytes);
    Sha256Digest::new(digest)
}
