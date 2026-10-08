//! Read-only inspection of one exact published checkpoint while its writer may still be active.

use super::super::{
    checkpoint_validation, error,
    memory::checkpoint_index::read_complete_checkpoint_indexes,
    record::{
        ArchivedObservation, CHECKPOINT_SCHEMA_VERSION, CheckpointInspectionRoot,
        CheckpointManifest, HOST_INDEX_CHECKPOINT_SCHEMA_VERSION,
        INDEXED_CHECKPOINT_SCHEMA_VERSION, INDEX_PAGE_SCHEMA_VERSION,
        INSPECTION_INDEX_SCHEMA_VERSION, LEGACY_CHECKPOINT_SCHEMA_VERSION,
        MessageInspectionIndexPage, MessageInspectionPageReference,
        PAGED_CHECKPOINT_SCHEMA_VERSION, SNAPSHOT_CHECKPOINT_SCHEMA_VERSION, SourceIndexPage,
        TranscriptManifest, ViewInspectionChunkReference, ViewInspectionIndexPage, ViewValidation,
        decode, encode,
    },
    tools::hex,
    view_binding,
};
use super::{STATE_KEY, STATE_NAMESPACE, StoredArtifact, identity::StorageIdentity};
use peritus_agent::DeveloperLoopError;
use peritus_artifact_store::{
    ArtifactDigest, ArtifactReadChunk, ArtifactReadHandle, ArtifactStore, StoreConfig,
};
use peritus_codec::sha256;
use peritus_context::working::{
    ObservationId, ObservationSource, WorkingBinding, WorkingLimits, WorkingStateArtifact,
    WorkingStateReadError, WorkingStateRootInspection, decode_paged_working_state_from,
    decode_working_state, decode_working_state_core, inspect_paged_working_state_root,
};
use peritus_journal::JournalReader;
use peritus_model_protocol::{
    ContentBlock, HistoryArchiveIdentity, Message, PhysicalPageCapacity, ProtocolLimits,
    decode_message_archive_page, decode_messages,
};
use peritus_types::Sha256Digest;
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeSet, path::Path};

const SOURCE_PAGE_ALLOCATION: usize = 256;
const MESSAGE_PAGE_ALLOCATION: usize = 16;
const INDEX_PAGE_ENTRIES: usize = 255;
const VIEW_CHUNK_BYTES: usize = 64 * 1024;
const LINEAGE_PAGE_ALLOCATION: usize = 256;

pub(in crate::local_context) fn inspect(
    root: &Path,
    binding: WorkingBinding,
) -> Result<String, DeveloperLoopError> {
    let mut inspection = open(root, binding)?;
    let (sources, messages, archive) = inspection.complete_frontier()?;
    Ok(Value::from_iter([
        ("schema_version", Value::from(1)),
        ("description", Value::from("Exact last published model-visible view. If uncovered_tail is true, the next view has not yet been published; this command does not assemble or recover it.")),
        ("uncovered_tail", Value::from(inspection.uncovered_tail)),
        ("checkpoint", serde_json::to_value(&inspection.manifest).map_err(|_| error("encode inspection manifest"))?),
        ("validation", serde_json::to_value(&inspection.validation).map_err(|_| error("encode inspection validation"))?),
        ("source_index", serde_json::to_value(&sources).map_err(|_| error("encode inspection sources"))?),
        ("readable_messages", Value::from(readable_messages(&messages, "retained in canonical_view_archive_hex"))),
        ("canonical_view_archive_hex", Value::from(hex(&archive))),
    ]).to_string())
}

/// One immutable inspection frontier. Every progress claim identifies exactly what was verified.
pub struct LocalContextInspection {
    manifest: CheckpointManifest,
    validation: ViewValidation,
    backend: InspectionBackend,
    journal: JournalReader,
    uncovered_tail: bool,
    lineage_verified: BTreeSet<u64>,
    complete_verified: bool,
}

enum InspectionBackend {
    Legacy {
        sources: Vec<ArchivedObservation>,
        messages: Vec<Message>,
        view: ArtifactReadHandle,
    },
    Paged(RefCell<PagedInspection>),
}

struct PagedInspection {
    config: StoreConfig,
    binding: WorkingBinding,
    profile: [u8; 16],
    source_index: StoredArtifact,
    root: CheckpointInspectionRoot,
    state_root: WorkingStateRootInspection,
    source_pages: Vec<SourcePageFrontier>,
    message_index_pages: Vec<MessageInspectionIndexPage>,
    view_index_pages: Vec<ViewInspectionIndexPage>,
    message_payload_pages_verified: BTreeSet<u64>,
    view_chunks_verified: BTreeSet<u64>,
    state_pages_verified: usize,
}

#[derive(Clone, Copy)]
struct SourcePageFrontier {
    artifact: StoredArtifact,
    previous: Option<StoredArtifact>,
    first_sequence: u64,
    count: u64,
}

impl LocalContextInspection {
    /// Returns the exact checkpoint identity, logical counts, and current verification frontier.
    ///
    /// # Errors
    /// Returns a JSON encoding failure; no context state is mutated.
    pub fn summary(&self) -> Result<String, DeveloperLoopError> {
        let (source_count, message_count) = self.counts();
        Ok(Value::from_iter([
            ("schema_version", Value::from(3)),
            ("uncovered_tail", Value::from(self.uncovered_tail)),
            ("checkpoint", serde_json::to_value(&self.manifest).map_err(|_| error("encode inspection manifest"))?),
            ("validation", serde_json::to_value(&self.validation).map_err(|_| error("encode inspection validation"))?),
            ("source_count", Value::from(source_count)),
            ("message_count", Value::from(message_count)),
            ("canonical_view_bytes", Value::from(self.manifest.view.bytes)),
            ("verification", self.verification_frontier()),
        ]).to_string())
    }

