//! Bounded late readers for atomically published completion receipts.

use std::{fs, path::Path};

use peritus_types::Sha256Digest;

use crate::{
    ErrorCode, OutputStream, ProcessCursor, ProcessError, ProcessOperation, RecoveryClass,
    RetainedOwnerCompletionStatus, RetainedOwnerObservation, RetainedProcessKey,
    RetainedServiceOwner, RetainedStreamPage,
};

use super::{
    EVENT_PAGE_RECORDS, STREAM_CHUNK_BYTES, STREAM_CHUNK_BYTES_U64, codec,
    event_path, receipt_path, stream_chunk_path, stream_index,
};
use super::codec::{CompletionHeader, SnapshotMetadata};
use crate::registry_storage::{read_canonical_file, store_error};

pub(crate) fn load_owner_completion_status(
    directory: &Path,
    key: RetainedProcessKey,
    service_owner: RetainedServiceOwner,
    request_digest: Option<Sha256Digest>,
) -> Result<Option<RetainedOwnerCompletionStatus>, ProcessError> {
    let Some(header) = load_header_exact(directory, key, service_owner, request_digest)? else {
        return Ok(None);
    };
    Ok(Some(RetainedOwnerCompletionStatus::new(
        header.binding,
        header.terminal,
        header.tree,
    )))
}

pub(crate) fn load_owner_completion_observation(
    directory: &Path,
    key: RetainedProcessKey,
    service_owner: RetainedServiceOwner,
    cursor: ProcessCursor,
    max_events: usize,
) -> Result<Option<RetainedOwnerObservation>, ProcessError> {
    let Some(header) = load_header_exact(directory, key, service_owner, None)? else {
        return Ok(None);
    };
    let root = receipt_path(directory, key.process_id());
    let events = read_event_selection(
        &root.join("events"),
        &header,
        cursor,
        max_events.min(EVENT_PAGE_RECORDS),
    )?;
    Ok(Some(RetainedOwnerObservation::new(
        events,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Some(header.terminal),
        true,
        header.tree,
        None,
    )))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_owner_completion_stream_page(
    directory: &Path,
    key: RetainedProcessKey,
    service_owner: RetainedServiceOwner,
    stream: OutputStream,
    snapshot_digest: Option<Sha256Digest>,
    offset: u64,
    max_bytes: usize,
) -> Result<Option<RetainedStreamPage>, ProcessError> {
    let Some(header) = load_header_exact(directory, key, service_owner, None)? else {
        return Ok(None);
    };
    if max_bytes == 0 {
        return Err(invalid("retained completion stream page size is zero"));
    }
    let index = stream_index(stream);
    let (metadata, outstanding) = select_snapshot(
        header.final_streams[index],
        header.outstanding_streams[index],
        snapshot_digest,
    )?;
    if offset > metadata.total_bytes {
        return Err(invalid("retained completion stream offset exceeds its snapshot"));
    }
    let requested = max_bytes.min(STREAM_CHUNK_BYTES);
    let end = offset
        .checked_add(u64::try_from(requested).map_err(|_| {
            invalid("retained completion stream page size is unrepresentable")
        })?)
        .unwrap_or(metadata.total_bytes)
        .min(metadata.total_bytes);
    let streams = receipt_path(directory, key.process_id()).join("streams");
    let mut bytes = Vec::with_capacity(requested);
    let mut current = offset;
    while current < end {
        let chunk_index = current / STREAM_CHUNK_BYTES_U64;
        if chunk_index >= metadata.chunk_count {
            return Err(corrupt("retained completion stream chunk is missing"));
        }
        let chunk_offset = chunk_index
            .checked_mul(STREAM_CHUNK_BYTES_U64)
            .ok_or_else(|| corrupt("retained completion stream chunk offset overflowed"))?;
        let encoded = read_component(
            &stream_chunk_path(&streams, stream, outstanding, chunk_index),
            "retained completion stream chunk cannot be read",
        )?;
        let chunk = codec::decode_stream_chunk(
            &encoded,
            header.binding_digest,
            stream,
            outstanding,
            metadata,
            chunk_offset,
        )?;
        let expected_length = usize::try_from(
            metadata.total_bytes.saturating_sub(chunk_offset).min(STREAM_CHUNK_BYTES_U64),
        )
        .map_err(|_| corrupt("retained completion stream chunk length is invalid"))?;
        if chunk.len() != expected_length {
            return Err(corrupt("retained completion stream chunk length differs"));
        }
        let within = usize::try_from(current - chunk_offset)
            .map_err(|_| corrupt("retained completion stream offset is invalid"))?;
        let available = usize::try_from(end - current)
            .map_err(|_| corrupt("retained completion stream range is invalid"))?;
        let take = available.min(chunk.len().saturating_sub(within));
        if take == 0 {
            return Err(corrupt("retained completion stream chunk made no progress"));
        }
        bytes.extend_from_slice(&chunk[within..within + take]);
        current = current
            .checked_add(u64::try_from(take).map_err(|_| {
                corrupt("retained completion stream progress is invalid")
            })?)
            .ok_or_else(|| corrupt("retained completion stream offset overflowed"))?;
    }
    RetainedStreamPage::new(
        stream,
        metadata.digest,
        metadata.total_bytes,
        offset,
        bytes,
    )
    .map(Some)
}

pub(super) fn load_header_path(path: &Path) -> Result<Option<CompletionHeader>, ProcessError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(crate::consumption::store_cause(
            "completion receipt cannot be inspected",
            error,
        )),
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(store_error("completion receipt is not a real directory"));
    }
    let header = read_component(
        &path.join("header"),
        "completion receipt header cannot be read",
    )?;
    codec::decode_header(&header).map(Some)
}

