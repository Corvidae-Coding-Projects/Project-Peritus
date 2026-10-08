//! Paged working snapshots with bounded physical model, protocol, and environment frames.

use peritus_codec::{CanonicalReader, CanonicalWriter, sha256};
use peritus_types::Sha256Digest;
use std::collections::BTreeSet;

use super::{WorkingCodecError, count, entry, fields, reader, writer};
use super::super::{
    ObservationId, ObservationSource, WorkingBinding, WorkingEntry, WorkingError, WorkingLimits,
    WorkingEnvironment, WorkingFileDigest, WorkingPendingOperation, WorkingProtocol, WorkingState,
};

const ENTRY_PAGE_ITEMS: usize = 64;
const PROTOCOL_PAGE_ITEMS: usize = 1_024;
const ENVIRONMENT_PAGE_ITEMS: usize = 1_024;
// One predecessor plus these data pages fits the host's 256-child bundle fanout.
const DESCRIPTOR_PAGE_ITEMS: usize = 255;

/// Immutable external artifact referenced by a paged working-state root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingStateArtifact {
    digest: [u8; 32],
    bytes: u64,
}

impl WorkingStateArtifact {
    /// Creates a nonempty verified artifact reference.
    ///
    /// # Errors
    /// Rejects an empty physical artifact.
    pub const fn new(digest: [u8; 32], bytes: u64) -> Result<Self, WorkingCodecError> {
        if bytes == 0 {
            Err(WorkingCodecError::InvalidValue)
        } else {
            Ok(Self { digest, bytes })
        }
    }

    /// Exact SHA-256 digest expected from the artifact store.
    #[must_use]
    pub const fn digest(self) -> [u8; 32] { self.digest }

    /// Exact physical byte length expected from the artifact store.
    #[must_use]
    pub const fn bytes(self) -> u64 { self.bytes }
}

/// Failure while following a paged root through caller-owned artifact storage.
#[derive(Debug)]
pub enum WorkingStateReadError<E> {
    /// Canonical bytes or reconstructed state violated the paged-state contract.
    Codec(WorkingCodecError),
    /// The caller's artifact reader failed, retaining its typed cancellation/storage error.
    Artifact(E),
}

impl<E> From<WorkingCodecError> for WorkingStateReadError<E> {
    fn from(error: WorkingCodecError) -> Self { Self::Codec(error) }
}

/// Failure while publishing a paged snapshot through caller-owned storage.
#[derive(Debug)]
pub enum WorkingStateWriteError<E> {
    /// The source state or generated canonical page violated the paged-state contract.
    Codec(WorkingCodecError),
    /// The caller's page sink failed, retaining its typed storage error.
    Artifact(E),
}

impl<E> From<WorkingCodecError> for WorkingStateWriteError<E> {
    fn from(error: WorkingCodecError) -> Self { Self::Codec(error) }
}

impl<E> From<peritus_codec::CodecError> for WorkingStateWriteError<E> {
    fn from(error: peritus_codec::CodecError) -> Self {
        Self::Codec(WorkingCodecError::Codec(error))
    }
}

impl<E> From<WorkingError> for WorkingStateWriteError<E> {
    fn from(error: WorkingError) -> Self { Self::Codec(WorkingCodecError::State(error)) }
}

/// Semantic contents carried by one bounded physical working-state page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkingStatePageKind {
    /// Canonically ordered current entries and retired entries pinned by their dependency closure.
    Entries,
    /// Canonically ordered governing instruction observation identifiers.
    Requirements,
    /// Canonically ordered unresolved operation dispositions.
    PendingOperations,
    /// Canonically ordered current file or entity digests.
    EnvironmentFiles,
    /// Canonically ordered immutable retired-entry history available by exact identifier.
    RetiredEntries,
}

impl WorkingStatePageKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Entries => 0,
            Self::Requirements => 1,
            Self::PendingOperations => 2,
            Self::EnvironmentFiles => 3,
            Self::RetiredEntries => 4,
        }
    }

    const fn magic(self) -> [u8; 4] {
        match self {
            Self::Entries => *b"PWPE",
            Self::Requirements => *b"PWPQ",
            Self::PendingOperations => *b"PWPO",
            Self::EnvironmentFiles => *b"PWPF",
            Self::RetiredEntries => *b"PWPH",
        }
    }

    const fn from_tag(tag: u8) -> Result<Self, WorkingCodecError> {
        match tag {
            0 => Ok(Self::Entries),
            1 => Ok(Self::Requirements),
            2 => Ok(Self::PendingOperations),
            3 => Ok(Self::EnvironmentFiles),
            4 => Ok(Self::RetiredEntries),
            _ => Err(WorkingCodecError::InvalidValue),
        }
    }
}

/// One encoded physical page and the exact descriptor committed by its root.
#[derive(Debug, Eq, PartialEq)]
pub struct EncodedWorkingStatePage {
    kind: WorkingStatePageKind,
    first: u64,
    count: u64,
    artifact: WorkingStateArtifact,
    bytes: Vec<u8>,
}

impl EncodedWorkingStatePage {
    /// Page semantic kind.
    #[must_use]
    pub const fn kind(&self) -> WorkingStatePageKind { self.kind }

    /// Zero-based first logical item index.
    #[must_use]
    pub const fn first(&self) -> u64 { self.first }

    /// Number of logical items in this physical page.
    #[must_use]
    pub const fn count(&self) -> u64 { self.count }

    /// Digest and byte length committed in the paged root.
    #[must_use]
    pub const fn artifact(&self) -> WorkingStateArtifact { self.artifact }

    /// Exact canonical physical page bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] { &self.bytes }
}

/// One bounded page in the authenticated linked descriptor index.
#[derive(Debug, Eq, PartialEq)]
pub struct EncodedWorkingStateDescriptorPage {
    first: u64,
    count: u64,
    artifact: WorkingStateArtifact,
    bytes: Vec<u8>,
}

impl EncodedWorkingStateDescriptorPage {
    /// Zero-based ordinal of the first data-page descriptor in this index page.
    #[must_use]
    pub const fn first(&self) -> u64 { self.first }

    /// Number of data-page descriptors in this bounded index page.
    #[must_use]
    pub const fn count(&self) -> u64 { self.count }

    /// Exact digest and byte length committed by the successor page or root tail.
    #[must_use]
    pub const fn artifact(&self) -> WorkingStateArtifact { self.artifact }

    /// Exact canonical descriptor-page bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] { &self.bytes }
}

/// One transient canonical part emitted while publishing a paged snapshot.
#[derive(Clone, Copy, Debug)]
pub enum EncodedWorkingStatePart<'a> {
    /// A bounded entry, protocol, or environment data page.
    Data(&'a EncodedWorkingStatePage),
    /// A previously verified immutable history page reused without re-encoding its entries.
    Reused(WorkingStatePageReference),
    /// A bounded linked descriptor page emitted after all of its data children.
    Descriptor(&'a EncodedWorkingStateDescriptorPage),
}

/// Exact stored-page reference emitted when immutable working history can be reused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingStatePageReference {
    kind: WorkingStatePageKind,
    first: u64,
    count: u64,
    artifact: WorkingStateArtifact,
}