    /// Reads at most 256 exact source records, extending verification toward the chain origin.
    ///
    /// # Errors
    /// Rejects invalid offsets/allocations or any broken authenticated page relationship.
    pub fn source_page(
        &self,
        offset: usize,
        maximum: usize,
    ) -> Result<String, DeveloperLoopError> {
        let generation = self.manifest.generation;
        let view_digest = self.manifest.view.digest;
        let (total, sources) = match &self.backend {
            InspectionBackend::Legacy { sources, .. } => {
                let end = page_end(sources.len(), offset, maximum, SOURCE_PAGE_ALLOCATION)?;
                (sources.len(), sources[offset..end].to_vec())
            }
            InspectionBackend::Paged(paged) => {
                let mut paged = paged.borrow_mut();
                let total = usize::try_from(paged.root.source_count)
                    .map_err(|_| error("inspection source count overflow"))?;
                let end = page_end(total, offset, maximum, SOURCE_PAGE_ALLOCATION)?;
                (total, paged.source_records(offset, end)?)
            }
        };
        let end = offset.checked_add(sources.len())
            .ok_or_else(|| error("inspection source page overflow"))?;
        Ok(Value::from_iter([
            ("generation", Value::from(generation)),
            ("canonical_view_sha256", Value::from(hex(view_digest.as_bytes()))),
            ("offset", Value::from(offset)),
            ("total", Value::from(total)),
            ("next_offset", if end < total { Value::from(end) } else { Value::Null }),
            ("sources", serde_json::to_value(&sources).map_err(|_| error("encode inspection source page"))?),
            ("verification", self.verification_frontier()),
        ]).to_string())
    }

    /// Reads at most 16 complete message protocol records from bounded authenticated pages.
    ///
    /// # Errors
    /// Rejects invalid offsets/allocations, page conflicts, or invalid message protocol bytes.
    pub fn message_page(
        &self,
        offset: usize,
        maximum: usize,
    ) -> Result<String, DeveloperLoopError> {
        let generation = self.manifest.generation;
        let view_digest = self.manifest.view.digest;
        let (total, messages) = match &self.backend {
            InspectionBackend::Legacy { messages, .. } => {
                let end = page_end(messages.len(), offset, maximum, MESSAGE_PAGE_ALLOCATION)?;
                (messages.len(), messages[offset..end].to_vec())
            }
            InspectionBackend::Paged(paged) => {
                let mut paged = paged.borrow_mut();
                let total = usize::try_from(paged.root.message_count)
                    .map_err(|_| error("inspection message count overflow"))?;
                let end = page_end(total, offset, maximum, MESSAGE_PAGE_ALLOCATION)?;
                (total, paged.message_records(offset, end)?)
            }
        };
        let end = offset.checked_add(messages.len())
            .ok_or_else(|| error("inspection message page overflow"))?;
        Ok(Value::from_iter([
            ("generation", Value::from(generation)),
            ("canonical_view_sha256", Value::from(hex(view_digest.as_bytes()))),
            ("offset", Value::from(offset)),
            ("total", Value::from(total)),
            ("next_offset", if end < total { Value::from(end) } else { Value::Null }),
            ("messages", Value::from(readable_messages(&messages, "retained in read_view canonical archive bytes"))),
            ("verification", self.verification_frontier()),
        ]).to_string())
    }

    /// Reads exact canonical archive bytes from bounded authenticated physical chunks.
    ///
    /// # Errors
    /// Rejects an invalid range/allocation or any missing, changed, or conflicting chunk.
    pub fn read_view(
        &mut self,
        offset: u64,
        maximum: usize,
    ) -> Result<Option<ArtifactReadChunk>, DeveloperLoopError> {
        if maximum == 0 || maximum > VIEW_CHUNK_BYTES {
            return Err(error("inspection view page is outside the physical allocation envelope"));
        }
        match &mut self.backend {
            InspectionBackend::Legacy { view, .. } => view
                .read_chunk_at(offset, maximum)
                .map_err(|_| error("exact inspection view range is unavailable or changed")),
            InspectionBackend::Paged(paged) => paged.get_mut()
                .read_view(offset, maximum, self.manifest.view),
        }
    }

