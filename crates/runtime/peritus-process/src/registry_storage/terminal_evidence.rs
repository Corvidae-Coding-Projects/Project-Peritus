//! Immutable paged terminal evidence referenced by bounded recovery roots.

use std::{
    fs::{self, File},
    io::{ErrorKind, Read as _, Write as _},
    path::{Path, PathBuf},
};

use peritus_types::{ProcessId, Sha256Digest};
use sha2::{Digest as _, Sha256};

use crate::{
    ErrorCode, ProcessError, ProcessOperation, RecoveryClass, TerminalResult,
    consumption::{store_cause, store_error},
    terminal::decode_terminal,
};

use super::{hex, sync_directory};

const MAGIC: &[u8] = b"PERITUS-PROCESS-TERMINAL-EVIDENCE-V1\0";
const PAGE_PAYLOAD_BYTES: usize = 64 * 1_024;
const FIXED_PAGE_BYTES: usize = 16 + Sha256Digest::LENGTH + 8 + 8 + 8 + 4;
const MAX_PAGE_BYTES: usize = MAGIC.len() + FIXED_PAGE_BYTES + PAGE_PAYLOAD_BYTES
    + Sha256Digest::LENGTH;

#[derive(Clone, Copy)]
pub(crate) struct TerminalEvidenceRoot {
    pub(crate) total_bytes: u64,
    pub(crate) page_count: u64,
}

pub(crate) fn persist_terminal_evidence(
    root: &Path,
    process_id: ProcessId,
    terminal_digest: Sha256Digest,
    terminal_bytes: &[u8],
) -> Result<TerminalEvidenceRoot, ProcessError> {
    if peritus_codec::sha256(terminal_bytes) != terminal_digest || terminal_bytes.is_empty() {
        return Err(corrupt("terminal evidence differs from its recovery digest"));
    }
    ensure_directory(root, root.parent())?;
    let process_directory = root.join(hex(process_id.as_bytes()));
    ensure_directory(&process_directory, Some(root))?;
    let digest_directory = process_directory.join(hex(terminal_digest.as_bytes()));
    ensure_directory(&digest_directory, Some(root))?;

    let total_bytes = u64::try_from(terminal_bytes.len())
        .map_err(|_| unavailable("terminal evidence length is not representable"))?;
    let page_count = page_count(total_bytes)?;
    for (ordinal, payload) in terminal_bytes.chunks(PAGE_PAYLOAD_BYTES).enumerate() {
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| unavailable("terminal evidence page ordinal is not representable"))?;
        let encoded = encode_page(
            process_id,
            terminal_digest,
            total_bytes,
            page_count,
            ordinal,
            payload,
        )?;
        publish_page(&digest_directory, ordinal, &encoded)?;
    }
    sync_directory(&digest_directory)?;
    Ok(TerminalEvidenceRoot { total_bytes, page_count })
}

pub(crate) fn load_terminal_evidence(
    root: &Path,
    process_id: ProcessId,
    terminal_digest: Sha256Digest,
    expected: TerminalEvidenceRoot,
) -> Result<TerminalResult, ProcessError> {
    if expected.total_bytes == 0
        || expected.page_count == 0
        || page_count(expected.total_bytes)? != expected.page_count
    {
        return Err(corrupt("terminal evidence root has an invalid page frontier"));
    }
    let process_directory = root.join(hex(process_id.as_bytes()));
    let digest_directory = process_directory.join(hex(terminal_digest.as_bytes()));
    validate_directory(root, &process_directory)?;
    validate_directory(root, &digest_directory)?;
    let mut terminal_bytes = Vec::new();
    for ordinal in 0..expected.page_count {
        let encoded = read_page(&page_path(&digest_directory, ordinal))?;
        let payload = decode_page(
            &encoded,
            process_id,
            terminal_digest,
            expected,
            ordinal,
        )?;
        terminal_bytes
            .try_reserve(payload.len())
            .map_err(|_| unavailable("terminal evidence allocation is unavailable"))?;
        terminal_bytes.extend_from_slice(payload);
    }
    if u64::try_from(terminal_bytes.len()).ok() != Some(expected.total_bytes)
        || peritus_codec::sha256(&terminal_bytes) != terminal_digest
    {
        return Err(corrupt("terminal evidence frontier or digest differs"));
    }
    let terminal = decode_terminal(&terminal_bytes)?;
    if terminal.process_id() != process_id {
        return Err(corrupt("terminal evidence identity differs from its recovery root"));
    }
    Ok(terminal)
}

