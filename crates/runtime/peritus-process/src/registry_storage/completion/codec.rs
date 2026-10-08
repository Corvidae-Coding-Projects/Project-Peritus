//! Canonical small-header and bounded diagnostic-page codecs.

use peritus_types::Sha256Digest;

use crate::{
    ErrorCode, OutputStream, ProcessError, ProcessEvent, ProcessOperation, ProcessTreeIdentity,
    RecoveryClass, RetainedCompletionBinding, RetainedOwnerNonce, RetainedProcessKey,
    RetainedServiceOwner, TerminalResult,
};

const HEADER_MAGIC: &[u8] = b"PERITUS-OWNER-COMPLETION-HEADER-V1\0";
const EVENT_MAGIC: &[u8] = b"PERITUS-OWNER-COMPLETION-EVENT-PAGE-V1\0";
const STREAM_MAGIC: &[u8] = b"PERITUS-OWNER-COMPLETION-STREAM-CHUNK-V1\0";
const BINDING_DOMAIN: &[u8] = b"peritus.owner-completion.binding.v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SnapshotMetadata {
    pub(super) digest: Sha256Digest,
    pub(super) total_bytes: u64,
    pub(super) chunk_count: u64,
}

#[derive(Clone, Debug)]
pub(super) struct CompletionHeader {
    pub(super) binding: RetainedCompletionBinding,
    pub(super) binding_digest: Sha256Digest,
    pub(super) terminal: TerminalResult,
    pub(super) tree: Option<ProcessTreeIdentity>,
    pub(super) event_count: u64,
    pub(super) event_page_count: u64,
    pub(super) first_event_sequence: Option<u64>,
    pub(super) last_event_sequence: Option<u64>,
    pub(super) final_streams: [SnapshotMetadata; 3],
    pub(super) outstanding_streams: [Option<SnapshotMetadata>; 3],
}

pub(super) fn completion_binding_digest(
    binding: RetainedCompletionBinding,
    terminal: &TerminalResult,
    tree: Option<ProcessTreeIdentity>,
) -> Result<Sha256Digest, ProcessError> {
    let terminal = terminal.encode_retained_owner()?;
    let mut bytes = Vec::with_capacity(BINDING_DOMAIN.len() + terminal.len() + 256);
    bytes.extend_from_slice(BINDING_DOMAIN);
    encode_binding(&mut bytes, binding);
    encode_tree(&mut bytes, tree);
    frame(&mut bytes, &terminal)?;
    Ok(peritus_codec::sha256(&bytes))
}

pub(super) fn encode_header(header: &CompletionHeader) -> Result<Vec<u8>, ProcessError> {
    if header.binding_digest
        != completion_binding_digest(header.binding, &header.terminal, header.tree)?
        || !event_frontier_valid(header)
    {
        return Err(corrupt("completion header binding is inconsistent"));
    }
    let terminal = header.terminal.encode_retained_owner()?;
    let mut bytes = Vec::with_capacity(HEADER_MAGIC.len() + terminal.len() + 512);
    bytes.extend_from_slice(HEADER_MAGIC);
    encode_binding(&mut bytes, header.binding);
    bytes.extend_from_slice(header.binding_digest.as_bytes());
    encode_tree(&mut bytes, header.tree);
    frame(&mut bytes, &terminal)?;
    bytes.extend_from_slice(&header.event_count.to_be_bytes());
    bytes.extend_from_slice(&header.event_page_count.to_be_bytes());
    optional_u64(&mut bytes, header.first_event_sequence);
    optional_u64(&mut bytes, header.last_event_sequence);
    for index in 0..3 {
        encode_snapshot(&mut bytes, header.final_streams[index]);
        match header.outstanding_streams[index] {
            None => bytes.push(0),
            Some(snapshot) => {
                bytes.push(1);
                encode_snapshot(&mut bytes, snapshot);
            }
        }
    }
    let checksum = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    Ok(bytes)
}