    /// Reads at most 256 checkpoint manifests, newest first, without walking unrelated ancestors.
    ///
    /// `offset` is the number of generations before the accepted current root.
    ///
    /// # Errors
    /// Rejects invalid ranges, missing authenticated history, or any predecessor conflict.
    pub fn lineage_page(
        &mut self,
        offset: usize,
        maximum: usize,
    ) -> Result<String, DeveloperLoopError> {
        let total = usize::try_from(self.manifest.generation)
            .map_err(|_| error("inspection lineage count overflow"))?;
        let end = page_end(total, offset, maximum, LINEAGE_PAGE_ALLOCATION)?;
        let mut manifests = Vec::new();
        manifests.try_reserve_exact(end.saturating_sub(offset))
            .map_err(|_| error("allocate inspection lineage page"))?;
        let mut successor = if offset == 0 {
            None
        } else {
            let revision = self.manifest.generation
                .checked_sub(u64::try_from(offset).map_err(|_| error("lineage offset overflow"))?)
                .and_then(|revision| revision.checked_add(1))
                .ok_or_else(|| error("lineage offset exceeds checkpoint root"))?;
            Some(if revision == self.manifest.generation {
                self.manifest.clone()
            } else {
                self.read_manifest_revision(revision)?.0
            })
        };
        for index in offset..end {
            let distance = u64::try_from(index).map_err(|_| error("lineage offset overflow"))?;
            let revision = self.manifest.generation.checked_sub(distance)
                .ok_or_else(|| error("lineage offset exceeds checkpoint root"))?;
            let (manifest, bytes) = if index == 0 {
                (self.manifest.clone(), encode(&self.manifest)?)
            } else {
                self.read_manifest_revision(revision)?
            };
            if !supported_schema(manifest.schema_version)
                || manifest.scope != self.manifest.scope
                || manifest.generation != revision
            {
                return Err(error("checkpoint lineage scope, generation, or schema mismatch"));
            }
            if let Some(successor_manifest) = successor.as_ref() {
                checkpoint_validation::validate_predecessor(successor_manifest, &manifest)?;
                if successor_manifest.previous != Some(sha256(&bytes).into_bytes()) {
                    return Err(error("checkpoint lineage digest mismatch"));
                }
            }
            if revision == 1 && manifest.previous.is_some() {
                return Err(error("checkpoint lineage origin has a predecessor"));
            }
            self.lineage_verified.insert(revision);
            successor = Some(manifest.clone());
            manifests.push(manifest);
        }
        Ok(Value::from_iter([
            ("generation", Value::from(self.manifest.generation)),
            ("offset", Value::from(offset)),
            ("total", Value::from(total)),
            ("next_offset", if end < total { Value::from(end) } else { Value::Null }),
            ("checkpoints", serde_json::to_value(&manifests).map_err(|_| error("encode inspection lineage page"))?),
            ("verification", self.verification_frontier()),
        ]).to_string())
    }

    fn counts(&self) -> (usize, usize) {
        match &self.backend {
            InspectionBackend::Legacy { sources, messages, .. } => (sources.len(), messages.len()),
            InspectionBackend::Paged(paged) => {
                let paged = paged.borrow();
                (
                    usize::try_from(paged.root.source_count).unwrap_or(usize::MAX),
                    usize::try_from(paged.root.message_count).unwrap_or(usize::MAX),
                )
            }
        }
    }

    fn verification_frontier(&self) -> Value {
        let (state_pages, source_from, source_verified, message_index_from, message_payloads,
            view_index_from, view_chunks, state_complete) = match &self.backend {
            InspectionBackend::Legacy { sources, messages, .. } => (
                0,
                if sources.is_empty() { None } else { Some(1) },
                sources.len(),
                if messages.is_empty() { None } else { Some(0) },
                messages.len().div_ceil(MESSAGE_PAGE_ALLOCATION),
                Some(0),
                usize::try_from(self.manifest.view.bytes).unwrap_or(usize::MAX)
                    .div_ceil(VIEW_CHUNK_BYTES),
                true,
            ),
            InspectionBackend::Paged(paged) => {
                let paged = paged.borrow();
                let source_from = paged.source_pages.last().map(|page| page.first_sequence);
                let source_verified = source_from.map_or(0, |first| {
                    paged.root.source_count.saturating_sub(first).saturating_add(1)
                });
                (
                    paged.state_pages_verified,
                    source_from,
                    usize::try_from(source_verified).unwrap_or(usize::MAX),
                    paged.message_index_pages.last().map(|page| page.first_page),
                    paged.message_payload_pages_verified.len(),
                    paged.view_index_pages.last().map(|page| page.first_chunk),
                    paged.view_chunks_verified.len(),
                    paged.state_pages_verified == paged.state_root.descriptor_page_count(),
                )
            }
        };
        Value::from_iter([
            ("accepted_root", Value::from(true)),
            ("working_state_root", Value::from(true)),
            ("working_state_descriptor_pages_verified", Value::from(state_pages)),
            ("working_state_complete", Value::from(state_complete)),
            ("source_index_verified_from_sequence", source_from.map_or(Value::Null, Value::from)),
            ("source_records_verified", Value::from(source_verified)),
            ("message_index_verified_from_page", message_index_from.map_or(Value::Null, Value::from)),
            ("message_payload_pages_verified", Value::from(message_payloads)),
            ("view_index_verified_from_chunk", view_index_from.map_or(Value::Null, Value::from)),
            ("view_chunks_verified", Value::from(view_chunks)),
            ("lineage_records_verified", Value::from(self.lineage_verified.len())),
            ("complete", Value::from(self.complete_verified)),
        ])
    }

    fn read_manifest_revision(
        &self,
        revision: u64,
    ) -> Result<(CheckpointManifest, Vec<u8>), DeveloperLoopError> {
        let row = self.journal
            .state_record_revision(STATE_NAMESPACE, STATE_KEY, revision)
            .map_err(|_| error("read authenticated checkpoint history"))?
            .ok_or_else(|| error("checkpoint history revision is unavailable"))?;
        let bytes = row.bytes().to_vec();
        Ok((decode(&bytes)?, bytes))
    }