fn encode_page(
    process_id: ProcessId,
    terminal_digest: Sha256Digest,
    total_bytes: u64,
    page_count: u64,
    ordinal: u64,
    payload: &[u8],
) -> Result<Vec<u8>, ProcessError> {
    if payload.is_empty() || payload.len() > PAGE_PAYLOAD_BYTES {
        return Err(corrupt("terminal evidence page payload is invalid"));
    }
    let payload_length = u32::try_from(payload.len())
        .map_err(|_| unavailable("terminal evidence page length is not representable"))?;
    let mut bytes = Vec::with_capacity(MAGIC.len() + FIXED_PAGE_BYTES + payload.len() + 32);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(process_id.as_bytes());
    bytes.extend_from_slice(terminal_digest.as_bytes());
    bytes.extend_from_slice(&total_bytes.to_be_bytes());
    bytes.extend_from_slice(&page_count.to_be_bytes());
    bytes.extend_from_slice(&ordinal.to_be_bytes());
    bytes.extend_from_slice(&payload_length.to_be_bytes());
    bytes.extend_from_slice(payload);
    let checksum: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&checksum);
    Ok(bytes)
}

fn decode_page<'a>(
    bytes: &'a [u8],
    process_id: ProcessId,
    terminal_digest: Sha256Digest,
    root: TerminalEvidenceRoot,
    ordinal: u64,
) -> Result<&'a [u8], ProcessError> {
    let payload_end = bytes
        .len()
        .checked_sub(Sha256Digest::LENGTH)
        .ok_or_else(|| corrupt("terminal evidence page is truncated"))?;
    let expected_checksum: [u8; 32] = Sha256::digest(&bytes[..payload_end]).into();
    if !bytes.starts_with(MAGIC) || bytes[payload_end..] != expected_checksum {
        return Err(corrupt("terminal evidence page framing or checksum differs"));
    }
    let mut offset = MAGIC.len();
    let mut take = |length: usize| -> Result<&[u8], ProcessError> {
        let end = offset
            .checked_add(length)
            .ok_or_else(|| corrupt("terminal evidence page offset overflowed"))?;
        let value = bytes
            .get(offset..end)
            .ok_or_else(|| corrupt("terminal evidence page is truncated"))?;
        offset = end;
        Ok(value)
    };
    let stored_process = ProcessId::new(
        take(16)?
            .try_into()
            .map_err(|_| corrupt("terminal evidence process identity is invalid"))?,
    )
    .map_err(|_| corrupt("terminal evidence process identity is zero"))?;
    let stored_digest = Sha256Digest::new(
        take(Sha256Digest::LENGTH)?
            .try_into()
            .map_err(|_| corrupt("terminal evidence digest is invalid"))?,
    );
    let stored_total = u64::from_be_bytes(
        take(8)?.try_into().map_err(|_| corrupt("terminal evidence total is invalid"))?,
    );
    let stored_pages = u64::from_be_bytes(
        take(8)?.try_into().map_err(|_| corrupt("terminal evidence page count is invalid"))?,
    );
    let stored_ordinal = u64::from_be_bytes(
        take(8)?.try_into().map_err(|_| corrupt("terminal evidence ordinal is invalid"))?,
    );
    let payload_length = usize::try_from(u32::from_be_bytes(
        take(4)?.try_into().map_err(|_| corrupt("terminal evidence payload length is invalid"))?,
    ))
    .map_err(|_| unavailable("terminal evidence payload length is not addressable"))?;
    let expected_length = expected_payload_length(root, ordinal)?;
    if stored_process != process_id
        || stored_digest != terminal_digest
        || stored_total != root.total_bytes
        || stored_pages != root.page_count
        || stored_ordinal != ordinal
        || payload_length != expected_length
    {
        return Err(corrupt("terminal evidence page binding differs from its recovery root"));
    }
    let payload = take(payload_length)?;
    if offset != payload_end {
        return Err(corrupt("terminal evidence page has trailing bytes"));
    }
    Ok(payload)
}

fn page_count(total_bytes: u64) -> Result<u64, ProcessError> {
    let page_bytes = u64::try_from(PAGE_PAYLOAD_BYTES)
        .map_err(|_| unavailable("terminal evidence page size is not representable"))?;
    total_bytes
        .checked_add(page_bytes - 1)
        .ok_or_else(|| unavailable("terminal evidence page count overflowed"))
        .map(|rounded| rounded / page_bytes)
}