impl WorkingStatePageReference {
    /// Page semantic kind.
    #[must_use]
    pub const fn kind(self) -> WorkingStatePageKind { self.kind }

    /// Zero-based first logical item index.
    #[must_use]
    pub const fn first(self) -> u64 { self.first }

    /// Number of logical items in this physical page.
    #[must_use]
    pub const fn count(self) -> u64 { self.count }

    /// Exact digest and byte length already verified for this page.
    #[must_use]
    pub const fn artifact(self) -> WorkingStateArtifact { self.artifact }
}

/// Verified immutable retired-entry pages available to a successor PWP2 snapshot.
#[derive(Debug, Eq, PartialEq)]
pub struct ReusableWorkingStateHistory {
    retired_entries: Vec<WorkingEntry>,
    retired_pages: Vec<PageDescriptor>,
}

/// Canonical root plus its independently storable bounded physical pages.
#[derive(Debug, Eq, PartialEq)]
pub struct EncodedWorkingStateSnapshot {
    observation_index: WorkingStateArtifact,
    root: Vec<u8>,
    descriptor_pages: Vec<EncodedWorkingStateDescriptorPage>,
    pages: Vec<EncodedWorkingStatePage>,
}

impl EncodedWorkingStateSnapshot {
    /// Verified external paged observation-index root referenced by this snapshot.
    #[must_use]
    pub const fn observation_index(&self) -> WorkingStateArtifact { self.observation_index }

    /// Canonical fixed-size root referencing the observation index and descriptor-chain tail.
    #[must_use]
    pub fn root(&self) -> &[u8] { &self.root }

    /// Bounded linked index pages, ordered from the chain origin through the root-owned tail.
    #[must_use]
    pub fn descriptor_pages(&self) -> &[EncodedWorkingStateDescriptorPage] {
        &self.descriptor_pages
    }

    /// Physical entry, protocol, and environment pages in the exact root-declared order.
    #[must_use]
    pub fn pages(&self) -> &[EncodedWorkingStatePage] { &self.pages }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PageDescriptor {
    kind: WorkingStatePageKind,
    first: u64,
    count: u64,
    artifact: WorkingStateArtifact,
}

struct DecodedWorkingStateRoot {
    state: WorkingState,
    counts: [usize; 5],
    descriptor_count: usize,
    descriptor_page_count: usize,
    descriptor_tail: Option<WorkingStateArtifact>,
    partitioned_entries: bool,
}

struct DecodedDescriptorPage {
    previous: Option<WorkingStateArtifact>,
    descriptors: Vec<PageDescriptor>,
}

struct StreamingPagePublisher<F> {
    publish: F,
    descriptors: Vec<PageDescriptor>,
    published: Vec<PageDescriptor>,
    descriptor_tail: Option<WorkingStateArtifact>,
    page_count: usize,
    descriptor_page_count: usize,
}

/// Encodes a paged snapshot whose root binds the independently paged observation index.
///
/// No physical frame contains the cumulative entry or protocol history. Every page stays under
/// the working codec frame bound and can be committed independently before publishing the root.
///
/// # Errors
/// Rejects a malformed state, empty observation-index reference, or one physical item that cannot
/// fit the canonical page envelope.
pub fn encode_paged_working_state(
    state: &WorkingState,
    observation_index: WorkingStateArtifact,
) -> Result<EncodedWorkingStateSnapshot, WorkingCodecError> {
    super::state::validate_snapshot(state)?;
    let (active_entries, retired_entries) = partition_entries(&state.entries);
    let mut pages = Vec::new();
    encode_entry_pages(WorkingStatePageKind::Entries, &active_entries, &mut pages)?;
    encode_requirement_pages(state, &mut pages)?;
    encode_pending_pages(state, &mut pages)?;
    encode_environment_pages(state, &mut pages)?;
    encode_entry_pages(WorkingStatePageKind::RetiredEntries, &retired_entries, &mut pages)?;
    let descriptor_pages = encode_descriptor_pages(&pages)?;

    let mut root = writer(*b"PWP2")?;
    super::state::write_limits(&mut root, state.limits())?;
    fields::write_binding(&mut root, state.binding())?;
    root.write_fixed(state.environment().candidate().as_bytes())?;
    root.write_u64(state.revision())?;
    root.write_u64(state.through_observation())?;
    write_artifact(&mut root, observation_index)?;
    root.write_u64(usize_to_u64(active_entries.len())?)?;
    root.write_u64(usize_to_u64(state.protocol.requirements().len())?)?;
    root.write_u64(usize_to_u64(state.protocol.pending().len())?)?;
    root.write_u64(usize_to_u64(state.environment().files().len())?)?;
    root.write_u64(usize_to_u64(retired_entries.len())?)?;
    root.write_u64(usize_to_u64(pages.len())?)?;
    root.write_u64(usize_to_u64(descriptor_pages.len())?)?;
    root.write_option_tag(!descriptor_pages.is_empty())?;
    if let Some(tail) = descriptor_pages.last() {
        write_artifact(&mut root, tail.artifact)?;
    }
    Ok(EncodedWorkingStateSnapshot {
        observation_index,
        root: root.into_bytes(),
        descriptor_pages,
        pages,
    })
}

/// Encodes and publishes each bounded physical page before returning the fixed root bytes.
///
/// The sink sees data pages in canonical order and then the descriptor page that owns each
/// bounded group. Page bytes are borrowed only for the duration of the sink call, so successful
/// callers need retain only their storage references rather than a second encoded snapshot.
///
/// # Errors
/// Rejects a malformed state, an invalid source-index reference, an unencodable item, or a sink
/// failure. No root is returned until every referenced page has been accepted by the sink.
#[allow(clippy::too_many_lines, reason = "the five canonical page kinds have distinct codecs")]
pub fn encode_paged_working_state_with<E>(
    state: &WorkingState,
    observation_index: WorkingStateArtifact,
    publish: impl for<'a> FnMut(EncodedWorkingStatePart<'a>) -> Result<(), E>,
) -> Result<Vec<u8>, WorkingStateWriteError<E>> {
    encode_paged_working_state_reusing_with(state, observation_index, None, publish)
        .map(|(root, _)| root)
}