    fn complete_frontier(
        &mut self,
    ) -> Result<(Vec<ArchivedObservation>, Vec<Message>, Vec<u8>), DeveloperLoopError> {
        let completed = match &mut self.backend {
            InspectionBackend::Legacy { sources, messages, view } => {
                let mut archive = Vec::new();
                while let Some(chunk) = view.read_chunk_at(
                    u64::try_from(archive.len()).map_err(|_| error("inspection archive offset overflow"))?,
                    VIEW_CHUNK_BYTES,
                ).map_err(|_| error("read owned inspection view"))? {
                    archive.try_reserve(chunk.bytes().len())
                        .map_err(|_| error("inspection view allocation unavailable"))?;
                    archive.extend_from_slice(chunk.bytes());
                }
                (sources.clone(), messages.clone(), archive)
            }
            InspectionBackend::Paged(paged) => paged.get_mut().complete_frontier(
                &self.manifest,
                &self.validation,
            )?,
        };
        let mut offset = 0_usize;
        let total = usize::try_from(self.manifest.generation)
            .map_err(|_| error("inspection lineage count overflow"))?;
        while offset < total {
            let maximum = (total - offset).min(LINEAGE_PAGE_ALLOCATION);
            self.lineage_page(offset, maximum)?;
            offset = offset.checked_add(maximum)
                .ok_or_else(|| error("inspection lineage offset overflow"))?;
        }
        self.complete_verified = true;
        Ok(completed)
    }
}

impl PagedInspection {
    fn source_records(
        &mut self,
        offset: usize,
        end: usize,
    ) -> Result<Vec<ArchivedObservation>, DeveloperLoopError> {
        if offset == end {
            return Ok(Vec::new());
        }
        self.ensure_source_page(u64::try_from(offset).map_err(|_| error("source offset overflow"))?)?;
        let start_sequence = u64::try_from(offset).ok().and_then(|value| value.checked_add(1))
            .ok_or_else(|| error("source offset overflow"))?;
        let end_sequence = u64::try_from(end).map_err(|_| error("source offset overflow"))?;
        let mut matching = self.source_pages.iter().copied().filter(|page| {
            let page_end = page.first_sequence.saturating_add(page.count);
            page.first_sequence <= end_sequence && page_end > start_sequence
        }).collect::<Vec<_>>();
        matching.reverse();
        let mut records = Vec::new();
        records.try_reserve_exact(end - offset)
            .map_err(|_| error("allocate source inspection page"))?;
        for frontier in matching {
            let page = self.read_source_page(frontier.artifact)?;
            if page.previous != frontier.previous
                || page.first_sequence != frontier.first_sequence
                || u64::try_from(page.observations.len()).ok() != Some(frontier.count)
            {
                return Err(error("source page changed after frontier verification"));
            }
            records.extend(page.observations.into_iter().filter(|source| {
                source.sequence >= start_sequence && source.sequence <= end_sequence
            }));
        }
        if records.len() != end - offset {
            return Err(error("source page range is incomplete"));
        }
        Ok(records)
    }

    fn ensure_source_page(&mut self, target: u64) -> Result<(), DeveloperLoopError> {
        let sequence = target.checked_add(1).ok_or_else(|| error("source sequence overflow"))?;
        while !self.source_pages.iter().any(|page| {
            sequence >= page.first_sequence
                && sequence < page.first_sequence.saturating_add(page.count)
        }) {
            let artifact = self.source_pages.last()
                .map_or(Some(self.source_index), |page| page.previous)
                .ok_or_else(|| error("source index does not cover requested sequence"))?;
            if self.source_pages.iter().any(|page| page.artifact == artifact) {
                return Err(error("source index contains an artifact cycle"));
            }
            let page = self.read_source_page(artifact)?;
            let count = u64::try_from(page.observations.len())
                .map_err(|_| error("source page count overflow"))?;
            let expected_end = self.source_pages.last().map_or_else(
                || self.root.source_count.checked_add(1),
                |page| Some(page.first_sequence),
            ).ok_or_else(|| error("source page frontier overflow"))?;
            let observed_end = page.first_sequence.checked_add(count)
                .ok_or_else(|| error("source page frontier overflow"))?;
            if page.schema_version != INDEX_PAGE_SCHEMA_VERSION
                || page.observations.len() > SOURCE_PAGE_ALLOCATION
                || (page.observations.is_empty() && page.previous.is_some())
                || observed_end != expected_end
                || page.observations.iter().enumerate().any(|(index, source)| {
                    u64::try_from(index).ok()
                        .and_then(|index| page.first_sequence.checked_add(index))
                        != Some(source.sequence)
                })
                || page.previous.is_none() != (page.first_sequence == 1)
            {
                return Err(error("source page conflicts with authenticated frontier"));
            }
            self.source_pages.push(SourcePageFrontier {
                artifact,
                previous: page.previous,
                first_sequence: page.first_sequence,
                count,
            });
        }
        Ok(())
    }

    fn read_source_page(
        &self,
        artifact: StoredArtifact,
    ) -> Result<SourceIndexPage, DeveloperLoopError> {
        decode(&read_artifact(&self.config, artifact)?)
    }

