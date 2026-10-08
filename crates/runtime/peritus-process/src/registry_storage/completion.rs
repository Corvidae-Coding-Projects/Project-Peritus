//! Atomic independently-owned completion receipt publication.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    OutputStream, ProcessControl, ProcessCursor, ProcessError, RetainedCompletionBinding,
    RetainedOwnerCompletionStatus, RetainedOwnerObservation, RetainedProcessKey,
    RetainedServiceOwner, RetainedStreamPage, RetainedStreamSnapshot, TerminalResult,
    consumption::store_cause,
};

use super::{
    acquire_retained_owner_transaction, hex, read_canonical_file, store_error, sync_directory,
};

mod codec;
mod reader;

use codec::{CompletionHeader, SnapshotMetadata, completion_binding_digest};
pub(crate) use reader::{
    load_owner_completion_observation, load_owner_completion_status,
    load_owner_completion_stream_page,
};

pub(super) const EVENT_PAGE_RECORDS: usize = 256;
pub(super) const STREAM_CHUNK_BYTES: usize = 64 * 1_024;
pub(super) const STREAM_CHUNK_BYTES_U64: u64 = 64 * 1_024;

pub(crate) fn load_owner_completion_request(
    directory: &Path,
    process_id: ProcessId,
) -> Result<Vec<u8>, ProcessError> {
    let path = directory.join(format!("{}.request", hex(process_id.as_bytes())));
    read_canonical_file(&path, "retained owner completion request cannot be read")?
        .ok_or_else(|| store_error("retained owner completion request is missing"))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn persist_owner_completion(
    directory: &Path,
    retained_owners: &Path,
    binding: RetainedCompletionBinding,
    terminal: &TerminalResult,
    control: Option<&ProcessControl>,
    outstanding: &[Option<RetainedStreamSnapshot>; 3],
) -> Result<(), ProcessError> {
    let _transaction =
        acquire_retained_owner_transaction(retained_owners, binding.key().process_id())?;
    let target = receipt_path(directory, binding.key().process_id());
    let tree = control.and_then(ProcessControl::tree_identity);
    if let Some(existing) = reader::load_header_path(&target)? {
        if existing.binding == binding && existing.terminal == *terminal && existing.tree == tree {
            sync_directory(directory)?;
            return Ok(());
        }
        return Err(store_error("retained owner completion receipt conflicts"));
    }

    let staging = tempfile::Builder::new()
        .prefix(".owner-completion-staging-")
        .tempdir_in(directory)
        .map_err(|error| store_cause("completion staging directory cannot be created", error))?;
    let events_directory = staging.path().join("events");
    let streams_directory = staging.path().join("streams");
    fs::create_dir(&events_directory)
        .map_err(|error| store_cause("completion event directory cannot be created", error))?;
    fs::create_dir(&streams_directory)
        .map_err(|error| store_cause("completion stream directory cannot be created", error))?;

    let binding_digest = completion_binding_digest(binding, terminal, tree)?;
    let event_frontier = write_event_pages(
        &events_directory,
        binding,
        binding_digest,
        control,
    )?;
    let (final_streams, outstanding_streams) = write_streams(
        &streams_directory,
        binding_digest,
        control,
        outstanding,
    )?;
    let header = CompletionHeader {
        binding,
        binding_digest,
        terminal: terminal.clone(),
        tree,
        event_count: event_frontier.count,
        event_page_count: event_frontier.pages,
        first_event_sequence: event_frontier.first,
        last_event_sequence: event_frontier.last,
        final_streams,
        outstanding_streams,
    };
    write_new_file(&staging.path().join("header"), &codec::encode_header(&header)?)?;
    sync_directory(&events_directory)?;
    sync_directory(&streams_directory)?;
    sync_directory(staging.path())?;
    let staging_path = staging.path().to_path_buf();
    match fs::rename(&staging_path, &target) {
        Ok(()) => {
            sync_directory(directory)?;
            drop(staging);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = reader::load_header_path(&target)?
                .ok_or_else(|| store_error("completion receipt disappeared during publication"))?;
            if existing.binding != binding || existing.terminal != *terminal || existing.tree != tree
            {
                return Err(store_error("retained owner completion receipt conflicts"));
            }
            sync_directory(directory)
        }
        Err(error) => Err(store_cause("completion receipt cannot be published", error)),
    }
}

struct EventFrontier {
    count: u64,
    pages: u64,
    first: Option<u64>,
    last: Option<u64>,
}

fn write_event_pages(
    directory: &Path,
    binding: RetainedCompletionBinding,
    binding_digest: Sha256Digest,
    control: Option<&ProcessControl>,
) -> Result<EventFrontier, ProcessError> {
    let Some(control) = control else {
        return Ok(EventFrontier { count: 0, pages: 0, first: None, last: None });
    };
    let mut cursor = ProcessCursor::after(0);
    let mut count = 0_u64;
    let mut pages = 0_u64;
    let mut first = None;
    let mut last = None;
    loop {
        let events = control.read_events(cursor, EVENT_PAGE_RECORDS);
        if events.is_empty() { break; }
        for event in &events {
            let expected = last.and_then(|sequence: u64| sequence.checked_add(1));
            if event.process_id() != binding.key().process_id()
                || event.plan_digest() != binding.plan_digest()
                || expected.is_some_and(|expected| event.sequence() != expected)
            {
                return Err(store_error("completion event frontier is inconsistent"));
            }
            first.get_or_insert(event.sequence());
            last = Some(event.sequence());
        }
        let page = codec::encode_event_page(binding_digest, pages, &events)?;
        write_new_file(&event_path(directory, pages), &page)?;
        count = count
            .checked_add(u64::try_from(events.len()).map_err(|error| {
                store_cause("completion event count is unrepresentable", error)
            })?)
            .ok_or_else(|| store_error("completion event count overflowed"))?;
        pages = pages
            .checked_add(1)
            .ok_or_else(|| store_error("completion event page count overflowed"))?;
        cursor = ProcessCursor::after_event(
            events.last().ok_or_else(|| store_error("completion event page is empty"))?,
        );
    }
    Ok(EventFrontier { count, pages, first, last })
}

fn write_streams(
    directory: &Path,
    binding_digest: Sha256Digest,
    control: Option<&ProcessControl>,
    outstanding: &[Option<RetainedStreamSnapshot>; 3],
) -> Result<([SnapshotMetadata; 3], [Option<SnapshotMetadata>; 3]), ProcessError> {
    let mut final_streams = [empty_metadata(); 3];
    let mut outstanding_streams = [None; 3];
    for stream in [OutputStream::Stdout, OutputStream::Stderr, OutputStream::Terminal] {
        let index = stream_index(stream);
        let final_bytes = control.map_or_else(Vec::new, |control| {
            control.retained_stream_output(stream)
        });
        let final_metadata = snapshot_metadata(&final_bytes)?;
        write_stream_chunks(
            directory,
            binding_digest,
            stream,
            false,
            final_metadata,
            &final_bytes,
        )?;
        final_streams[index] = final_metadata;
        if let Some(snapshot) = outstanding[index].as_ref()
            && (snapshot.digest() != final_metadata.digest
                || u64::try_from(snapshot.bytes().len()).ok() != Some(final_metadata.total_bytes))
        {
            let metadata = snapshot_metadata(snapshot.bytes())?;
            write_stream_chunks(
                directory,
                binding_digest,
                stream,
                true,
                metadata,
                snapshot.bytes(),
            )?;
            outstanding_streams[index] = Some(metadata);
        }
    }
    Ok((final_streams, outstanding_streams))
}

fn write_stream_chunks(
    directory: &Path,
    binding_digest: Sha256Digest,
    stream: OutputStream,
    outstanding: bool,
    metadata: SnapshotMetadata,
    bytes: &[u8],
) -> Result<(), ProcessError> {
    for (index, chunk) in bytes.chunks(STREAM_CHUNK_BYTES).enumerate() {
        let index = u64::try_from(index)
            .map_err(|error| store_cause("completion stream chunk index is invalid", error))?;
        let offset = index.checked_mul(STREAM_CHUNK_BYTES_U64)
            .ok_or_else(|| store_error("completion stream chunk offset overflowed"))?;
        let encoded = codec::encode_stream_chunk(
            binding_digest,
            stream,
            outstanding,
            metadata,
            offset,
            chunk,
        )?;
        write_new_file(
            &stream_chunk_path(directory, stream, outstanding, index),
            &encoded,
        )?;
    }
    Ok(())
}

fn snapshot_metadata(bytes: &[u8]) -> Result<SnapshotMetadata, ProcessError> {
    let total_bytes = u64::try_from(bytes.len())
        .map_err(|error| store_cause("completion stream length is unrepresentable", error))?;
    let chunk_count = total_bytes
        .checked_add(STREAM_CHUNK_BYTES_U64.saturating_sub(1))
        .ok_or_else(|| store_error("completion stream chunk count overflowed"))?
        / STREAM_CHUNK_BYTES_U64;
    Ok(SnapshotMetadata {
        digest: peritus_codec::sha256(bytes),
        total_bytes,
        chunk_count,
    })
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), ProcessError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| store_cause("completion component cannot be created", error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| store_cause("completion component cannot be synchronized", error))
}

pub(super) fn receipt_path(directory: &Path, process_id: ProcessId) -> PathBuf {
    directory.join(format!("{}.completion", hex(process_id.as_bytes())))
}

pub(super) fn event_path(directory: &Path, page: u64) -> PathBuf {
    directory.join(format!("event-{page:016x}.page"))
}

pub(super) fn stream_chunk_path(
    directory: &Path,
    stream: OutputStream,
    outstanding: bool,
    chunk: u64,
) -> PathBuf {
    let stream = match stream {
        OutputStream::Stdout => "stdout",
        OutputStream::Stderr => "stderr",
        OutputStream::Terminal => "terminal",
    };
    let snapshot = if outstanding { "outstanding" } else { "final" };
    directory.join(format!("{stream}-{snapshot}-{chunk:016x}.chunk"))
}

pub(super) const fn stream_index(stream: OutputStream) -> usize {
    match stream {
        OutputStream::Stdout => 0,
        OutputStream::Stderr => 1,
        OutputStream::Terminal => 2,
    }
}

const fn empty_metadata() -> SnapshotMetadata {
    SnapshotMetadata {
        digest: Sha256Digest::new([0; Sha256Digest::LENGTH]),
        total_bytes: 0,
        chunk_count: 0,
    }
}