/// Encodes a PWP2 snapshot while reusing unchanged immutable retired-entry pages.
///
/// The returned history is safe to adopt only after the caller has durably published the returned
/// root and all emitted dependencies. Mutable pages are always freshly encoded. A retired page is
/// reused only when every decoded semantic entry still exactly matches the same canonical range.
///
/// # Errors
/// Rejects the same malformed state, source-index, page, and sink failures as
/// [`encode_paged_working_state_with`].
#[allow(clippy::too_many_lines, reason = "the five canonical page kinds have distinct codecs")]
pub fn encode_paged_working_state_reusing_with<E>(
    state: &WorkingState,
    observation_index: WorkingStateArtifact,
    history: Option<&ReusableWorkingStateHistory>,
    publish: impl for<'a> FnMut(EncodedWorkingStatePart<'a>) -> Result<(), E>,
) -> Result<(Vec<u8>, ReusableWorkingStateHistory), WorkingStateWriteError<E>> {
    super::state::validate_snapshot(state)?;
    let (active_entries, retired_entries) = partition_entries(&state.entries);
    let mut publisher = StreamingPagePublisher::new(publish)?;

    for (page_index, records) in active_entries.chunks(ENTRY_PAGE_ITEMS).enumerate() {
        let first = page_index.checked_mul(ENTRY_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let mut page = writer(WorkingStatePageKind::Entries.magic())?;
        page.write_u64(usize_to_u64(first)?)?;
        page.write_collection_len(records.len())?;
        for record in records { entry::write_entry(&mut page, record)?; }
        publisher.data(&encoded_page(
            WorkingStatePageKind::Entries,
            first,
            records.len(),
            page.into_bytes(),
        )?)?;
    }
    for (page_index, requirements) in
        state.protocol.requirements().chunks(PROTOCOL_PAGE_ITEMS).enumerate()
    {
        let first = page_index.checked_mul(PROTOCOL_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let mut page = writer(WorkingStatePageKind::Requirements.magic())?;
        page.write_u64(usize_to_u64(first)?)?;
        page.write_collection_len(requirements.len())?;
        for id in requirements { page.write_u64(id.get())?; }
        publisher.data(&encoded_page(
            WorkingStatePageKind::Requirements,
            first,
            requirements.len(),
            page.into_bytes(),
        )?)?;
    }
    for (page_index, pending) in
        state.protocol.pending().chunks(PROTOCOL_PAGE_ITEMS).enumerate()
    {
        let first = page_index.checked_mul(PROTOCOL_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let mut page = writer(WorkingStatePageKind::PendingOperations.magic())?;
        page.write_u64(usize_to_u64(first)?)?;
        page.write_collection_len(pending.len())?;
        for operation in pending { super::protocol::write_pending(&mut page, *operation)?; }
        publisher.data(&encoded_page(
            WorkingStatePageKind::PendingOperations,
            first,
            pending.len(),
            page.into_bytes(),
        )?)?;
    }
    for (page_index, files) in
        state.environment().files().chunks(ENVIRONMENT_PAGE_ITEMS).enumerate()
    {
        let first = page_index.checked_mul(ENVIRONMENT_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let mut page = writer(WorkingStatePageKind::EnvironmentFiles.magic())?;
        page.write_u64(usize_to_u64(first)?)?;
        page.write_collection_len(files.len())?;
        for file in files {
            page.write_fixed(file.key().as_bytes())?;
            page.write_fixed(file.digest().as_bytes())?;
        }
        publisher.data(&encoded_page(
            WorkingStatePageKind::EnvironmentFiles,
            first,
            files.len(),
            page.into_bytes(),
        )?)?;
    }
    for (page_index, records) in retired_entries.chunks(ENTRY_PAGE_ITEMS).enumerate() {
        let first = page_index.checked_mul(ENTRY_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        if let Some(reference) = history.and_then(|history| history.reusable(first, records)) {
            publisher.reused(reference)?;
            continue;
        }
        let mut page = writer(WorkingStatePageKind::RetiredEntries.magic())?;
        page.write_u64(usize_to_u64(first)?)?;
        page.write_collection_len(records.len())?;
        for record in records { entry::write_entry(&mut page, record)?; }
        publisher.data(&encoded_page(
            WorkingStatePageKind::RetiredEntries,
            first,
            records.len(),
            page.into_bytes(),
        )?)?;
    }

    let (page_count, descriptor_page_count, descriptor_tail, descriptors) = publisher.finish()?;
    let mut root = writer(*b"PWP2")?;
    super::state::write_limits(&mut root, state.limits())?;
    fields::write_binding(&mut root, state.binding())?;
    root.write_fixed(state.environment().candidate().as_bytes())?;
    root.write_u64(state.revision())?;
    root.write_u64(state.through_observation())?;
    write_artifact(&mut root, observation_index)?;
    root.write_u64(usize_to_u64(active_entries.len())?)?;
    root.write_u64(usize_to_u64(state.protocol.requirements().len())?)?;
    root.write_u64(usize_to_u64(state.protocol.pending().len())?)?;
    root.write_u64(usize_to_u64(state.environment().files().len())?)?;
    root.write_u64(usize_to_u64(retired_entries.len())?)?;
    root.write_u64(usize_to_u64(page_count)?)?;
    root.write_u64(usize_to_u64(descriptor_page_count)?)?;
    root.write_option_tag(descriptor_tail.is_some())?;
    if let Some(tail) = descriptor_tail { write_artifact(&mut root, tail)?; }
    let history = reusable_history(&retired_entries, &descriptors)?;
    Ok((root.into_bytes(), history))
}

impl ReusableWorkingStateHistory {
    fn reusable(
        &self,
        first: usize,
        entries: &[&WorkingEntry],
    ) -> Option<WorkingStatePageReference> {
        let end = first.checked_add(entries.len())?;
        let previous = self.retired_entries.get(first..end)?;
        if previous
            .iter()
            .zip(entries.iter().copied())
            .any(|(old, current)| old != current)
        {
            return None;
        }
        let descriptor = *self.retired_pages.get(first / ENTRY_PAGE_ITEMS)?;
        if descriptor.kind != WorkingStatePageKind::RetiredEntries
            || descriptor.first != usize_to_u64(first).ok()?
            || descriptor.count != usize_to_u64(entries.len()).ok()?
        {
            return None;
        }
        Some(WorkingStatePageReference {
            kind: descriptor.kind,
            first: descriptor.first,
            count: descriptor.count,
            artifact: descriptor.artifact,
        })
    }
}

fn reusable_history(
    entries: &[&WorkingEntry],
    descriptors: &[PageDescriptor],
) -> Result<ReusableWorkingStateHistory, WorkingCodecError> {
    let mut retired_entries = Vec::new();
    retired_entries
        .try_reserve_exact(entries.len())
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    retired_entries.extend(entries.iter().map(|entry| (*entry).clone()));
    let retired_pages = descriptors
        .iter()
        .copied()
        .filter(|descriptor| descriptor.kind == WorkingStatePageKind::RetiredEntries)
        .collect::<Vec<_>>();
    if retired_pages.len() != page_count(retired_entries.len(), ENTRY_PAGE_ITEMS)? {
        return Err(WorkingCodecError::InvalidValue);
    }
    Ok(ReusableWorkingStateHistory { retired_entries, retired_pages })
}

impl<F> StreamingPagePublisher<F> {
    fn new(publish: F) -> Result<Self, WorkingCodecError> {
        let mut descriptors = Vec::new();
        descriptors.try_reserve_exact(DESCRIPTOR_PAGE_ITEMS)
            .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
        Ok(Self {
            publish,
            descriptors,
            published: Vec::new(),
            descriptor_tail: None,
            page_count: 0,
            descriptor_page_count: 0,
        })
    }

    fn data<E>(
        &mut self,
        page: &EncodedWorkingStatePage,
    ) -> Result<(), WorkingStateWriteError<E>>
    where
        F: for<'a> FnMut(EncodedWorkingStatePart<'a>) -> Result<(), E>,
    {
        (self.publish)(EncodedWorkingStatePart::Data(page))
            .map_err(WorkingStateWriteError::Artifact)?;
        let descriptor = PageDescriptor {
            kind: page.kind,
            first: page.first,
            count: page.count,
            artifact: page.artifact,
        };
        self.descriptors.push(descriptor);
        self.published.push(descriptor);
        self.page_count = self.page_count.checked_add(1)
            .ok_or(WorkingCodecError::InvalidValue)?;
        if self.descriptors.len() == DESCRIPTOR_PAGE_ITEMS { self.flush()?; }
        Ok(())
    }

    fn reused<E>(
        &mut self,
        page: WorkingStatePageReference,
    ) -> Result<(), WorkingStateWriteError<E>>
    where
        F: for<'a> FnMut(EncodedWorkingStatePart<'a>) -> Result<(), E>,
    {
        (self.publish)(EncodedWorkingStatePart::Reused(page))
            .map_err(WorkingStateWriteError::Artifact)?;
        let descriptor = PageDescriptor {
            kind: page.kind,
            first: page.first,
            count: page.count,
            artifact: page.artifact,
        };
        self.descriptors.push(descriptor);
        self.published.push(descriptor);
        self.page_count = self.page_count.checked_add(1)
            .ok_or(WorkingCodecError::InvalidValue)?;
        if self.descriptors.len() == DESCRIPTOR_PAGE_ITEMS { self.flush()?; }
        Ok(())
    }

    fn flush<E>(&mut self) -> Result<(), WorkingStateWriteError<E>>
    where
        F: for<'a> FnMut(EncodedWorkingStatePart<'a>) -> Result<(), E>,
    {
        if self.descriptors.is_empty() { return Ok(()); }
        let first = self.page_count.checked_sub(self.descriptors.len())
            .ok_or(WorkingCodecError::InvalidValue)?;
        let page = encode_descriptor_page(
            self.descriptor_page_count,
            first,
            &self.descriptors,
            self.descriptor_tail,
        )?;
        (self.publish)(EncodedWorkingStatePart::Descriptor(&page))
            .map_err(WorkingStateWriteError::Artifact)?;
        self.descriptor_tail = Some(page.artifact);
        self.descriptor_page_count = self.descriptor_page_count.checked_add(1)
            .ok_or(WorkingCodecError::InvalidValue)?;
        self.descriptors.clear();
        Ok(())
    }

    fn finish<E>(
        mut self,
    ) -> Result<
        (usize, usize, Option<WorkingStateArtifact>, Vec<PageDescriptor>),
        WorkingStateWriteError<E>,
    >
    where
        F: for<'a> FnMut(EncodedWorkingStatePart<'a>) -> Result<(), E>,
    {
        self.flush()?;
        Ok((
            self.page_count,
            self.descriptor_page_count,
            self.descriptor_tail,
            self.published,
        ))
    }
}

/// Restores a paged working snapshot against its exact source-index root and page artifacts.
///
/// # Errors
/// Rejects missing, reordered, duplicated, corrupt, oversized, cross-lineage, or incomplete pages
/// before returning a usable state.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "root, linked index, data pages, source index, and scope are independently authenticated"
)]
pub fn decode_paged_working_state(
    root_bytes: &[u8],
    descriptor_page_bytes: &[&[u8]],
    page_bytes: &[&[u8]],
    observation_index: WorkingStateArtifact,
    observations: &[ObservationSource],
    expected: WorkingBinding,
    maximum: WorkingLimits,
) -> Result<WorkingState, WorkingCodecError> {
    let mut root = decode_paged_root(
        root_bytes,
        observation_index,
        observations,
        expected,
        maximum,
    )?;
    if root.descriptor_count != page_bytes.len()
        || root.descriptor_page_count != descriptor_page_bytes.len()
    {
        return Err(WorkingCodecError::InvalidValue);
    }
    let descriptors = decode_descriptor_pages(
        descriptor_page_bytes,
        root.descriptor_count,
        root.descriptor_tail,
    )?;
    validate_descriptor_sequence(descriptors.iter(), root.descriptor_count, root.counts)?;
    root.state.entries.try_reserve_exact(root.counts[0])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut retired_entries = Vec::new();
    retired_entries.try_reserve_exact(root.counts[4])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut requirements = Vec::new();
    requirements.try_reserve_exact(root.counts[1])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut pending = Vec::new();
    pending.try_reserve_exact(root.counts[2])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut files = Vec::new();
    files.try_reserve_exact(root.counts[3])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    for (descriptor, bytes) in descriptors.iter().zip(page_bytes) {
        verify_page_artifact(*descriptor, bytes)?;
        match descriptor.kind {
            WorkingStatePageKind::Entries => {
                decode_entry_page(*descriptor, bytes, root.state.limits(), &mut root.state.entries)?;
            }
            WorkingStatePageKind::Requirements => {
                decode_requirement_page(*descriptor, bytes, &mut requirements)?;
            }
            WorkingStatePageKind::PendingOperations => {
                decode_pending_page(*descriptor, bytes, &mut pending)?;
            }
            WorkingStatePageKind::EnvironmentFiles => {
                decode_environment_page(*descriptor, bytes, &mut files)?;
            }
            WorkingStatePageKind::RetiredEntries => {
                decode_entry_page(*descriptor, bytes, root.state.limits(), &mut retired_entries)?;
            }
        }
    }
    finish_paged_state(root, retired_entries, files, requirements, pending)
}