    fn message_records(
        &mut self,
        offset: usize,
        end: usize,
    ) -> Result<Vec<Message>, DeveloperLoopError> {
        let mut messages = Vec::new();
        messages.try_reserve_exact(end.saturating_sub(offset))
            .map_err(|_| error("allocate message inspection page"))?;
        let mut cursor = offset;
        while cursor < end {
            let page_index = cursor / MESSAGE_PAGE_ALLOCATION;
            let reference = self.message_reference(
                u64::try_from(page_index).map_err(|_| error("message page index overflow"))?,
            )?;
            let bytes = read_artifact(&self.config, reference.artifact)?;
            let capacity = PhysicalPageCapacity::new(bytes.len())
                .map_err(|_| error("invalid retained message page capacity"))?;
            let page = decode_message_archive_page(&bytes, ProtocolLimits::PRODUCTION, capacity)
                .map_err(|_| error("invalid retained message page"))?;
            if page.identity() != self.message_identity()?
                || page.page_index() != reference.page_index
                || page.first_message() != reference.first_message
                || u64::try_from(page.messages().len()).ok() != Some(reference.message_count)
                || page.previous_page_digest().map(|digest| digest.into_bytes())
                    != reference.previous_page_digest
                || page.digest().into_bytes() != reference.page_digest
            {
                return Err(error("message page conflicts with authenticated index"));
            }
            let within = cursor % MESSAGE_PAGE_ALLOCATION;
            let take = (end - cursor).min(page.messages().len().saturating_sub(within));
            if take == 0 {
                return Err(error("message page range is incomplete"));
            }
            messages.extend_from_slice(&page.messages()[within..within + take]);
            self.message_payload_pages_verified.insert(reference.page_index);
            cursor = cursor.checked_add(take).ok_or_else(|| error("message offset overflow"))?;
        }
        Ok(messages)
    }

    fn message_reference(
        &mut self,
        page_index: u64,
    ) -> Result<MessageInspectionPageReference, DeveloperLoopError> {
        self.ensure_message_index(page_index)?;
        self.message_index_pages.iter().find_map(|page| {
            page.pages.iter().find(|reference| reference.page_index == page_index).copied()
        }).ok_or_else(|| error("message index does not cover requested page"))
    }

    fn ensure_message_index(&mut self, target: u64) -> Result<(), DeveloperLoopError> {
        while !self.message_index_pages.iter().any(|page| {
            target >= page.first_page
                && target < page.first_page.saturating_add(page.pages.len() as u64)
        }) {
            let artifact = self.message_index_pages.last()
                .map_or(self.root.message_index_tail, |page| page.previous)
                .ok_or_else(|| error("message index does not cover requested page"))?;
            let page: MessageInspectionIndexPage = decode(&read_artifact(&self.config, artifact)?)?;
            let count = u64::try_from(page.pages.len())
                .map_err(|_| error("message index page count overflow"))?;
            let expected_end = self.message_index_pages.last().map_or(
                self.root.message_page_count,
                |successor| successor.first_page,
            );
            if page.schema_version != INSPECTION_INDEX_SCHEMA_VERSION
                || page.pages.is_empty()
                || page.pages.len() > INDEX_PAGE_ENTRIES
                || page.first_page.checked_add(count) != Some(expected_end)
                || page.previous.is_none() != (page.first_page == 0)
            {
                return Err(error("message index page conflicts with authenticated frontier"));
            }
            for (offset, reference) in page.pages.iter().enumerate() {
                let expected_page = page.first_page.checked_add(offset as u64)
                    .ok_or_else(|| error("message page index overflow"))?;
                let expected_first = expected_page.checked_mul(MESSAGE_PAGE_ALLOCATION as u64)
                    .ok_or_else(|| error("message ordinal overflow"))?;
                let remaining = self.root.message_count.checked_sub(expected_first)
                    .ok_or_else(|| error("message index exceeds logical count"))?;
                let expected_count = remaining.min(MESSAGE_PAGE_ALLOCATION as u64);
                let predecessor = if offset == 0 {
                    None
                } else {
                    Some(page.pages[offset - 1].page_digest)
                };
                if reference.page_index != expected_page
                    || reference.first_message != expected_first
                    || reference.message_count != expected_count
                    || reference.message_count == 0
                    || (expected_page == 0 && reference.previous_page_digest.is_some())
                    || (expected_page != 0 && reference.previous_page_digest.is_none())
                    || (expected_page != 0 && offset != 0
                        && reference.previous_page_digest != predecessor)
                    || reference.artifact.bytes == 0
                {
                    return Err(error("message page descriptor is not contiguous"));
                }
            }
            if let Some(successor) = self.message_index_pages.last() {
                let expected = successor.pages.first()
                    .and_then(|reference| reference.previous_page_digest);
                if page.pages.last().map(|reference| reference.page_digest) != expected {
                    return Err(error("message page digest lineage is discontinuous"));
                }
            } else if page.pages.last().map(|reference| reference.page_digest)
                != self.root.last_message_page_digest
            {
                return Err(error("message index tail digest mismatch"));
            }
            self.message_index_pages.push(page);
        }
        Ok(())
    }

    fn message_identity(&self) -> Result<HistoryArchiveIdentity, DeveloperLoopError> {
        HistoryArchiveIdentity::new(
            self.binding.task().into_bytes(),
            self.profile,
            self.binding.run().into_bytes(),
        ).map_err(|_| error("invalid inspection message identity"))
    }