fn load_header_exact(
    directory: &Path,
    key: RetainedProcessKey,
    service_owner: RetainedServiceOwner,
    request_digest: Option<Sha256Digest>,
) -> Result<Option<CompletionHeader>, ProcessError> {
    let Some(header) = load_header_path(&receipt_path(directory, key.process_id()))? else {
        return Ok(None);
    };
    if header.binding.key() != key
        || header.binding.service_owner() != service_owner
        || request_digest.is_some_and(|digest| digest != header.binding.request_digest())
    {
        return Err(mismatch("retained completion receipt identity differs"));
    }
    Ok(Some(header))
}

fn read_event_selection(
    directory: &Path,
    header: &CompletionHeader,
    cursor: ProcessCursor,
    max_events: usize,
) -> Result<Vec<crate::ProcessEvent>, ProcessError> {
    let Some(first_sequence) = header.first_event_sequence else { return Ok(Vec::new()); };
    let last_sequence = header.last_event_sequence
        .ok_or_else(|| corrupt("completion event frontier is incomplete"))?;
    if max_events == 0 || cursor.sequence() >= last_sequence {
        return Ok(Vec::new());
    }
    let start_sequence = cursor.sequence().saturating_add(1).max(first_sequence);
    let start_ordinal = start_sequence
        .checked_sub(first_sequence)
        .ok_or_else(|| corrupt("completion event selection underflowed"))?;
    let mut page_index = start_ordinal / u64::try_from(EVENT_PAGE_RECORDS)
        .map_err(|_| corrupt("completion event page size is invalid"))?;
    let mut within_page = usize::try_from(start_ordinal % u64::try_from(EVENT_PAGE_RECORDS)
        .map_err(|_| corrupt("completion event page size is invalid"))?)
        .map_err(|_| corrupt("completion event page offset is invalid"))?;
    let mut selected = Vec::with_capacity(max_events.min(EVENT_PAGE_RECORDS));
    while selected.len() < max_events && page_index < header.event_page_count {
        let encoded = read_component(
            &event_path(directory, page_index),
            "completion event page cannot be read",
        )?;
        let events = codec::decode_event_page(&encoded, header.binding_digest, page_index)?;
        validate_event_page(header, page_index, &events)?;
        for event in events.into_iter().skip(within_page) {
            if selected.len() == max_events { break; }
            selected.push(event);
        }
        within_page = 0;
        page_index = page_index
            .checked_add(1)
            .ok_or_else(|| corrupt("completion event page index overflowed"))?;
    }
    if let Some(first) = selected.first_mut() {
        first.select_for_retained_cursor(cursor);
    }
    Ok(selected)
}

fn validate_event_page(
    header: &CompletionHeader,
    page_index: u64,
    events: &[crate::ProcessEvent],
) -> Result<(), ProcessError> {
    let page_start = page_index
        .checked_mul(u64::try_from(EVENT_PAGE_RECORDS)
            .map_err(|_| corrupt("completion event page size is invalid"))?)
        .ok_or_else(|| corrupt("completion event page start overflowed"))?;
    let remaining = header.event_count
        .checked_sub(page_start)
        .ok_or_else(|| corrupt("completion event page exceeds its frontier"))?;
    let expected_count = usize::try_from(remaining.min(
        u64::try_from(EVENT_PAGE_RECORDS)
            .map_err(|_| corrupt("completion event page size is invalid"))?,
    ))
    .map_err(|_| corrupt("completion event count is invalid"))?;
    if events.len() != expected_count {
        return Err(corrupt("completion event page count differs"));
    }
    let first = header.first_event_sequence
        .ok_or_else(|| corrupt("completion event first sequence is missing"))?;
    for (index, event) in events.iter().enumerate() {
        let expected = first
            .checked_add(page_start)
            .and_then(|value| value.checked_add(u64::try_from(index).ok()?))
            .ok_or_else(|| corrupt("completion event sequence overflowed"))?;
        if event.process_id() != header.binding.key().process_id()
            || event.plan_digest() != header.binding.plan_digest()
            || event.sequence() != expected
        {
            return Err(corrupt("completion event page binding differs"));
        }
    }
    Ok(())
}

fn select_snapshot(
    final_snapshot: SnapshotMetadata,
    outstanding_snapshot: Option<SnapshotMetadata>,
    requested: Option<Sha256Digest>,
) -> Result<(SnapshotMetadata, bool), ProcessError> {
    match requested {
        None => Ok((final_snapshot, false)),
        Some(digest) => match outstanding_snapshot {
            Some(snapshot) if snapshot.digest == digest => Ok((snapshot, true)),
            _ if final_snapshot.digest == digest => Ok((final_snapshot, false)),
            _ => Err(ProcessError::new(
                ErrorCode::Indeterminate,
                ProcessOperation::Reconcile,
                RecoveryClass::ReopenAndReconcile,
                "retained completion stream snapshot changed",
            )),
        },
    }
}

fn read_component(path: &Path, detail: &'static str) -> Result<Vec<u8>, ProcessError> {
    read_canonical_file(path, detail)?.ok_or_else(|| store_error(detail))
}

const fn mismatch(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::AuthorizationMismatch,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}

const fn invalid(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::InvalidInput,
        ProcessOperation::Stream,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

const fn corrupt(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}