/// Loads a paged snapshot by following the fixed root's authenticated descriptor-chain tail.
///
/// The reader is invoked only for an exact digest/length pair already committed by the root,
/// a descriptor predecessor, or a verified data-page descriptor.
///
/// # Errors
/// Rejects an unreadable or invalid artifact, a broken descriptor chain, or any snapshot error
/// accepted by [`decode_paged_working_state`].
pub fn decode_paged_working_state_from<E>(
    root_bytes: &[u8],
    observation_index: WorkingStateArtifact,
    observations: &[ObservationSource],
    expected: WorkingBinding,
    maximum: WorkingLimits,
    read: impl FnMut(WorkingStateArtifact) -> Result<Vec<u8>, E>,
) -> Result<WorkingState, WorkingStateReadError<E>> {
    decode_paged_working_state_with_history_from(
        root_bytes,
        observation_index,
        observations,
        expected,
        maximum,
        read,
    )
    .map(|(state, _)| state)
}

/// Loads a PWP2 snapshot and its verified immutable-history reuse frontier.
///
/// # Errors
/// Rejects the same artifact, descriptor-chain, and semantic failures as
/// [`decode_paged_working_state_from`].
pub fn decode_paged_working_state_with_history_from<E>(
    root_bytes: &[u8],
    observation_index: WorkingStateArtifact,
    observations: &[ObservationSource],
    expected: WorkingBinding,
    maximum: WorkingLimits,
    mut read: impl FnMut(WorkingStateArtifact) -> Result<Vec<u8>, E>,
) -> Result<(WorkingState, ReusableWorkingStateHistory), WorkingStateReadError<E>> {
    let mut root = decode_paged_root(
        root_bytes,
        observation_index,
        observations,
        expected,
        maximum,
    )?;
    let mut reversed = Vec::new();
    reversed.try_reserve_exact(root.descriptor_page_count)
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut current = root.descriptor_tail;
    let mut remaining = root.descriptor_page_count;
    while remaining != 0 {
        let artifact = current.ok_or(WorkingCodecError::InvalidValue)?;
        let bytes = read(artifact).map_err(WorkingStateReadError::Artifact)?;
        let page_index = remaining - 1;
        let decoded = decode_linked_descriptor_page(
            &bytes,
            artifact,
            page_index,
            root.descriptor_count,
        )?;
        current = decoded.previous;
        reversed.push(decoded);
        remaining -= 1;
    }
    if current.is_some() {
        return Err(WorkingCodecError::InvalidValue.into());
    }
    reversed.reverse();
    let mut descriptors = Vec::new();
    descriptors.try_reserve_exact(root.descriptor_count)
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    for page in reversed {
        descriptors.extend(page.descriptors);
    }
    validate_descriptor_sequence(
        descriptors.iter(),
        root.descriptor_count,
        root.counts,
    )?;
    root.state.entries.try_reserve_exact(root.counts[0])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut retired_entries = Vec::new();
    retired_entries.try_reserve_exact(root.counts[4])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut requirements = Vec::new();
    requirements.try_reserve_exact(root.counts[1])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut pending = Vec::new();
    pending.try_reserve_exact(root.counts[2])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut files = Vec::new();
    files.try_reserve_exact(root.counts[3])
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let limits = root.state.limits();
    for descriptor in descriptors.iter().copied() {
        let bytes = read(descriptor.artifact).map_err(WorkingStateReadError::Artifact)?;
        verify_page_artifact(descriptor, &bytes)?;
        match descriptor.kind {
            WorkingStatePageKind::Entries => {
                decode_entry_page(descriptor, &bytes, limits, &mut root.state.entries)?;
            }
            WorkingStatePageKind::Requirements => {
                decode_requirement_page(descriptor, &bytes, &mut requirements)?;
            }
            WorkingStatePageKind::PendingOperations => {
                decode_pending_page(descriptor, &bytes, &mut pending)?;
            }
            WorkingStatePageKind::EnvironmentFiles => {
                decode_environment_page(descriptor, &bytes, &mut files)?;
            }
            WorkingStatePageKind::RetiredEntries => {
                decode_entry_page(descriptor, &bytes, limits, &mut retired_entries)?;
            }
        }
    }
    let state = finish_paged_state(root, retired_entries, files, requirements, pending)
        .map_err(WorkingStateReadError::Codec)?;
    let (_, retired_entries) = partition_entries(&state.entries);
    let history = reusable_history(&retired_entries, &descriptors)?;
    Ok((state, history))
}