    fn read_view(
        &mut self,
        offset: u64,
        maximum: usize,
        view: StoredArtifact,
    ) -> Result<Option<ArtifactReadChunk>, DeveloperLoopError> {
        if offset > view.bytes {
            return Err(error("inspection view offset exceeds canonical archive"));
        }
        let count = (view.bytes - offset).min(maximum as u64);
        if count == 0 {
            return Ok(None);
        }
        let end = offset.checked_add(count).ok_or_else(|| error("inspection view range overflow"))?;
        let mut cursor = offset;
        let mut output = Vec::new();
        output.try_reserve_exact(usize::try_from(count).map_err(|_| error("inspection view allocation overflow"))?)
            .map_err(|_| error("allocate inspection view page"))?;
        while cursor < end {
            let chunk_index = cursor / VIEW_CHUNK_BYTES as u64;
            let reference = self.view_reference(chunk_index, view.bytes)?;
            let chunk = read_artifact(&self.config, reference.artifact)?;
            let within = usize::try_from(cursor - reference.first_byte)
                .map_err(|_| error("inspection view chunk offset overflow"))?;
            let take = usize::try_from(end - cursor).map_err(|_| error("inspection view range overflow"))?
                .min(chunk.len().saturating_sub(within));
            if take == 0 {
                return Err(error("inspection view chunk range is incomplete"));
            }
            output.extend_from_slice(&chunk[within..within + take]);
            self.view_chunks_verified.insert(chunk_index);
            cursor = cursor.checked_add(take as u64)
                .ok_or_else(|| error("inspection view range overflow"))?;
        }
        ArtifactReadChunk::from_authenticated(offset, output)
            .map(Some)
            .map_err(|_| error("construct authenticated inspection view page"))
    }

    fn view_reference(
        &mut self,
        chunk_index: u64,
        view_bytes: u64,
    ) -> Result<ViewInspectionChunkReference, DeveloperLoopError> {
        self.ensure_view_index(chunk_index, view_bytes)?;
        self.view_index_pages.iter().find_map(|page| {
            page.chunks.iter().find(|reference| reference.chunk_index == chunk_index).copied()
        }).ok_or_else(|| error("view index does not cover requested chunk"))
    }

    fn ensure_view_index(
        &mut self,
        target: u64,
        view_bytes: u64,
    ) -> Result<(), DeveloperLoopError> {
        while !self.view_index_pages.iter().any(|page| {
            target >= page.first_chunk
                && target < page.first_chunk.saturating_add(page.chunks.len() as u64)
        }) {
            let artifact = self.view_index_pages.last()
                .map_or(self.root.view_index_tail, |page| page.previous)
                .ok_or_else(|| error("view index does not cover requested chunk"))?;
            let page: ViewInspectionIndexPage = decode(&read_artifact(&self.config, artifact)?)?;
            let count = u64::try_from(page.chunks.len())
                .map_err(|_| error("view index page count overflow"))?;
            let expected_end = self.view_index_pages.last().map_or(
                self.root.view_chunk_count,
                |successor| successor.first_chunk,
            );
            if page.schema_version != INSPECTION_INDEX_SCHEMA_VERSION
                || page.chunks.is_empty()
                || page.chunks.len() > INDEX_PAGE_ENTRIES
                || page.first_chunk.checked_add(count) != Some(expected_end)
                || page.previous.is_none() != (page.first_chunk == 0)
            {
                return Err(error("view index page conflicts with authenticated frontier"));
            }
            for (offset, reference) in page.chunks.iter().enumerate() {
                let expected_index = page.first_chunk.checked_add(offset as u64)
                    .ok_or_else(|| error("view chunk index overflow"))?;
                let expected_first = expected_index.checked_mul(VIEW_CHUNK_BYTES as u64)
                    .ok_or_else(|| error("view chunk offset overflow"))?;
                let expected_bytes = view_bytes.checked_sub(expected_first)
                    .ok_or_else(|| error("view index exceeds canonical archive"))?
                    .min(VIEW_CHUNK_BYTES as u64);
                if reference.chunk_index != expected_index
                    || reference.first_byte != expected_first
                    || reference.artifact.bytes != expected_bytes
                    || expected_bytes == 0
                {
                    return Err(error("view chunk descriptor is not contiguous"));
                }
            }
            self.view_index_pages.push(page);
        }
        Ok(())
    }

    fn complete_frontier(
        &mut self,
        manifest: &CheckpointManifest,
        validation: &ViewValidation,
    ) -> Result<(Vec<ArchivedObservation>, Vec<Message>, Vec<u8>), DeveloperLoopError> {
        let (sources, transcript, _) = read_complete_checkpoint_indexes(
            manifest.schema_version,
            manifest.source_index,
            manifest.transcript_manifest,
            |artifact| read_artifact(&self.config, artifact),
        )?;
        let observations = observation_sources(&sources)?;
        let working = read_artifact(&self.config, manifest.working_state)?;
        let source_index = WorkingStateArtifact::new(
            manifest.source_index.digest.into_bytes(),
            manifest.source_index.bytes,
        ).map_err(|_| error("invalid inspection paged source-index reference"))?;
        let state = decode_paged_working_state_from(
            &working,
            source_index,
            &observations,
            self.binding,
            WorkingLimits::standard(),
            |artifact| read_artifact(&self.config, StoredArtifact {
                digest: Sha256Digest::new(artifact.digest()),
                bytes: artifact.bytes(),
            }),
        ).map_err(|failure| match failure {
            WorkingStateReadError::Artifact(failure) => failure,
            WorkingStateReadError::Codec(_) => error("invalid paged inspection working state"),
        })?;
        checkpoint_validation::validate_checkpoint(
            manifest.schema_version,
            &state,
            &sources,
            &transcript,
            validation,
            WorkingLimits::standard(),
        )?;
        self.state_pages_verified = self.state_root.descriptor_page_count();

        let archive = read_artifact(&self.config, manifest.view)?;
        view_binding::verify(manifest, &archive, validation)?;
        let messages = decode_messages(&archive, ProtocolLimits::PRODUCTION)?;

        let mut paged_sources = Vec::new();
        let mut offset = 0_usize;
        while offset < sources.len() {
            let end = offset.saturating_add(SOURCE_PAGE_ALLOCATION).min(sources.len());
            paged_sources.extend(self.source_records(offset, end)?);
            offset = end;
        }
        if paged_sources != sources {
            return Err(error("paged source inspection disagrees with checkpoint reconstruction"));
        }
        let mut paged_messages = Vec::new();
        offset = 0;
        while offset < messages.len() {
            let end = offset.saturating_add(MESSAGE_PAGE_ALLOCATION).min(messages.len());
            paged_messages.extend(self.message_records(offset, end)?);
            offset = end;
        }
        if paged_messages != messages {
            return Err(error("paged message inspection disagrees with canonical view"));
        }
        let mut paged_view = Vec::new();
        let mut view_offset = 0_u64;
        while let Some(chunk) = self.read_view(view_offset, VIEW_CHUNK_BYTES, manifest.view)? {
            paged_view.try_reserve(chunk.bytes().len())
                .map_err(|_| error("allocate complete paged inspection view"))?;
            paged_view.extend_from_slice(chunk.bytes());
            view_offset = view_offset.checked_add(chunk.bytes().len() as u64)
                .ok_or_else(|| error("inspection view offset overflow"))?;
        }
        if paged_view != archive {
            return Err(error("paged view inspection disagrees with canonical archive"));
        }
        Ok((sources, messages, archive))
    }
}