pub(super) fn decode_header(bytes: &[u8]) -> Result<CompletionHeader, ProcessError> {
    if bytes.len() < HEADER_MAGIC.len() + Sha256Digest::LENGTH {
        return Err(corrupt("completion header framing is invalid"));
    }
    let checksum_at = bytes.len() - Sha256Digest::LENGTH;
    if !bytes.starts_with(HEADER_MAGIC)
        || peritus_codec::sha256(&bytes[..checksum_at]).as_bytes() != &bytes[checksum_at..]
    {
        return Err(corrupt("completion header checksum differs"));
    }
    let mut reader = Reader::new(&bytes[HEADER_MAGIC.len()..checksum_at]);
    let binding = decode_binding(&mut reader)?;
    let binding_digest = reader.digest()?;
    let tree = decode_tree(&mut reader)?;
    let terminal = TerminalResult::decode_retained_owner(reader.frame()?)?;
    let event_count = reader.u64()?;
    let event_page_count = reader.u64()?;
    let first_event_sequence = reader.optional_u64()?;
    let last_event_sequence = reader.optional_u64()?;
    let mut final_streams = [empty_snapshot(); 3];
    let mut outstanding_streams = [None; 3];
    for index in 0..3 {
        final_streams[index] = decode_snapshot(&mut reader)?;
        outstanding_streams[index] = match reader.u8()? {
            0 => None,
            1 => Some(decode_snapshot(&mut reader)?),
            _ => return Err(corrupt("completion header snapshot tag is invalid")),
        };
    }
    let header = CompletionHeader {
        binding,
        binding_digest,
        terminal,
        tree,
        event_count,
        event_page_count,
        first_event_sequence,
        last_event_sequence,
        final_streams,
        outstanding_streams,
    };
    if !reader.is_empty()
        || binding_digest != completion_binding_digest(binding, &header.terminal, tree)?
        || header.terminal.process_id() != binding.key().process_id()
        || header.terminal.plan_digest() != binding.plan_digest()
        || !event_frontier_valid(&header)
    {
        return Err(corrupt("completion header fields are inconsistent"));
    }
    Ok(header)
}

pub(super) fn encode_event_page(
    binding_digest: Sha256Digest,
    page_index: u64,
    events: &[ProcessEvent],
) -> Result<Vec<u8>, ProcessError> {
    let count = u16::try_from(events.len())
        .map_err(|_| corrupt("completion event page count is unrepresentable"))?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(EVENT_MAGIC);
    bytes.extend_from_slice(binding_digest.as_bytes());
    bytes.extend_from_slice(&page_index.to_be_bytes());
    bytes.extend_from_slice(&count.to_be_bytes());
    for event in events {
        frame(&mut bytes, &event.encode_retained_owner()?)?;
    }
    let checksum = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    Ok(bytes)
}