fn decode_paged_root(
    root_bytes: &[u8],
    observation_index: WorkingStateArtifact,
    observations: &[ObservationSource],
    expected: WorkingBinding,
    maximum: WorkingLimits,
) -> Result<DecodedWorkingStateRoot, WorkingCodecError> {
    let partitioned_entries = match root_bytes.get(..4) {
        Some(bytes) if bytes == b"PWP2" => true,
        Some(bytes) if bytes == b"PWPR" => false,
        _ => return Err(WorkingCodecError::InvalidValue),
    };
    let mut root = reader(root_bytes, if partitioned_entries { *b"PWP2" } else { *b"PWPR" })?;
    let limits = super::state::read_limits(&mut root, maximum)?;
    let binding = fields::read_binding(&mut root)?;
    let candidate = Sha256Digest::new(root.read_fixed()?);
    if !expected.same_lineage(binding) {
        return Err(WorkingError::BindingMismatch.into());
    }
    let revision = root.read_u64()?;
    let through = root.read_u64()?;
    if read_artifact(&mut root)? != observation_index
        || through != usize_to_u64(observations.len())?
    {
        return Err(WorkingCodecError::InvalidValue);
    }
    let entries = logical_usize(root.read_u64()?)?;
    let requirements = logical_usize(root.read_u64()?)?;
    let pending = logical_usize(root.read_u64()?)?;
    let files = logical_usize(root.read_u64()?)?;
    let retired_entries = if partitioned_entries {
        logical_usize(root.read_u64()?)?
    } else {
        0
    };
    let entry_pages = page_count(entries, ENTRY_PAGE_ITEMS)?;
    let requirement_pages = page_count(requirements, PROTOCOL_PAGE_ITEMS)?;
    let pending_pages = page_count(pending, PROTOCOL_PAGE_ITEMS)?;
    let file_pages = page_count(files, ENVIRONMENT_PAGE_ITEMS)?;
    let retired_entry_pages = page_count(retired_entries, ENTRY_PAGE_ITEMS)?;
    let expected_descriptors = entry_pages
        .checked_add(requirement_pages)
        .and_then(|count| count.checked_add(pending_pages))
        .and_then(|count| count.checked_add(file_pages))
        .and_then(|count| count.checked_add(retired_entry_pages))
        .ok_or(WorkingCodecError::InvalidValue)?;
    let descriptor_count = logical_usize(root.read_u64()?)?;
    let descriptor_page_count = logical_usize(root.read_u64()?)?;
    let tail = if root.read_option_tag()? {
        Some(read_artifact(&mut root)?)
    } else {
        None
    };
    root.finish()?;
    if descriptor_count != expected_descriptors
        || descriptor_page_count != page_count(descriptor_count, DESCRIPTOR_PAGE_ITEMS)?
        || tail.is_some() != (descriptor_count != 0)
    {
        return Err(WorkingCodecError::InvalidValue);
    }
    let mut state = WorkingState::new(
        WorkingEnvironment::new(binding, candidate, Vec::new(), limits)?,
        limits,
    )?;
    state.revision = revision;
    adopt_observations(&mut state, observations)?;
    Ok(DecodedWorkingStateRoot {
        state,
        counts: [entries, requirements, pending, files, retired_entries],
        descriptor_count,
        descriptor_page_count,
        descriptor_tail: tail,
        partitioned_entries,
    })
}