fn page_end(
    total: usize,
    offset: usize,
    maximum: usize,
    allocation: usize,
) -> Result<usize, DeveloperLoopError> {
    if offset > total || maximum == 0 || maximum > allocation {
        return Err(error("invalid exact inspection page range"));
    }
    Ok(offset.saturating_add(maximum).min(total))
}

pub(in crate::local_context) fn open(
    root: &Path,
    binding: WorkingBinding,
) -> Result<LocalContextInspection, DeveloperLoopError> {
    let identity = StorageIdentity::new(binding)?;
    let database = root.join("journal.sqlite3");
    let journal = JournalReader::open(&database, identity.store)
        .map_err(|_| error("open exact read-only context journal"))?;
    let row = journal.state_record(STATE_NAMESPACE, STATE_KEY)
        .map_err(|_| error("read exact checkpoint root"))?
        .ok_or_else(|| error("no published model-visible view exists"))?;
    let manifest: CheckpointManifest = decode(row.bytes())?;
    if !supported_schema(manifest.schema_version)
        || manifest.scope != identity.scope.into_bytes()
        || manifest.generation != row.revision()
    {
        return Err(error("inspection scope or generation mismatch"));
    }
    let config = StoreConfig::for_available_space_without_artifact_limit(root.join("artifacts"))
        .and_then(|config| config.with_database_path(&database))
        .map_err(|_| error("invalid inspection artifact configuration"))?;
    let validation: ViewValidation = decode(&read_artifact(&config, manifest.validation)?)?;
    let head = journal.head(identity.aggregate)
        .map_err(|_| error("read context inspection head"))?
        .ok_or_else(|| error("missing checkpoint producing journal"))?;
    let uncovered_tail = head.sequence().get() > manifest.through_event.saturating_add(1);

    let backend = if manifest.schema_version == CHECKPOINT_SCHEMA_VERSION {
        let root = manifest.inspection.clone()
            .ok_or_else(|| error("current checkpoint lacks inspection root"))?;
        let working = read_artifact(&config, manifest.working_state)?;
        let source_index = WorkingStateArtifact::new(
            manifest.source_index.digest.into_bytes(),
            manifest.source_index.bytes,
        ).map_err(|_| error("invalid inspection source-index reference"))?;
        let state_root = inspect_paged_working_state_root(
            &working,
            source_index,
            root.source_count,
            binding,
            WorkingLimits::standard(),
        ).map_err(|_| error("invalid inspection working-state root"))?;
        checkpoint_validation::validate_inspection_root(
            &manifest,
            &validation,
            &root,
            state_root,
        )?;
        view_binding::verify(&manifest, &[], &validation)?;
        InspectionBackend::Paged(RefCell::new(PagedInspection {
            config,
            binding,
            profile: validation.profile,
            source_index: manifest.source_index,
            root,
            state_root,
            source_pages: Vec::new(),
            message_index_pages: Vec::new(),
            view_index_pages: Vec::new(),
            message_payload_pages_verified: BTreeSet::new(),
            view_chunks_verified: BTreeSet::new(),
            state_pages_verified: 0,
        }))
    } else {
        let mut view = ArtifactStore::open_existing(
            &config,
            ArtifactDigest::from_sha256(manifest.view.digest),
        ).map_err(|_| error("open immutable inspection view"))?;
        if view.metadata().size() != manifest.view.bytes {
            return Err(error("inspection view length mismatch"));
        }
        let limits = WorkingLimits::standard();
        let (sources, transcript, state) = if matches!(
            manifest.schema_version,
            INDEXED_CHECKPOINT_SCHEMA_VERSION
                | PAGED_CHECKPOINT_SCHEMA_VERSION
                | HOST_INDEX_CHECKPOINT_SCHEMA_VERSION
        ) {
            let (sources, transcript, _) = read_complete_checkpoint_indexes(
                manifest.schema_version,
                manifest.source_index,
                manifest.transcript_manifest,
                |artifact| read_artifact(&config, artifact),
            )?;
            let observations = observation_sources(&sources)?;
            let working = read_artifact(&config, manifest.working_state)?;
            let state = if matches!(
                manifest.schema_version,
                PAGED_CHECKPOINT_SCHEMA_VERSION | HOST_INDEX_CHECKPOINT_SCHEMA_VERSION
            ) {
                let source_index = WorkingStateArtifact::new(
                    manifest.source_index.digest.into_bytes(), manifest.source_index.bytes,
                ).map_err(|_| error("invalid inspection paged source-index reference"))?;
                decode_paged_working_state_from(
                    &working,
                    source_index,
                    &observations,
                    binding,
                    limits,
                    |artifact| read_artifact(&config, StoredArtifact {
                        digest: Sha256Digest::new(artifact.digest()),
                        bytes: artifact.bytes(),
                    }),
                ).map_err(|failure| match failure {
                    WorkingStateReadError::Artifact(failure) => failure,
                    WorkingStateReadError::Codec(_) => error("invalid paged inspection working state"),
                })?
            } else {
                decode_working_state_core(&working, &observations, binding, limits)
                    .map_err(|_| error("invalid incremental inspection working state"))?
            };
            (sources, transcript, state)
        } else {
            let sources: Vec<ArchivedObservation> = decode(&read_artifact(&config, manifest.source_index)?)?;
            let transcript: TranscriptManifest = decode(&read_artifact(&config, manifest.transcript_manifest)?)?;
            let state = decode_working_state(
                &read_artifact(&config, manifest.working_state)?, binding, limits,
            ).map_err(|_| error("invalid inspection working state"))?;
            (sources, transcript, state)
        };
        checkpoint_validation::validate_checkpoint(
            manifest.schema_version,
            &state,
            &sources,
            &transcript,
            &validation,
            limits,
        )?;
        let mut archive = Vec::new();
        while let Some(chunk) = view.read_chunk_at(
            u64::try_from(archive.len()).map_err(|_| error("inspection archive offset overflow"))?,
            VIEW_CHUNK_BYTES,
        ).map_err(|_| error("read owned inspection view"))? {
            archive.try_reserve(chunk.bytes().len())
                .map_err(|_| error("inspection view allocation unavailable"))?;
            archive.extend_from_slice(chunk.bytes());
        }
        if u64::try_from(archive.len()).ok() != Some(manifest.view.bytes) {
            return Err(error("inspection archive is incomplete"));
        }
        view_binding::verify(&manifest, &archive, &validation)?;
        let messages = decode_messages(&archive, ProtocolLimits::PRODUCTION)?;
        InspectionBackend::Legacy { sources, messages, view }
    };
    let mut lineage_verified = BTreeSet::new();
    lineage_verified.insert(manifest.generation);
    Ok(LocalContextInspection {
        manifest,
        validation,
        backend,
        journal,
        uncovered_tail,
        lineage_verified,
        complete_verified: false,
    })
}