pub(super) fn decode_event_page(
    bytes: &[u8],
    binding_digest: Sha256Digest,
    page_index: u64,
) -> Result<Vec<ProcessEvent>, ProcessError> {
    let checksum_at = bytes.len().checked_sub(Sha256Digest::LENGTH)
        .ok_or_else(|| corrupt("completion event page is truncated"))?;
    if !bytes.starts_with(EVENT_MAGIC)
        || peritus_codec::sha256(&bytes[..checksum_at]).as_bytes() != &bytes[checksum_at..]
    {
        return Err(corrupt("completion event page checksum differs"));
    }
    let mut reader = Reader::new(&bytes[EVENT_MAGIC.len()..checksum_at]);
    if reader.digest()? != binding_digest || reader.u64()? != page_index {
        return Err(corrupt("completion event page binding differs"));
    }
    let count = usize::from(reader.u16()?);
    if count > super::EVENT_PAGE_RECORDS {
        return Err(corrupt("completion event page exceeds its physical record bound"));
    }
    let mut events = Vec::new();
    events.try_reserve_exact(count)
        .map_err(|_| corrupt("completion event page allocation failed"))?;
    for _ in 0..count {
        events.push(ProcessEvent::decode_retained_owner(reader.frame()?)?);
    }
    if !reader.is_empty() {
        return Err(corrupt("completion event page has trailing bytes"));
    }
    Ok(events)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn encode_stream_chunk(
    binding_digest: Sha256Digest,
    stream: OutputStream,
    outstanding: bool,
    snapshot: SnapshotMetadata,
    offset: u64,
    bytes_value: &[u8],
) -> Result<Vec<u8>, ProcessError> {
    let mut bytes = Vec::with_capacity(STREAM_MAGIC.len() + bytes_value.len() + 160);
    bytes.extend_from_slice(STREAM_MAGIC);
    bytes.extend_from_slice(binding_digest.as_bytes());
    bytes.push(stream_tag(stream));
    bytes.push(u8::from(outstanding));
    encode_snapshot(&mut bytes, snapshot);
    bytes.extend_from_slice(&offset.to_be_bytes());
    frame(&mut bytes, bytes_value)?;
    let checksum = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn decode_stream_chunk(
    bytes: &[u8],
    binding_digest: Sha256Digest,
    stream: OutputStream,
    outstanding: bool,
    snapshot: SnapshotMetadata,
    offset: u64,
) -> Result<Vec<u8>, ProcessError> {
    let checksum_at = bytes.len().checked_sub(Sha256Digest::LENGTH)
        .ok_or_else(|| corrupt("completion stream chunk is truncated"))?;
    if !bytes.starts_with(STREAM_MAGIC)
        || peritus_codec::sha256(&bytes[..checksum_at]).as_bytes() != &bytes[checksum_at..]
    {
        return Err(corrupt("completion stream chunk checksum differs"));
    }
    let mut reader = Reader::new(&bytes[STREAM_MAGIC.len()..checksum_at]);
    if reader.digest()? != binding_digest
        || decode_stream(reader.u8()?)? != stream
        || reader.boolean()? != outstanding
        || decode_snapshot(&mut reader)? != snapshot
        || reader.u64()? != offset
    {
        return Err(corrupt("completion stream chunk binding differs"));
    }
    let value = reader.frame()?.to_vec();
    if !reader.is_empty() {
        return Err(corrupt("completion stream chunk has trailing bytes"));
    }
    Ok(value)
}

fn event_frontier_valid(header: &CompletionHeader) -> bool {
    let page_size = u64::try_from(super::EVENT_PAGE_RECORDS).unwrap_or(u64::MAX);
    let expected_pages = header.event_count
        .checked_add(page_size.saturating_sub(1))
        .map(|count| count / page_size);
    match (header.first_event_sequence, header.last_event_sequence) {
        (None, None) => {
            header.event_count == 0
                && header.event_page_count == 0
                && expected_pages == Some(0)
        }
        (Some(first), Some(last)) => {
            first > 0
                && last >= first
                && last.checked_sub(first).and_then(|value| value.checked_add(1))
                    == Some(header.event_count)
                && expected_pages == Some(header.event_page_count)
        }
        _ => false,
    }
}

fn encode_binding(bytes: &mut Vec<u8>, binding: RetainedCompletionBinding) {
    bytes.extend_from_slice(binding.service_owner().digest().as_bytes());
    bytes.extend_from_slice(binding.key().process_id().as_bytes());
    bytes.extend_from_slice(&binding.key().nonce().as_bytes());
    bytes.extend_from_slice(binding.key().operation_digest().as_bytes());
    bytes.extend_from_slice(binding.request_digest().as_bytes());
    bytes.extend_from_slice(binding.plan_digest().as_bytes());
}

fn decode_binding(reader: &mut Reader<'_>) -> Result<RetainedCompletionBinding, ProcessError> {
    let service_owner = RetainedServiceOwner::from_digest(reader.digest()?);
    let process_id = peritus_types::ProcessId::new(reader.array()?)
        .map_err(|_| corrupt("completion header has a zero process identity"))?;
    let nonce = RetainedOwnerNonce::from_bytes(reader.array()?)?;
    let key = RetainedProcessKey::new(process_id, nonce, reader.digest()?);
    Ok(RetainedCompletionBinding::new(
        service_owner,
        key,
        reader.digest()?,
        reader.digest()?,
    ))
}

fn encode_tree(bytes: &mut Vec<u8>, tree: Option<ProcessTreeIdentity>) {
    let Some(tree) = tree else { bytes.push(0); return; };
    bytes.push(1);
    bytes.extend_from_slice(&tree.root_pid().to_be_bytes());
    optional_u64(bytes, tree.start_token());
    match tree.process_group() {
        None => bytes.push(0),
        Some(group) => { bytes.push(1); bytes.extend_from_slice(&group.to_be_bytes()); }
    }
    bytes.push(u8::from(tree.complete_containment()));
}

fn decode_tree(reader: &mut Reader<'_>) -> Result<Option<ProcessTreeIdentity>, ProcessError> {
    match reader.u8()? {
        0 => Ok(None),
        1 => {
            let root = reader.u32()?;
            if root == 0 { return Err(corrupt("completion tree has a zero root process")); }
            let start = reader.optional_u64()?;
            let group = match reader.u8()? {
                0 => None,
                1 => Some(reader.u32()?),
                _ => return Err(corrupt("completion tree group tag is invalid")),
            };
            Ok(Some(ProcessTreeIdentity::new(root, start, group, reader.boolean()?)))
        }
        _ => Err(corrupt("completion tree tag is invalid")),
    }
}

fn encode_snapshot(bytes: &mut Vec<u8>, snapshot: SnapshotMetadata) {
    bytes.extend_from_slice(snapshot.digest.as_bytes());
    bytes.extend_from_slice(&snapshot.total_bytes.to_be_bytes());
    bytes.extend_from_slice(&snapshot.chunk_count.to_be_bytes());
}

fn decode_snapshot(reader: &mut Reader<'_>) -> Result<SnapshotMetadata, ProcessError> {
    let snapshot = SnapshotMetadata {
        digest: reader.digest()?,
        total_bytes: reader.u64()?,
        chunk_count: reader.u64()?,
    };
    let expected_chunks = snapshot.total_bytes
        .checked_add(super::STREAM_CHUNK_BYTES_U64.saturating_sub(1))
        .map(|value| value / super::STREAM_CHUNK_BYTES_U64)
        .ok_or_else(|| corrupt("completion stream chunk count overflowed"))?;
    if snapshot.chunk_count != expected_chunks {
        return Err(corrupt("completion stream chunk count differs"));
    }
    Ok(snapshot)
}

const fn empty_snapshot() -> SnapshotMetadata {
    SnapshotMetadata {
        digest: Sha256Digest::new([0; Sha256Digest::LENGTH]),
        total_bytes: 0,
        chunk_count: 0,
    }
}

fn frame(bytes: &mut Vec<u8>, value: &[u8]) -> Result<(), ProcessError> {
    let length = u32::try_from(value.len())
        .map_err(|_| corrupt("completion frame length is unrepresentable"))?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(())
}

fn optional_u64(bytes: &mut Vec<u8>, value: Option<u64>) {
    match value {
        None => bytes.push(0),
        Some(value) => { bytes.push(1); bytes.extend_from_slice(&value.to_be_bytes()); }
    }
}

const fn stream_tag(stream: OutputStream) -> u8 {
    match stream { OutputStream::Stdout => 1, OutputStream::Stderr => 2, OutputStream::Terminal => 3 }
}

fn decode_stream(tag: u8) -> Result<OutputStream, ProcessError> {
    match tag {
        1 => Ok(OutputStream::Stdout),
        2 => Ok(OutputStream::Stderr),
        3 => Ok(OutputStream::Terminal),
        _ => Err(corrupt("completion stream tag is invalid")),
    }
}

struct Reader<'a> { bytes: &'a [u8], offset: usize }

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self { Self { bytes, offset: 0 } }
    fn take(&mut self, length: usize) -> Result<&'a [u8], ProcessError> {
        let end = self.offset.checked_add(length)
            .ok_or_else(|| corrupt("completion frame offset overflowed"))?;
        let value = self.bytes.get(self.offset..end)
            .ok_or_else(|| corrupt("completion frame is truncated"))?;
        self.offset = end;
        Ok(value)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProcessError> {
        self.take(N)?.try_into().map_err(|_| corrupt("completion field length differs"))
    }
    fn u8(&mut self) -> Result<u8, ProcessError> { Ok(self.array::<1>()?[0]) }
    fn u16(&mut self) -> Result<u16, ProcessError> { Ok(u16::from_be_bytes(self.array()?)) }
    fn u32(&mut self) -> Result<u32, ProcessError> { Ok(u32::from_be_bytes(self.array()?)) }
    fn u64(&mut self) -> Result<u64, ProcessError> { Ok(u64::from_be_bytes(self.array()?)) }
    fn boolean(&mut self) -> Result<bool, ProcessError> {
        match self.u8()? { 0 => Ok(false), 1 => Ok(true), _ => Err(corrupt("completion boolean is invalid")) }
    }
    fn digest(&mut self) -> Result<Sha256Digest, ProcessError> { Ok(Sha256Digest::new(self.array()?)) }
    fn optional_u64(&mut self) -> Result<Option<u64>, ProcessError> {
        match self.u8()? { 0 => Ok(None), 1 => self.u64().map(Some), _ => Err(corrupt("completion optional integer tag is invalid")) }
    }
    fn frame(&mut self) -> Result<&'a [u8], ProcessError> {
        let length = usize::try_from(self.u32()?)
            .map_err(|_| corrupt("completion frame length is unrepresentable"))?;
        self.take(length)
    }
    const fn is_empty(&self) -> bool { self.offset == self.bytes.len() }
}

const fn corrupt(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}