fn decode_linked_descriptor_page(
    bytes: &[u8],
    artifact: WorkingStateArtifact,
    page_index: usize,
    descriptor_count: usize,
) -> Result<DecodedDescriptorPage, WorkingCodecError> {
    if artifact.bytes != usize_to_u64(bytes.len())?
        || artifact.digest != sha256(bytes).into_bytes()
    {
        return Err(WorkingCodecError::InvalidValue);
    }
    let first = page_index.checked_mul(DESCRIPTOR_PAGE_ITEMS)
        .ok_or(WorkingCodecError::InvalidValue)?;
    let mut page = reader(bytes, *b"PWPD")?;
    if page.read_u64()? != usize_to_u64(page_index)?
        || page.read_u64()? != usize_to_u64(first)?
    {
        return Err(WorkingCodecError::InvalidValue);
    }
    let previous = if page.read_option_tag()? {
        Some(read_artifact(&mut page)?)
    } else {
        None
    };
    if previous.is_some() != (page_index != 0) {
        return Err(WorkingCodecError::InvalidValue);
    }
    let expected = descriptor_count.checked_sub(first)
        .ok_or(WorkingCodecError::InvalidValue)?
        .min(DESCRIPTOR_PAGE_ITEMS);
    let item_count = count(&mut page, DESCRIPTOR_PAGE_ITEMS, 1 + 8 + 8 + 32 + 8)?;
    if item_count != expected || item_count == 0 {
        return Err(WorkingCodecError::InvalidValue);
    }
    let mut descriptors = Vec::new();
    descriptors.try_reserve_exact(item_count)
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    for _ in 0..item_count {
        descriptors.push(PageDescriptor {
            kind: WorkingStatePageKind::from_tag(page.read_u8()?)?,
            first: page.read_u64()?,
            count: page.read_u64()?,
            artifact: read_artifact(&mut page)?,
        });
    }
    page.finish()?;
    Ok(DecodedDescriptorPage { previous, descriptors })
}

fn encode_descriptor_pages(
    pages: &[EncodedWorkingStatePage],
) -> Result<Vec<EncodedWorkingStateDescriptorPage>, WorkingCodecError> {
    let mut encoded = Vec::new();
    let mut previous = None;
    for (page_index, descriptors) in pages.chunks(DESCRIPTOR_PAGE_ITEMS).enumerate() {
        let first = page_index.checked_mul(DESCRIPTOR_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let mut page_descriptors = Vec::new();
        page_descriptors.try_reserve_exact(descriptors.len())
            .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
        page_descriptors.extend(descriptors.iter().map(|page| PageDescriptor {
            kind: page.kind,
            first: page.first,
            count: page.count,
            artifact: page.artifact,
        }));
        let page = encode_descriptor_page(page_index, first, &page_descriptors, previous)?;
        previous = Some(page.artifact);
        encoded.push(page);
    }
    Ok(encoded)
}

fn encode_descriptor_page(
    page_index: usize,
    first: usize,
    descriptors: &[PageDescriptor],
    previous: Option<WorkingStateArtifact>,
) -> Result<EncodedWorkingStateDescriptorPage, WorkingCodecError> {
    if descriptors.is_empty() || descriptors.len() > DESCRIPTOR_PAGE_ITEMS {
        return Err(WorkingCodecError::InvalidValue);
    }
    let mut page = writer(*b"PWPD")?;
    page.write_u64(usize_to_u64(page_index)?)?;
    page.write_u64(usize_to_u64(first)?)?;
    page.write_option_tag(previous.is_some())?;
    if let Some(artifact) = previous { write_artifact(&mut page, artifact)?; }
    page.write_collection_len(descriptors.len())?;
    for descriptor in descriptors {
        page.write_u8(descriptor.kind.tag())?;
        page.write_u64(descriptor.first)?;
        page.write_u64(descriptor.count)?;
        write_artifact(&mut page, descriptor.artifact)?;
    }
    let bytes = page.into_bytes();
    let artifact = WorkingStateArtifact::new(
        sha256(&bytes).into_bytes(),
        usize_to_u64(bytes.len())?,
    )?;
    Ok(EncodedWorkingStateDescriptorPage {
        first: usize_to_u64(first)?,
        count: usize_to_u64(descriptors.len())?,
        artifact,
        bytes,
    })
}

fn decode_descriptor_pages(
    page_bytes: &[&[u8]],
    descriptor_count: usize,
    tail: Option<WorkingStateArtifact>,
) -> Result<Vec<PageDescriptor>, WorkingCodecError> {
    let mut descriptors = Vec::new();
    descriptors.try_reserve_exact(descriptor_count)
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    let mut previous = None;
    for (page_index, bytes) in page_bytes.iter().enumerate() {
        let artifact = WorkingStateArtifact::new(
            sha256(bytes).into_bytes(),
            usize_to_u64(bytes.len())?,
        )?;
        let mut page = reader(bytes, *b"PWPD")?;
        if page.read_u64()? != usize_to_u64(page_index)?
            || page.read_u64()? != usize_to_u64(descriptors.len())?
        {
            return Err(WorkingCodecError::InvalidValue);
        }
        let linked = if page.read_option_tag()? {
            Some(read_artifact(&mut page)?)
        } else {
            None
        };
        if linked != previous {
            return Err(WorkingCodecError::InvalidValue);
        }
        let remaining = descriptor_count.checked_sub(descriptors.len())
            .ok_or(WorkingCodecError::InvalidValue)?;
        let expected = remaining.min(DESCRIPTOR_PAGE_ITEMS);
        let item_count = count(
            &mut page,
            DESCRIPTOR_PAGE_ITEMS,
            1 + 8 + 8 + 32 + 8,
        )?;
        if item_count != expected || item_count == 0 {
            return Err(WorkingCodecError::InvalidValue);
        }
        for _ in 0..item_count {
            descriptors.push(PageDescriptor {
                kind: WorkingStatePageKind::from_tag(page.read_u8()?)?,
                first: page.read_u64()?,
                count: page.read_u64()?,
                artifact: read_artifact(&mut page)?,
            });
        }
        page.finish()?;
        previous = Some(artifact);
    }
    if descriptors.len() != descriptor_count || previous != tail {
        return Err(WorkingCodecError::InvalidValue);
    }
    Ok(descriptors)
}

fn validate_descriptor_sequence<'a>(
    descriptors: impl IntoIterator<Item = &'a PageDescriptor>,
    descriptor_count: usize,
    counts: [usize; 5],
) -> Result<(), WorkingCodecError> {
    let mut declared_through = [0_u64; 5];
    let mut previous_tag = None;
    let mut seen = 0_usize;
    for descriptor in descriptors {
        let tag = descriptor.kind.tag();
        let kind_index = usize::from(tag);
        let remaining = usize_to_u64(counts[kind_index])?
            .checked_sub(descriptor.first)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let expected_count = remaining.min(usize_to_u64(page_item_limit(descriptor.kind))?);
        if previous_tag.is_some_and(|previous| tag < previous)
            || descriptor.first != declared_through[kind_index]
            || descriptor.count != expected_count
            || descriptor.count == 0
        {
            return Err(WorkingCodecError::InvalidValue);
        }
        declared_through[kind_index] = descriptor.first.checked_add(descriptor.count)
            .ok_or(WorkingCodecError::InvalidValue)?;
        previous_tag = Some(tag);
        seen = seen.checked_add(1).ok_or(WorkingCodecError::InvalidValue)?;
    }
    if seen != descriptor_count
        || declared_through
            != [
                usize_to_u64(counts[0])?,
                usize_to_u64(counts[1])?,
                usize_to_u64(counts[2])?,
                usize_to_u64(counts[3])?,
                usize_to_u64(counts[4])?,
            ]
    {
        return Err(WorkingCodecError::InvalidValue);
    }
    Ok(())
}