fn expected_payload_length(
    root: TerminalEvidenceRoot,
    ordinal: u64,
) -> Result<usize, ProcessError> {
    if ordinal >= root.page_count {
        return Err(corrupt("terminal evidence page ordinal exceeds its frontier"));
    }
    let page_bytes = u64::try_from(PAGE_PAYLOAD_BYTES)
        .map_err(|_| unavailable("terminal evidence page size is not representable"))?;
    let offset = ordinal
        .checked_mul(page_bytes)
        .ok_or_else(|| corrupt("terminal evidence page offset overflowed"))?;
    let remaining = root
        .total_bytes
        .checked_sub(offset)
        .ok_or_else(|| corrupt("terminal evidence page starts after its terminal record"))?;
    usize::try_from(remaining.min(page_bytes))
        .map_err(|_| unavailable("terminal evidence page length is not addressable"))
}

fn publish_page(directory: &Path, ordinal: u64, bytes: &[u8]) -> Result<(), ProcessError> {
    let target = page_path(directory, ordinal);
    let mut staging = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| store_cause("terminal evidence page cannot be staged", error))?;
    staging
        .write_all(bytes)
        .and_then(|()| staging.as_file().sync_all())
        .map_err(|error| store_cause("terminal evidence page cannot be synchronized", error))?;
    match staging.persist_noclobber(&target) {
        Ok(_) => sync_directory(directory),
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {
            if read_page(&target)?.as_slice() != bytes {
                return Err(corrupt("terminal evidence page conflicts with its immutable identity"));
            }
            File::open(&target)
                .and_then(|file| file.sync_all())
                .map_err(|error| store_cause("terminal evidence page cannot be resynchronized", error))?;
            sync_directory(directory)
        }
        Err(error) => Err(store_cause("terminal evidence page cannot be published", error.error)),
    }
}

fn read_page(path: &Path) -> Result<Vec<u8>, ProcessError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|_| corrupt("terminal evidence page is missing"))?;
    if !path_metadata.file_type().is_file() || path_metadata.file_type().is_symlink() {
        return Err(corrupt("terminal evidence page is not a regular file"));
    }
    let mut file = File::open(path)
        .map_err(|error| store_cause("terminal evidence page cannot be opened", error))?;
    let metadata = file
        .metadata()
        .map_err(|error| store_cause("terminal evidence page metadata cannot be read", error))?;
    if !metadata.file_type().is_file()
        || metadata.len() > u64::try_from(MAX_PAGE_BYTES).unwrap_or(u64::MAX)
    {
        return Err(corrupt("terminal evidence page exceeds its physical bound"));
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len()).unwrap_or(MAX_PAGE_BYTES).min(MAX_PAGE_BYTES),
    );
    file.take(u64::try_from(MAX_PAGE_BYTES).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| store_cause("terminal evidence page cannot be read", error))?;
    if bytes.len() > MAX_PAGE_BYTES || u64::try_from(bytes.len()).ok() != Some(metadata.len()) {
        return Err(corrupt("terminal evidence page changed while it was read"));
    }
    Ok(bytes)
}

fn ensure_directory(directory: &Path, protected_root: Option<&Path>) -> Result<(), ProcessError> {
    match fs::create_dir(directory) {
        Ok(()) => {
            if let Some(parent) = directory.parent() {
                sync_directory(parent)?;
            }
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(store_cause("terminal evidence directory cannot be created", error));
        }
    }
    if let Some(root) = protected_root {
        validate_directory(root, directory)?;
    } else {
        let metadata = fs::symlink_metadata(directory)
            .map_err(|error| store_cause("terminal evidence root cannot be inspected", error))?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(store_error("terminal evidence root is not a real directory"));
        }
    }
    Ok(())
}

fn validate_directory(root: &Path, directory: &Path) -> Result<(), ProcessError> {
    let metadata = fs::symlink_metadata(directory)
        .map_err(|_| corrupt("terminal evidence directory is missing"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(corrupt("terminal evidence path is not a real directory"));
    }
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| store_cause("terminal evidence root cannot be canonicalized", error))?;
    let canonical = fs::canonicalize(directory)
        .map_err(|error| store_cause("terminal evidence directory cannot be canonicalized", error))?;
    if !canonical.starts_with(canonical_root) {
        return Err(corrupt("terminal evidence directory escaped its protected root"));
    }
    Ok(())
}

fn page_path(directory: &Path, ordinal: u64) -> PathBuf {
    directory.join(format!("{ordinal:016x}.page"))
}

const fn corrupt(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}

const fn unavailable(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Indeterminate,
        ProcessOperation::Reconcile,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}