fn read_artifact(
    config: &StoreConfig,
    artifact: StoredArtifact,
) -> Result<Vec<u8>, DeveloperLoopError> {
    let bytes = ArtifactStore::read_existing(
        config,
        ArtifactDigest::from_sha256(artifact.digest),
        artifact.bytes,
    ).map_err(|_| error("checkpoint artifact missing or corrupt"))?;
    if u64::try_from(bytes.len()).ok() != Some(artifact.bytes) {
        return Err(error("checkpoint artifact size mismatch"));
    }
    Ok(bytes)
}

fn supported_schema(schema: u16) -> bool {
    matches!(
        schema,
        LEGACY_CHECKPOINT_SCHEMA_VERSION
            | SNAPSHOT_CHECKPOINT_SCHEMA_VERSION
            | INDEXED_CHECKPOINT_SCHEMA_VERSION
            | PAGED_CHECKPOINT_SCHEMA_VERSION
            | HOST_INDEX_CHECKPOINT_SCHEMA_VERSION
            | CHECKPOINT_SCHEMA_VERSION
    )
}

fn observation_sources(
    sources: &[ArchivedObservation],
) -> Result<Vec<ObservationSource>, DeveloperLoopError> {
    sources.iter().map(|source| {
        ObservationSource::new(
            ObservationId::new(source.sequence)
                .map_err(|_| error("invalid checkpoint observation sequence"))?,
            source.artifact.digest,
            source.artifact.bytes,
            0,
            source.artifact.bytes,
            source.kind.source_kind(),
        ).map_err(|_| error("invalid checkpoint observation source"))
    }).collect()
}

fn readable_messages(messages: &[Message], archive_location: &str) -> Vec<Value> {
    messages.iter().map(|message| {
        Value::from_iter([
            ("role", Value::from(format!("{:?}", message.role()))),
            ("content", Value::from(message.content().iter().map(|block| match block {
                ContentBlock::Text(text) => Value::from_iter([
                    ("kind", Value::from("text")),
                    ("text", Value::from(text.expose_for_wire())),
                ]),
                ContentBlock::ToolCall(call) => Value::from_iter([
                    ("kind", Value::from("tool_call")),
                    ("id", Value::from(call.id().expose_for_wire())),
                    ("name", Value::from(call.name().as_str())),
                    ("arguments", Value::from(call.arguments().to_wire_string())),
                ]),
                ContentBlock::ToolResult(result) => Value::from_iter([
                    ("kind", Value::from("tool_result")),
                    ("id", Value::from(result.call_id().expose_for_wire())),
                    ("output", Value::from(result.output().to_wire_string())),
                    ("is_error", Value::from(result.is_error())),
                ]),
                _ => Value::from_iter([
                    ("kind", Value::from("non_text")),
                    ("exact_bytes", Value::from(archive_location)),
                ]),
            }).collect::<Vec<_>>())),
        ])
    }).collect()
}