fn finish_paged_state(
    mut root: DecodedWorkingStateRoot,
    mut retired_entries: Vec<WorkingEntry>,
    files: Vec<WorkingFileDigest>,
    requirements: Vec<ObservationId>,
    pending: Vec<WorkingPendingOperation>,
) -> Result<WorkingState, WorkingCodecError> {
    if root.state.entries.len() != root.counts[0]
        || retired_entries.len() != root.counts[4]
        || requirements.len() != root.counts[1]
        || pending.len() != root.counts[2]
        || files.len() != root.counts[3]
    {
        return Err(WorkingCodecError::InvalidValue);
    }
    let active_ids = root.state.entries.iter().map(WorkingEntry::id).collect::<Vec<_>>();
    let retired_ids = retired_entries.iter().map(WorkingEntry::id).collect::<Vec<_>>();
    root.state.entries.append(&mut retired_entries);
    root.state.entries.sort_by_key(WorkingEntry::id);
    if root.state.entries.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
        return Err(WorkingError::NonCanonicalOrder.into());
    }
    if root.partitioned_entries {
        let (expected_active, expected_retired) = partition_entries(&root.state.entries);
        if expected_active.len() != active_ids.len()
            || expected_retired.len() != retired_ids.len()
            || expected_active.iter().zip(&active_ids).any(|(entry, id)| entry.id() != *id)
            || expected_retired.iter().zip(&retired_ids).any(|(entry, id)| entry.id() != *id)
        {
            return Err(WorkingCodecError::InvalidValue);
        }
    }
    root.state.environment = WorkingEnvironment::new(
        root.state.binding(),
        root.state.environment().candidate(),
        files,
        root.state.limits(),
    )?;
    root.state.protocol = WorkingProtocol::new(requirements, pending, root.state.limits())?;
    super::state::validate_snapshot(&root.state)?;
    Ok(root.state)
}

fn encode_entry_pages(
    kind: WorkingStatePageKind,
    entries: &[&WorkingEntry],
    pages: &mut Vec<EncodedWorkingStatePage>,
) -> Result<(), WorkingCodecError> {
    for (page_index, records) in entries.chunks(ENTRY_PAGE_ITEMS).enumerate() {
        let first = page_index.checked_mul(ENTRY_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let mut page = writer(kind.magic())?;
        page.write_u64(usize_to_u64(first)?)?;
        page.write_collection_len(records.len())?;
        for record in records { entry::write_entry(&mut page, record)?; }
        push_page(pages, kind, first, records.len(), page.into_bytes())?;
    }
    Ok(())
}

fn partition_entries(entries: &[WorkingEntry]) -> (Vec<&WorkingEntry>, Vec<&WorkingEntry>) {
    let mut active = entries
        .iter()
        .filter(|entry| !entry.status().is_retired())
        .map(WorkingEntry::id)
        .collect::<BTreeSet<_>>();
    let mut frontier = active.iter().copied().collect::<Vec<_>>();
    let mut cursor = 0;
    while cursor < frontier.len() {
        let id = frontier[cursor];
        cursor += 1;
        let Ok(index) = entries.binary_search_by_key(&id, WorkingEntry::id) else {
            continue;
        };
        for dependency in entries[index].links().depends_on() {
            if active.insert(*dependency) {
                frontier.push(*dependency);
            }
        }
    }
    entries.iter().partition(|entry| active.contains(&entry.id()))
}

fn encode_requirement_pages(
    state: &WorkingState,
    pages: &mut Vec<EncodedWorkingStatePage>,
) -> Result<(), WorkingCodecError> {
    for (page_index, requirements) in state.protocol.requirements().chunks(PROTOCOL_PAGE_ITEMS).enumerate() {
        let first = page_index.checked_mul(PROTOCOL_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let mut page = writer(WorkingStatePageKind::Requirements.magic())?;
        page.write_u64(usize_to_u64(first)?)?;
        page.write_collection_len(requirements.len())?;
        for id in requirements { page.write_u64(id.get())?; }
        push_page(pages, WorkingStatePageKind::Requirements, first, requirements.len(), page.into_bytes())?;
    }
    Ok(())
}

fn encode_pending_pages(
    state: &WorkingState,
    pages: &mut Vec<EncodedWorkingStatePage>,
) -> Result<(), WorkingCodecError> {
    for (page_index, pending) in state.protocol.pending().chunks(PROTOCOL_PAGE_ITEMS).enumerate() {
        let first = page_index.checked_mul(PROTOCOL_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let mut page = writer(WorkingStatePageKind::PendingOperations.magic())?;
        page.write_u64(usize_to_u64(first)?)?;
        page.write_collection_len(pending.len())?;
        for operation in pending { super::protocol::write_pending(&mut page, *operation)?; }
        push_page(
            pages,
            WorkingStatePageKind::PendingOperations,
            first,
            pending.len(),
            page.into_bytes(),
        )?;
    }
    Ok(())
}

fn encode_environment_pages(
    state: &WorkingState,
    pages: &mut Vec<EncodedWorkingStatePage>,
) -> Result<(), WorkingCodecError> {
    for (page_index, files) in
        state.environment().files().chunks(ENVIRONMENT_PAGE_ITEMS).enumerate()
    {
        let first = page_index.checked_mul(ENVIRONMENT_PAGE_ITEMS)
            .ok_or(WorkingCodecError::InvalidValue)?;
        let mut page = writer(WorkingStatePageKind::EnvironmentFiles.magic())?;
        page.write_u64(usize_to_u64(first)?)?;
        page.write_collection_len(files.len())?;
        for file in files {
            page.write_fixed(file.key().as_bytes())?;
            page.write_fixed(file.digest().as_bytes())?;
        }
        push_page(
            pages,
            WorkingStatePageKind::EnvironmentFiles,
            first,
            files.len(),
            page.into_bytes(),
        )?;
    }
    Ok(())
}

fn push_page(
    pages: &mut Vec<EncodedWorkingStatePage>,
    kind: WorkingStatePageKind,
    first: usize,
    count: usize,
    bytes: Vec<u8>,
) -> Result<(), WorkingCodecError> {
    pages.push(encoded_page(kind, first, count, bytes)?);
    Ok(())
}

fn encoded_page(
    kind: WorkingStatePageKind,
    first: usize,
    count: usize,
    bytes: Vec<u8>,
) -> Result<EncodedWorkingStatePage, WorkingCodecError> {
    let artifact = WorkingStateArtifact::new(sha256(&bytes).into_bytes(), usize_to_u64(bytes.len())?)?;
    Ok(EncodedWorkingStatePage {
        kind,
        first: usize_to_u64(first)?,
        count: usize_to_u64(count)?,
        artifact,
        bytes,
    })
}

fn decode_entry_page(
    descriptor: PageDescriptor,
    bytes: &[u8],
    limits: WorkingLimits,
    entries: &mut Vec<super::super::WorkingEntry>,
) -> Result<(), WorkingCodecError> {
    if descriptor.first != usize_to_u64(entries.len())? {
        return Err(WorkingCodecError::InvalidValue);
    }
    let mut page = reader(bytes, descriptor.kind.magic())?;
    if page.read_u64()? != descriptor.first {
        return Err(WorkingCodecError::InvalidValue);
    }
    let item_count = count(&mut page, ENTRY_PAGE_ITEMS, entry::MINIMUM_ENTRY_BYTES)?;
    if usize_to_u64(item_count)? != descriptor.count {
        return Err(WorkingCodecError::InvalidValue);
    }
    for _ in 0..item_count {
        let record = entry::read_entry(&mut page, limits)?;
        if entries.last().is_some_and(|old| old.id() >= record.id()) {
            return Err(WorkingError::NonCanonicalOrder.into());
        }
        entries.push(record);
    }
    page.finish()?;
    Ok(())
}

fn decode_requirement_page(
    descriptor: PageDescriptor,
    bytes: &[u8],
    requirements: &mut Vec<ObservationId>,
) -> Result<(), WorkingCodecError> {
    if descriptor.first != usize_to_u64(requirements.len())? {
        return Err(WorkingCodecError::InvalidValue);
    }
    let mut page = reader(bytes, descriptor.kind.magic())?;
    if page.read_u64()? != descriptor.first {
        return Err(WorkingCodecError::InvalidValue);
    }
    let item_count = count(&mut page, PROTOCOL_PAGE_ITEMS, 8)?;
    if usize_to_u64(item_count)? != descriptor.count {
        return Err(WorkingCodecError::InvalidValue);
    }
    for _ in 0..item_count { requirements.push(ObservationId::new(page.read_u64()?)?); }
    page.finish()?;
    Ok(())
}

fn decode_pending_page(
    descriptor: PageDescriptor,
    bytes: &[u8],
    pending: &mut Vec<WorkingPendingOperation>,
) -> Result<(), WorkingCodecError> {
    if descriptor.first != usize_to_u64(pending.len())? {
        return Err(WorkingCodecError::InvalidValue);
    }
    let mut page = reader(bytes, descriptor.kind.magic())?;
    if page.read_u64()? != descriptor.first {
        return Err(WorkingCodecError::InvalidValue);
    }
    let item_count = count(&mut page, PROTOCOL_PAGE_ITEMS, 16 + 8 + 1)?;
    if usize_to_u64(item_count)? != descriptor.count {
        return Err(WorkingCodecError::InvalidValue);
    }
    for _ in 0..item_count { pending.push(super::protocol::read_pending(&mut page)?); }
    page.finish()?;
    Ok(())
}

fn decode_environment_page(
    descriptor: PageDescriptor,
    bytes: &[u8],
    files: &mut Vec<WorkingFileDigest>,
) -> Result<(), WorkingCodecError> {
    if descriptor.first != usize_to_u64(files.len())? {
        return Err(WorkingCodecError::InvalidValue);
    }
    let mut page = reader(bytes, descriptor.kind.magic())?;
    if page.read_u64()? != descriptor.first {
        return Err(WorkingCodecError::InvalidValue);
    }
    let item_count = count(&mut page, ENVIRONMENT_PAGE_ITEMS, 16 + 32)?;
    if usize_to_u64(item_count)? != descriptor.count {
        return Err(WorkingCodecError::InvalidValue);
    }
    for _ in 0..item_count {
        let file = WorkingFileDigest::new(
            fields::read_id(&mut page)?,
            Sha256Digest::new(page.read_fixed()?),
        );
        if files.last().is_some_and(|old| old.key() >= file.key()) {
            return Err(WorkingError::NonCanonicalOrder.into());
        }
        files.push(file);
    }
    page.finish()?;
    Ok(())
}

fn verify_page_artifact(
    descriptor: PageDescriptor,
    bytes: &[u8],
) -> Result<(), WorkingCodecError> {
    if usize_to_u64(bytes.len())? != descriptor.artifact.bytes
        || sha256(bytes).into_bytes() != descriptor.artifact.digest
    {
        return Err(WorkingCodecError::InvalidValue);
    }
    Ok(())
}

fn adopt_observations(
    state: &mut WorkingState,
    observations: &[ObservationSource],
) -> Result<(), WorkingCodecError> {
    state.observations.try_reserve_exact(observations.len())
        .map_err(|_| WorkingCodecError::State(WorkingError::Capacity))?;
    for (index, source) in observations.iter().copied().enumerate() {
        let expected = usize_to_u64(index)?
            .checked_add(1)
            .ok_or(WorkingCodecError::InvalidValue)?;
        if source.id().get() != expected {
            return Err(WorkingError::SourceSequence.into());
        }
        state.observations.push(source);
    }
    Ok(())
}

fn write_artifact(
    writer: &mut CanonicalWriter,
    artifact: WorkingStateArtifact,
) -> Result<(), WorkingCodecError> {
    writer.write_fixed(&artifact.digest)?;
    writer.write_u64(artifact.bytes)?;
    Ok(())
}

fn read_artifact(reader: &mut CanonicalReader<'_>) -> Result<WorkingStateArtifact, WorkingCodecError> {
    WorkingStateArtifact::new(reader.read_fixed()?, reader.read_u64()?)
}

const fn page_item_limit(kind: WorkingStatePageKind) -> usize {
    match kind {
        WorkingStatePageKind::Entries | WorkingStatePageKind::RetiredEntries => ENTRY_PAGE_ITEMS,
        WorkingStatePageKind::Requirements | WorkingStatePageKind::PendingOperations => {
            PROTOCOL_PAGE_ITEMS
        }
        WorkingStatePageKind::EnvironmentFiles => ENVIRONMENT_PAGE_ITEMS,
    }
}

fn page_count(items: usize, page_items: usize) -> Result<usize, WorkingCodecError> {
    items.checked_add(page_items - 1)
        .map(|value| value / page_items)
        .ok_or(WorkingCodecError::InvalidValue)
}

fn logical_usize(value: u64) -> Result<usize, WorkingCodecError> {
    usize::try_from(value).map_err(|_| WorkingCodecError::InvalidValue)
}

fn usize_to_u64(value: usize) -> Result<u64, WorkingCodecError> {
    u64::try_from(value).map_err(|_| WorkingCodecError::InvalidValue)
}
