//! Append-only checkpoint index pages and verified transcript deltas.

use super::super::{
    checkpoint_validation,
    error,
    record::{
        ArchiveKind, ArchivedObservation, CHECKPOINT_SCHEMA_VERSION, HOST_INDEX_SCHEMA_VERSION,
        HostIndexRoot, INDEX_PAGE_SCHEMA_VERSION, MemoryRecord, ObservedFileIndexPage,
        PENDING_EFFECT_REFERENCE_SCHEMA_VERSION, PendingDescriptor, PendingEffectIdentity,
        PendingIndexPage, SourceIndexPage, TranscriptDeltaPage, TranscriptManifest, decode, encode,
        pending_effect_key,
    },
    storage::StoredArtifact,
};
use super::LocalMemory;
use peritus_agent::DeveloperLoopError;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;

// One predecessor plus these observation artifacts fits the 256-child physical bundle fanout.
const SOURCE_PAGE_ENTRIES: usize = 255;
// Schema-one stores already contain pages written before physical dependency binding.
const LEGACY_SOURCE_PAGE_ENTRIES: usize = 256;
const TRANSCRIPT_PAGE_ENTRIES: usize = 256;
// One predecessor plus one exact handle artifact per upsert fits physical bundle fanout.
const PENDING_PAGE_ENTRIES: usize = 255;

struct StoredHostIndex {
    root: StoredArtifact,
    transcript: StoredArtifact,
    pending: StoredArtifact,
    observed_files: StoredArtifact,
    changed: bool,
}

pub(in crate::local_context) fn read_complete_checkpoint_indexes(
    schema_version: u16,
    source: StoredArtifact,
    host_index: StoredArtifact,
    mut read: impl FnMut(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<
    (
        Vec<ArchivedObservation>,
        TranscriptManifest,
        BTreeMap<u64, String>,
    ),
    DeveloperLoopError,
> {
    let sources = read_source_chain(source, None, 0, &mut read)?;
    let (transcript, invocations) = if schema_version == CHECKPOINT_SCHEMA_VERSION {
        read_host_index_chain(host_index, &mut read)?
    } else {
        read_transcript_chain(
            host_index,
            None,
            TranscriptManifest::default(),
            BTreeMap::new(),
            &mut read,
        )?
    };
    validate_transcript_invocation_frontier(&invocations, &transcript)?;
    if sources
        .iter()
        .any(|source| !invocations.contains_key(&source.invocation))
        || sources
            .windows(2)
            .any(|pair| pair[0].invocation > pair[1].invocation)
    {
        return Err(error("source ranges do not match the invocation prefix index"));
    }
    Ok((sources, transcript, invocations))
}

impl LocalMemory {
    pub(super) fn publish_checkpoint_index(
        &mut self,
    ) -> Result<(StoredArtifact, StoredArtifact), DeveloperLoopError> {
        let indexed = usize::try_from(self.indexed_source_count)
            .map_err(|_| error("indexed source count overflow"))?;
        if indexed > self.sources.len() {
            return Err(error("checkpoint source frontier exceeds current archive"));
        }
        let mut source_tail = self.source_index_tail;
        let mut source_changed = false;
        for observations in self.sources[indexed..].chunks(SOURCE_PAGE_ENTRIES) {
            let first_sequence = observations
                .first()
                .map_or(self.indexed_source_count.checked_add(1), |source| {
                    Some(source.sequence)
                })
                .ok_or_else(|| error("source page sequence overflow"))?;
            let page = SourceIndexPage {
                schema_version: INDEX_PAGE_SCHEMA_VERSION,
                previous: source_tail,
                first_sequence,
                observations: observations.to_vec(),
            };
            let mut children = source_tail.into_iter().collect::<Vec<_>>();
            children.extend(observations.iter().map(|observation| observation.artifact));
            let artifact = self.store.store_bundle(&encode(&page)?, &children)?;
            source_tail = Some(artifact);
            source_changed = true;
        }
        if source_tail.is_none() {
            let page = SourceIndexPage {
                schema_version: INDEX_PAGE_SCHEMA_VERSION,
                previous: None,
                first_sequence: 1,
                observations: Vec::new(),
            };
            let artifact = self.store.store_bundle(&encode(&page)?, &[])?;
            source_tail = Some(artifact);
            source_changed = true;
        }

        let stored = self.store_host_index_pages()?;
        let source = source_tail.ok_or_else(|| error("source index tail is missing"))?;
        if source_changed || stored.changed {
            self.commit(
                &MemoryRecord::CheckpointHostIndex { source, root: stored.root },
                &[source.digest, stored.root.digest],
            )?;
        }
        self.source_index_tail = Some(source);
        self.adopt_stored_host_index(&stored);
        self.indexed_source_count = u64::try_from(self.sources.len())
            .map_err(|_| error("indexed source count overflow"))?;
        self.indexed_invocation_count = self.transcript.invocation;
        Ok((source, stored.root))
    }

    pub(super) fn publish_transcript_index(&mut self) -> Result<(), DeveloperLoopError> {
        let stored = self.store_host_index_pages()?;
        if stored.changed {
            self.commit(&MemoryRecord::HostIndex { root: stored.root }, &[stored.root.digest])?;
        }
        self.adopt_stored_host_index(&stored);
        self.indexed_invocation_count = self.transcript.invocation;
        Ok(())
    }

    fn store_host_index_pages(&self) -> Result<StoredHostIndex, DeveloperLoopError> {
        let (transcript, transcript_changed) = self.store_transcript_pages()?;
        let (pending, pending_changed) = self.store_pending_pages()?;
        let (observed_files, files_changed) = self.store_observed_file_pages()?;
        let transcript = transcript.ok_or_else(|| error("transcript index tail is missing"))?;
        let pending = pending.ok_or_else(|| error("pending index tail is missing"))?;
        let observed_files =
            observed_files.ok_or_else(|| error("observed-file index tail is missing"))?;
        let projection = prompt_projection(&self.transcript);
        let root_record = HostIndexRoot {
            schema_version: HOST_INDEX_SCHEMA_VERSION,
            invocation: projection.invocation,
            request_prefix: projection.request_prefix.clone(),
            transcript,
            pending,
            observed_files,
            message_count: u64::try_from(projection.message_ids.len())
                .map_err(|_| error("transcript message count overflow"))?,
            pending_count: u64::try_from(self.transcript.pending.len())
                .map_err(|_| error("pending index count overflow"))?,
            observed_file_count: u64::try_from(self.transcript.files.len())
                .map_err(|_| error("observed-file index count overflow"))?,
        };
        let root = self.store.store_bundle(
            &encode(&root_record)?,
            &[transcript, pending, observed_files],
        )?;
        Ok(StoredHostIndex {
            root,
            transcript,
            pending,
            observed_files,
            changed: transcript_changed
                || pending_changed
                || files_changed
                || self.pending_index_tail.is_none()
                || self.observed_file_index_tail.is_none(),
        })
    }

    fn adopt_stored_host_index(&mut self, stored: &StoredHostIndex) {
        self.transcript_index_tail = Some(stored.transcript);
        self.pending_index_tail = Some(stored.pending);
        self.observed_file_index_tail = Some(stored.observed_files);
        self.indexed_transcript = prompt_projection(&self.transcript);
        self.indexed_pending.clone_from(&self.transcript.pending);
        self.indexed_observed_files.clone_from(&self.transcript.files);
    }

    fn store_transcript_pages(
        &self,
    ) -> Result<(Option<StoredArtifact>, bool), DeveloperLoopError> {
        let mut tail = self.transcript_index_tail;
        let mut current = self.indexed_transcript.clone();
        let mut changed = false;
        validate_invocation_frontier(
            &self.invocations,
            self.indexed_invocation_count,
            &current,
        )?;
        let target = prompt_projection(&self.transcript);
        validate_invocation_frontier(
            &self.invocations,
            target.invocation,
            &target,
        )?;
        for (&invocation, request_prefix) in self.invocations.range((
            std::ops::Bound::Excluded(self.indexed_invocation_count),
            std::ops::Bound::Unbounded,
        )) {
            let mut marker = current.clone();
            marker.invocation = invocation;
            marker.request_prefix.clone_from(request_prefix);
            marker.current_inputs.clear();
            marker.message_ids.clear();
            let page = transcript_delta(tail, &current, &marker)?;
            let children = tail.into_iter().collect::<Vec<_>>();
            let artifact = self.store.store_bundle(&encode(&page)?, &children)?;
            tail = Some(artifact);
            current = marker;
            changed = true;
        }
        while let Some(next) = next_transcript_step(&current, &target)? {
            let page = transcript_delta(tail, &current, &next)?;
            let children = tail.into_iter().collect::<Vec<_>>();
            let artifact = self.store.store_bundle(&encode(&page)?, &children)?;
            tail = Some(artifact);
            current = next;
            changed = true;
        }
        if tail.is_none() {
            let page = transcript_delta(None, &current, &current)?;
            let artifact = self.store.store_bundle(&encode(&page)?, &[])?;
            tail = Some(artifact);
            changed = true;
        }
        Ok((tail, changed))
    }

    fn store_pending_pages(
        &self,
    ) -> Result<(Option<StoredArtifact>, bool), DeveloperLoopError> {
        let mut tail = self.pending_index_tail;
        let mut current = self.indexed_pending.clone();
        let mut changed = false;
        while let Some(next) = next_pending_step(&current, &self.transcript.pending)? {
            let page = pending_delta(tail, &current, &next)?;
            let mut children = tail.into_iter().collect::<Vec<_>>();
            children.extend(
                page.upserts
                    .iter()
                    .filter_map(|pending| pending.handle.as_ref()?.artifact()),
            );
            let artifact = self.store.store_bundle(&encode(&page)?, &children)?;
            tail = Some(artifact);
            current = next;
            changed = true;
        }
        if tail.is_none() {
            let page = pending_delta(None, &current, &current)?;
            tail = Some(self.store.store_bundle(&encode(&page)?, &[])?);
            changed = true;
        }
        Ok((tail, changed))
    }

    fn store_observed_file_pages(
        &self,
    ) -> Result<(Option<StoredArtifact>, bool), DeveloperLoopError> {
        let mut tail = self.observed_file_index_tail;
        let mut current = self.indexed_observed_files.clone();
        let mut changed = false;
        while let Some(next) = next_observed_file_step(&current, &self.transcript.files)? {
            let page = observed_file_delta(tail, &current, &next)?;
            let children = tail.into_iter().collect::<Vec<_>>();
            let artifact = self.store.store_bundle(&encode(&page)?, &children)?;
            tail = Some(artifact);
            current = next;
            changed = true;
        }
        if tail.is_none() {
            let page = observed_file_delta(None, &current, &current)?;
            tail = Some(self.store.store_bundle(&encode(&page)?, &[])?);
            changed = true;
        }
        Ok((tail, changed))
    }

    pub(super) fn restore_checkpoint_indexes(
        &mut self,
        schema_version: u16,
        source: StoredArtifact,
        host_index: StoredArtifact,
    ) -> Result<(), DeveloperLoopError> {
        let (sources, restored_transcript, invocations) = read_complete_checkpoint_indexes(
            schema_version,
            source,
            host_index,
            |artifact| self.store.read(artifact),
        )?;
        let source_count = u64::try_from(sources.len())
            .map_err(|_| error("checkpoint source count overflow"))?;
        self.sources = sources;
        self.transcript = restored_transcript.clone();
        self.source_index_tail = Some(source);
        self.indexed_source_count = source_count;
        if schema_version == CHECKPOINT_SCHEMA_VERSION {
            let root: HostIndexRoot = decode(&self.store.read(host_index)?)?;
            self.transcript_index_tail = Some(root.transcript);
            self.pending_index_tail = Some(root.pending);
            self.observed_file_index_tail = Some(root.observed_files);
            self.indexed_transcript = prompt_projection(&restored_transcript);
            self.indexed_pending.clone_from(&restored_transcript.pending);
            self.indexed_observed_files.clone_from(&restored_transcript.files);
        } else {
            self.transcript_index_tail = Some(host_index);
            self.pending_index_tail = None;
            self.observed_file_index_tail = None;
            self.indexed_transcript = restored_transcript;
            self.indexed_pending.clear();
            self.indexed_observed_files.clear();
        }
        self.invocation_ranges = invocation_ranges(&invocations);
        self.invocations = invocations;
        self.indexed_invocation_count = self.transcript.invocation;
        self.validate_invocation_index()?;
        Ok(())
    }

    pub(super) fn replay_checkpoint_index(
        &mut self,
        source: StoredArtifact,
        transcript: StoredArtifact,
    ) -> Result<(), DeveloperLoopError> {
        if self.pending_index_tail.is_some() || self.observed_file_index_tail.is_some() {
            return Err(error("combined checkpoint index followed a separated host index"));
        }
        let indexed_sources = read_source_chain(
            source,
            self.source_index_tail,
            self.indexed_source_count,
            &mut |artifact| self.store.read(artifact),
        )?;
        let start = usize::try_from(self.indexed_source_count)
            .map_err(|_| error("indexed source count overflow"))?;
        let end = start
            .checked_add(indexed_sources.len())
            .ok_or_else(|| error("indexed source range overflow"))?;
        if self.sources.get(start..end) != Some(indexed_sources.as_slice()) {
            return Err(error("checkpoint source page differs from replayed observations"));
        }
        let (indexed_transcript, indexed_invocation_count) = read_transcript_extension(
            transcript,
            self.transcript_index_tail,
            self.indexed_transcript.clone(),
            self.indexed_invocation_count,
            &self.invocations,
            &mut |artifact| self.store.read(artifact),
        )?;
        if indexed_transcript != self.transcript
            || indexed_invocation_count != self.transcript.invocation
        {
            return Err(error("checkpoint transcript page differs from replayed projection"));
        }
        self.source_index_tail = Some(source);
        self.transcript_index_tail = Some(transcript);
        self.indexed_source_count = end as u64;
        self.indexed_transcript = indexed_transcript;
        self.indexed_invocation_count = indexed_invocation_count;
        Ok(())
    }

    pub(super) fn replay_checkpoint_host_index(
        &mut self,
        source: StoredArtifact,
        root: StoredArtifact,
    ) -> Result<(), DeveloperLoopError> {
        let indexed_sources = read_source_chain(
            source,
            self.source_index_tail,
            self.indexed_source_count,
            &mut |artifact| self.store.read(artifact),
        )?;
        let start = usize::try_from(self.indexed_source_count)
            .map_err(|_| error("indexed source count overflow"))?;
        let end = start
            .checked_add(indexed_sources.len())
            .ok_or_else(|| error("indexed source range overflow"))?;
        if self.sources.get(start..end) != Some(indexed_sources.as_slice()) {
            return Err(error("checkpoint source page differs from replayed observations"));
        }
        self.replay_host_index(root)?;
        self.source_index_tail = Some(source);
        self.indexed_source_count =
            u64::try_from(end).map_err(|_| error("indexed source count overflow"))?;
        Ok(())
    }

    pub(super) fn replay_host_index(
        &mut self,
        root: StoredArtifact,
    ) -> Result<(), DeveloperLoopError> {
        let (mut transcript, pending, files, root_record, indexed_invocation_count) =
            read_host_index_extension(
                root,
                self.transcript_index_tail,
                self.pending_index_tail,
                self.observed_file_index_tail,
                self.indexed_transcript.clone(),
                self.indexed_pending.clone(),
                self.indexed_observed_files.clone(),
                self.indexed_invocation_count,
                &self.invocations,
                &mut |artifact| self.store.read(artifact),
            )?;
        transcript.pending.clone_from(&pending);
        transcript.files.clone_from(&files);
        if transcript != self.transcript || indexed_invocation_count != self.transcript.invocation {
            return Err(error("host index differs from replayed projection"));
        }
        self.transcript_index_tail = Some(root_record.transcript);
        self.pending_index_tail = Some(root_record.pending);
        self.observed_file_index_tail = Some(root_record.observed_files);
        self.indexed_transcript = prompt_projection(&transcript);
        self.indexed_pending = pending;
        self.indexed_observed_files = files;
        self.indexed_invocation_count = indexed_invocation_count;
        Ok(())
    }

    pub(super) fn replay_transcript_index(
        &mut self,
        transcript: StoredArtifact,
    ) -> Result<(), DeveloperLoopError> {
        if self.pending_index_tail.is_some() || self.observed_file_index_tail.is_some() {
            return Err(error("combined transcript index followed a separated host index"));
        }
        let (restored, indexed_invocation_count) = read_transcript_extension(
            transcript,
            self.transcript_index_tail,
            self.indexed_transcript.clone(),
            self.indexed_invocation_count,
            &self.invocations,
            &mut |artifact| self.store.read(artifact),
        )?;
        if restored.invocation != self.transcript.invocation
            || restored.message_ids != self.transcript.message_ids
            || indexed_invocation_count != self.transcript.invocation
        {
            return Err(error(
                "transcript index does not match replayed invocation or message history",
            ));
        }
        checkpoint_validation::validate_transcript(
            &self.state,
            &self.sources,
            &restored,
            restored.invocation,
            self.limits,
        )?;
        self.transcript = restored.clone();
        self.transcript_index_tail = Some(transcript);
        self.indexed_transcript = restored;
        self.indexed_invocation_count = indexed_invocation_count;
        Ok(())
    }

    pub(super) fn validate_next_invocation(
        &self,
        invocation: u64,
    ) -> Result<(), DeveloperLoopError> {
        validate_next_invocation(&self.invocations, invocation)
    }

    pub(super) fn adopt_invocation(
        &mut self,
        invocation: u64,
        request_prefix: String,
    ) -> Result<(), DeveloperLoopError> {
        validate_next_invocation(&self.invocations, invocation)?;
        if let Some((logical_prefix, _)) = split_request_prefix(&request_prefix) {
            extend_invocation_ranges(
                &mut self.invocation_ranges,
                logical_prefix.to_owned(),
                invocation,
            );
        }
        self.invocations.insert(invocation, request_prefix);
        Ok(())
    }

    pub(super) fn validate_invocation_index(&self) -> Result<(), DeveloperLoopError> {
        validate_transcript_invocation_frontier(&self.invocations, &self.transcript)?;
        if self.invocation_ranges != invocation_ranges(&self.invocations)
            || self
            .sources
            .iter()
            .any(|source| !self.invocations.contains_key(&source.invocation))
            || self
                .sources
                .windows(2)
                .any(|pair| pair[0].invocation > pair[1].invocation)
        {
            return Err(error("invocation prefix index high-water mismatch"));
        }
        Ok(())
    }

    pub(in crate::local_context) fn grounding_observations_for_prefix(
        &self,
        logical_prefix: &str,
    ) -> Result<Vec<ArchivedObservation>, DeveloperLoopError> {
        self.validate_live_invocation_frontier()?;
        let Some(ranges) = self.invocation_ranges.get(logical_prefix) else {
            return Ok(Vec::new());
        };
        if ranges.windows(2).any(|pair| pair[0].1 >= pair[1].0)
            || ranges.iter().any(|&(first, last)| {
                first > last
                    || [first, last].into_iter().any(|invocation| {
                        self.invocations
                            .get(&invocation)
                            .and_then(|prefix| split_request_prefix(prefix))
                            .is_none_or(|(prefix, _)| prefix != logical_prefix)
                    })
            })
        {
            return Err(error("logical invocation range index is inconsistent"));
        }
        let mut observations = Vec::new();
        for &(first, last) in ranges {
            let start = self
                .sources
                .partition_point(|source| source.invocation < first);
            let end = self
                .sources
                .partition_point(|source| source.invocation <= last);
            observations.extend(
                self.sources[start..end]
                    .iter()
                    .filter(|source| {
                        matches!(source.kind, ArchiveKind::Assistant | ArchiveKind::ToolOutput)
                    })
                    .cloned(),
            );
        }
        Ok(observations)
    }

    pub(in crate::local_context) fn grounding_prefixes_for_scope(
        &self,
        logical_scope: &str,
        expected_revision: u64,
    ) -> Result<Vec<String>, DeveloperLoopError> {
        self.validate_live_invocation_frontier()?;
        let mut prefixes = Vec::new();
        for (prefix, ranges) in self
            .invocation_ranges
            .iter()
            .filter(|(prefix, _)| prefix.starts_with(logical_scope))
        {
            let Some((cycle, revision)) = prefix
                .strip_prefix(logical_scope)
                .and_then(|suffix| suffix.split_once("-revision-"))
                .and_then(|(cycle, suffix)| {
                    suffix.strip_suffix("-invocation-").map(|revision| (cycle, revision))
                })
            else {
                return Err(error("grounding scope contains a malformed invocation prefix"));
            };
            if cycle.is_empty()
                || (cycle != "0" && cycle.starts_with('0'))
                || !cycle.bytes().all(|byte| byte.is_ascii_digit())
                || cycle.parse::<u32>().is_err()
            {
                return Err(error("grounding scope contains an invalid cycle"));
            }
            if revision.is_empty()
                || (revision != "0" && revision.starts_with('0'))
                || !revision.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(error("grounding scope contains an invalid revision"));
            }
            let revision = revision
                .parse::<u64>()
                .map_err(|_| error("grounding scope contains an invalid revision"))?;
            if revision != expected_revision {
                continue;
            }
            let first = ranges
                .first()
                .map(|range| range.0)
                .ok_or_else(|| error("grounding scope contains an empty invocation range"))?;
            prefixes.push((first, prefix.clone()));
        }
        prefixes.sort_by_key(|(first, _)| *first);
        Ok(prefixes.into_iter().map(|(_, prefix)| prefix).collect())
    }

    fn validate_live_invocation_frontier(&self) -> Result<(), DeveloperLoopError> {
        let invocation_count = u64::try_from(self.invocations.len())
            .map_err(|_| error("invocation index size overflow"))?;
        if invocation_count != self.transcript.invocation {
            return Err(error("live invocation index high-water mismatch"));
        }
        validate_invocation_frontier(&self.invocations, invocation_count, &self.transcript)?;
        let source_count = u64::try_from(self.sources.len())
            .map_err(|_| error("source index size overflow"))?;
        if let (Some(first), Some(last)) = (self.sources.first(), self.sources.last())
            && (first.sequence != 1
                || last.sequence != source_count
                || first.invocation == 0
                || last.invocation > invocation_count)
        {
            return Err(error("source index high-water does not match invocation history"));
        }
        Ok(())
    }
}

fn validate_next_invocation(
    invocations: &BTreeMap<u64, String>,
    invocation: u64,
) -> Result<(), DeveloperLoopError> {
    let expected = u64::try_from(invocations.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or_else(|| error("invocation index sequence overflow"))?;
    if invocation != expected {
        return Err(error("invocation prefix index is not contiguous"));
    }
    Ok(())
}

fn extend_invocation_ranges(
    ranges: &mut BTreeMap<String, Vec<(u64, u64)>>,
    logical_prefix: String,
    invocation: u64,
) {
    let ranges = ranges.entry(logical_prefix).or_default();
    if let Some((_, last)) = ranges.last_mut()
        && last.checked_add(1) == Some(invocation)
    {
        *last = invocation;
    } else {
        ranges.push((invocation, invocation));
    }
}

fn invocation_ranges(
    invocations: &BTreeMap<u64, String>,
) -> BTreeMap<String, Vec<(u64, u64)>> {
    let mut ranges = BTreeMap::new();
    for (&invocation, request_prefix) in invocations {
        if let Some((logical_prefix, _)) = split_request_prefix(request_prefix) {
            extend_invocation_ranges(&mut ranges, logical_prefix.to_owned(), invocation);
        }
    }
    ranges
}

fn observe_invocation(
    invocations: &mut BTreeMap<u64, String>,
    invocation: u64,
    request_prefix: &str,
) -> Result<(), DeveloperLoopError> {
    if let Some(existing) = invocations.get(&invocation) {
        return if existing == request_prefix {
            Ok(())
        } else {
            Err(error("invocation prefix index contains a conflicting identity"))
        };
    }
    validate_next_invocation(invocations, invocation)?;
    invocations.insert(invocation, request_prefix.to_owned());
    Ok(())
}

fn validate_invocation_frontier(
    invocations: &BTreeMap<u64, String>,
    count: u64,
    transcript: &TranscriptManifest,
) -> Result<(), DeveloperLoopError> {
    let available = u64::try_from(invocations.len())
        .map_err(|_| error("invocation index size overflow"))?;
    if count > available
        || transcript.invocation != count
        || (count == 0 && !transcript.request_prefix.is_empty())
        || (count != 0 && invocations.get(&count) != Some(&transcript.request_prefix))
    {
        return Err(error("transcript invocation frontier is not indexed"));
    }
    Ok(())
}

fn validate_transcript_invocation_frontier(
    invocations: &BTreeMap<u64, String>,
    transcript: &TranscriptManifest,
) -> Result<(), DeveloperLoopError> {
    validate_invocation_sequence(invocations)?;
    let count = u64::try_from(invocations.len())
        .map_err(|_| error("invocation index size overflow"))?;
    if count != transcript.invocation
        || (count == 0 && !transcript.request_prefix.is_empty())
        || (count != 0 && invocations.get(&count) != Some(&transcript.request_prefix))
    {
        return Err(error("transcript invocation frontier does not match its prefix index"));
    }
    Ok(())
}

fn validate_invocation_sequence(
    invocations: &BTreeMap<u64, String>,
) -> Result<(), DeveloperLoopError> {
    if invocations.keys().copied().enumerate().any(|(index, invocation)| {
        u64::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            != Some(invocation)
    }) {
        return Err(error("invocation prefix index is not contiguous"));
    }
    Ok(())
}

fn split_request_prefix(request_prefix: &str) -> Option<(&str, u64)> {
    let (prefix_and_sequence, nonce) = request_prefix.rsplit_once('-')?;
    if nonce.len() != 32
        || !nonce.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let sequence_start = prefix_and_sequence
        .bytes()
        .rposition(|byte| !byte.is_ascii_digit())
        .map_or(0, |index| index + 1);
    if sequence_start == prefix_and_sequence.len() {
        return None;
    }
    let (logical_prefix, sequence) = prefix_and_sequence.split_at(sequence_start);
    if !logical_prefix.ends_with('-') {
        return None;
    }
    let Ok(parsed) = sequence.parse::<u64>() else {
        return None;
    };
    (sequence == "0" || !sequence.starts_with('0')).then_some((logical_prefix, parsed))
}

fn next_transcript_step(
    before: &TranscriptManifest,
    after: &TranscriptManifest,
) -> Result<Option<TranscriptManifest>, DeveloperLoopError> {
    if before == after {
        return Ok(None);
    }
    let mut next = before.clone();
    if next.invocation != after.invocation
        || next.request_prefix != after.request_prefix
        || next.current_inputs != after.current_inputs
        || next.facts_through != after.facts_through
    {
        if next.invocation != after.invocation {
            next.message_ids.clear();
        }
        next.invocation = after.invocation;
        next.request_prefix.clone_from(&after.request_prefix);
        next.current_inputs.clone_from(&after.current_inputs);
        next.facts_through = after.facts_through;
        return Ok(Some(next));
    }
    if !after.message_ids.starts_with(&next.message_ids) {
        next.message_ids.clear();
        return Ok(Some(next));
    }
    if next.message_ids.len() < after.message_ids.len() {
        let start = next.message_ids.len();
        let end = next
            .message_ids
            .len()
            .saturating_add(TRANSCRIPT_PAGE_ENTRIES)
            .min(after.message_ids.len());
        next.message_ids.extend_from_slice(&after.message_ids[start..end]);
        return Ok(Some(next));
    }

    let after_pending = after
        .pending
        .iter()
        .map(|value| (value.key, value))
        .collect::<BTreeMap<_, _>>();
    let removals = next
        .pending
        .iter()
        .filter(|value| !after_pending.contains_key(&value.key))
        .map(|value| value.key)
        .take(TRANSCRIPT_PAGE_ENTRIES)
        .collect::<Vec<_>>();
    if !removals.is_empty() {
        next.pending.retain(|value| !removals.contains(&value.key));
        return Ok(Some(next));
    }
    let current_pending = next
        .pending
        .iter()
        .map(|value| (value.key, value))
        .collect::<BTreeMap<_, _>>();
    let upserts = after
        .pending
        .iter()
        .filter(|value| current_pending.get(&value.key).is_none_or(|old| *old != *value))
        .cloned()
        .take(TRANSCRIPT_PAGE_ENTRIES)
        .collect::<Vec<_>>();
    if !upserts.is_empty() {
        let replacements = upserts
            .iter()
            .map(|value| (value.key, value.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut pending = next
            .pending
            .iter()
            .filter(|value| !replacements.contains_key(&value.key))
            .cloned()
            .collect::<Vec<_>>();
        pending.extend(replacements.into_values());
        pending.sort_by_key(|value| value.key);
        next.pending = pending;
        return Ok(Some(next));
    }

    let after_files = after.files.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let removals = next
        .files
        .iter()
        .filter(|path| !after_files.contains(path.as_str()))
        .cloned()
        .take(TRANSCRIPT_PAGE_ENTRIES)
        .collect::<Vec<_>>();
    if !removals.is_empty() {
        next.files.retain(|path| !removals.contains(path));
        return Ok(Some(next));
    }
    let current_files = next.files.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let additions = after
        .files
        .iter()
        .filter(|path| !current_files.contains(path.as_str()))
        .cloned()
        .take(TRANSCRIPT_PAGE_ENTRIES)
        .collect::<Vec<_>>();
    if !additions.is_empty() {
        next.files.extend(additions);
        next.files.sort();
        return Ok(Some(next));
    }
    Err(error("transcript page plan cannot reach the exact projection"))
}

fn prompt_projection(transcript: &TranscriptManifest) -> TranscriptManifest {
    let mut projection = transcript.clone();
    projection.pending.clear();
    projection.files.clear();
    projection
}

fn next_pending_step(
    before: &[PendingDescriptor],
    after: &[PendingDescriptor],
) -> Result<Option<Vec<PendingDescriptor>>, DeveloperLoopError> {
    if before == after {
        return Ok(None);
    }
    let after_index = after
        .iter()
        .map(|value| (value.key, value))
        .collect::<BTreeMap<_, _>>();
    let removals = before
        .iter()
        .filter(|value| !after_index.contains_key(&value.key))
        .map(|value| value.key)
        .take(PENDING_PAGE_ENTRIES)
        .collect::<Vec<_>>();
    if !removals.is_empty() {
        return Ok(Some(
            before
                .iter()
                .filter(|value| !removals.contains(&value.key))
                .cloned()
                .collect(),
        ));
    }
    let before_index = before
        .iter()
        .map(|value| (value.key, value))
        .collect::<BTreeMap<_, _>>();
    let upserts = after
        .iter()
        .filter(|value| before_index.get(&value.key).is_none_or(|old| *old != *value))
        .cloned()
        .take(PENDING_PAGE_ENTRIES)
        .collect::<Vec<_>>();
    if upserts.is_empty() {
        return Err(error("pending page plan cannot reach the exact index"));
    }
    let replacements = upserts
        .iter()
        .map(|value| (value.key, value.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut next = before
        .iter()
        .filter(|value| !replacements.contains_key(&value.key))
        .cloned()
        .collect::<Vec<_>>();
    next.extend(replacements.into_values());
    next.sort_by_key(|value| value.key);
    Ok(Some(next))
}

fn pending_delta(
    previous: Option<StoredArtifact>,
    before: &[PendingDescriptor],
    after: &[PendingDescriptor],
) -> Result<PendingIndexPage, DeveloperLoopError> {
    let old = before.iter().map(|value| (value.key, value)).collect::<BTreeMap<_, _>>();
    let new = after.iter().map(|value| (value.key, value)).collect::<BTreeMap<_, _>>();
    Ok(PendingIndexPage {
        schema_version: INDEX_PAGE_SCHEMA_VERSION,
        previous,
        before: index_digest(before, "hash pending index")?,
        after: index_digest(after, "hash pending index")?,
        upserts: new
            .iter()
            .filter(|(key, value)| old.get(key).is_none_or(|prior| *prior != *value))
            .map(|(_, value)| (*value).clone())
            .collect(),
        removed: old.keys().filter(|key| !new.contains_key(key)).copied().collect(),
    })
}

fn next_observed_file_step(
    before: &[String],
    after: &[String],
) -> Result<Option<Vec<String>>, DeveloperLoopError> {
    if before == after {
        return Ok(None);
    }
    let after_set = after.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let removals = before
        .iter()
        .filter(|path| !after_set.contains(path.as_str()))
        .cloned()
        .take(TRANSCRIPT_PAGE_ENTRIES)
        .collect::<Vec<_>>();
    if !removals.is_empty() {
        return Ok(Some(
            before.iter().filter(|path| !removals.contains(path)).cloned().collect(),
        ));
    }
    let before_set = before.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let additions = after
        .iter()
        .filter(|path| !before_set.contains(path.as_str()))
        .cloned()
        .take(TRANSCRIPT_PAGE_ENTRIES)
        .collect::<Vec<_>>();
    if additions.is_empty() {
        return Err(error("observed-file page plan cannot reach the exact index"));
    }
    let mut next = before.to_vec();
    next.extend(additions);
    next.sort();
    Ok(Some(next))
}

fn observed_file_delta(
    previous: Option<StoredArtifact>,
    before: &[String],
    after: &[String],
) -> Result<ObservedFileIndexPage, DeveloperLoopError> {
    let old = before.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let new = after.iter().map(String::as_str).collect::<BTreeSet<_>>();
    Ok(ObservedFileIndexPage {
        schema_version: INDEX_PAGE_SCHEMA_VERSION,
        previous,
        before: index_digest(before, "hash observed-file index")?,
        after: index_digest(after, "hash observed-file index")?,
        added: new.difference(&old).map(|value| (*value).to_owned()).collect(),
        removed: old.difference(&new).map(|value| (*value).to_owned()).collect(),
    })
}

fn read_host_index_chain(
    root_artifact: StoredArtifact,
    read: &mut impl FnMut(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<(TranscriptManifest, BTreeMap<u64, String>), DeveloperLoopError> {
    let root: HostIndexRoot = decode(&read(root_artifact)?)?;
    let (mut transcript, invocations) = read_transcript_chain(
        root.transcript,
        None,
        TranscriptManifest::default(),
        BTreeMap::new(),
        read,
    )?;
    if !transcript.pending.is_empty() || !transcript.files.is_empty() {
        return Err(error("separated prompt transcript contains host indexes"));
    }
    let pending = read_pending_chain(root.pending, None, Vec::new(), read)?;
    let files = read_observed_file_chain(root.observed_files, None, Vec::new(), read)?;
    validate_host_index_root(&root, &transcript, &pending, &files)?;
    transcript.pending = pending;
    transcript.files = files;
    Ok((transcript, invocations))
}

#[allow(clippy::too_many_arguments)]
fn read_host_index_extension(
    root_artifact: StoredArtifact,
    transcript_stop: Option<StoredArtifact>,
    pending_stop: Option<StoredArtifact>,
    files_stop: Option<StoredArtifact>,
    transcript: TranscriptManifest,
    pending: Vec<PendingDescriptor>,
    files: Vec<String>,
    invocation_count: u64,
    invocations: &BTreeMap<u64, String>,
    read: &mut impl FnMut(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<
    (TranscriptManifest, Vec<PendingDescriptor>, Vec<String>, HostIndexRoot, u64),
    DeveloperLoopError,
> {
    let root: HostIndexRoot = decode(&read(root_artifact)?)?;
    let (transcript, invocation_count) = read_transcript_extension(
        root.transcript,
        transcript_stop,
        transcript,
        invocation_count,
        invocations,
        read,
    )?;
    if !transcript.pending.is_empty() || !transcript.files.is_empty() {
        return Err(error("separated prompt transcript contains host indexes"));
    }
    let pending = read_pending_chain(root.pending, pending_stop, pending, read)?;
    let files = read_observed_file_chain(root.observed_files, files_stop, files, read)?;
    validate_host_index_root(&root, &transcript, &pending, &files)?;
    Ok((transcript, pending, files, root, invocation_count))
}

fn validate_host_index_root(
    root: &HostIndexRoot,
    transcript: &TranscriptManifest,
    pending: &[PendingDescriptor],
    files: &[String],
) -> Result<(), DeveloperLoopError> {
    let messages = u64::try_from(transcript.message_ids.len())
        .map_err(|_| error("transcript message count overflow"))?;
    let pending_count =
        u64::try_from(pending.len()).map_err(|_| error("pending index count overflow"))?;
    let file_count =
        u64::try_from(files.len()).map_err(|_| error("observed-file index count overflow"))?;
    if root.schema_version != HOST_INDEX_SCHEMA_VERSION
        || root.invocation != transcript.invocation
        || root.request_prefix != transcript.request_prefix
        || root.message_count != messages
        || root.pending_count != pending_count
        || root.observed_file_count != file_count
    {
        return Err(error("host index root identity or count mismatch"));
    }
    Ok(())
}

fn read_pending_chain(
    tail: StoredArtifact,
    stop: Option<StoredArtifact>,
    mut pending: Vec<PendingDescriptor>,
    read: &mut impl FnMut(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<Vec<PendingDescriptor>, DeveloperLoopError> {
    let mut pages = Vec::new();
    let mut current = Some(tail);
    let mut seen = BTreeSet::new();
    while current != stop {
        let artifact = current.ok_or_else(|| error("pending page chain is truncated"))?;
        if !seen.insert(artifact.digest) {
            return Err(error("pending page chain contains a cycle"));
        }
        let page: PendingIndexPage = decode(&read(artifact)?)?;
        if page.schema_version != INDEX_PAGE_SCHEMA_VERSION
            || page.upserts.len() > TRANSCRIPT_PAGE_ENTRIES
            || page.removed.len() > TRANSCRIPT_PAGE_ENTRIES
            || (page.before == page.after && page.previous.is_some())
        {
            return Err(error("unsupported pending page schema"));
        }
        for pending in &page.upserts {
            if let Some(PendingEffectIdentity::Reference(reference)) = &pending.handle {
                if reference.schema_version != PENDING_EFFECT_REFERENCE_SCHEMA_VERSION
                    || reference.artifact.bytes == 0
                    || pending_effect_key(*reference) != pending.key
                {
                    return Err(error("invalid pending effect artifact reference"));
                }
                let handle = read(reference.artifact)?;
                if std::str::from_utf8(&handle).map_or(true, str::is_empty) {
                    return Err(error("pending effect artifact is not a complete handle"));
                }
            }
        }
        current = page.previous;
        pages.push(page);
    }
    pages.reverse();
    for page in pages {
        apply_pending_page(&mut pending, &page)?;
    }
    Ok(pending)
}

fn apply_pending_page(
    pending: &mut Vec<PendingDescriptor>,
    page: &PendingIndexPage,
) -> Result<(), DeveloperLoopError> {
    if index_digest(pending, "hash pending index")? != page.before
        || page.removed.windows(2).any(|pair| pair[0] >= pair[1])
        || page.upserts.windows(2).any(|pair| pair[0].key >= pair[1].key)
        || page
            .removed
            .iter()
            .any(|key| page.upserts.iter().any(|value| value.key == *key))
    {
        return Err(error("pending page predecessor or canonical order mismatch"));
    }
    let mut index = pending
        .iter()
        .cloned()
        .map(|value| (value.key, value))
        .collect::<BTreeMap<_, _>>();
    for key in &page.removed {
        if index.remove(key).is_none() {
            return Err(error("pending page removes an absent operation"));
        }
    }
    for value in &page.upserts {
        if index.get(&value.key) == Some(value) {
            return Err(error("pending page repeats an unchanged operation"));
        }
        index.insert(value.key, value.clone());
    }
    *pending = index.into_values().collect();
    if index_digest(pending, "hash pending index")? != page.after {
        return Err(error("pending page does not reach its committed identity"));
    }
    Ok(())
}

fn read_observed_file_chain(
    tail: StoredArtifact,
    stop: Option<StoredArtifact>,
    mut files: Vec<String>,
    read: &mut impl FnMut(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<Vec<String>, DeveloperLoopError> {
    let mut pages = Vec::new();
    let mut current = Some(tail);
    let mut seen = BTreeSet::new();
    while current != stop {
        let artifact = current.ok_or_else(|| error("observed-file page chain is truncated"))?;
        if !seen.insert(artifact.digest) {
            return Err(error("observed-file page chain contains a cycle"));
        }
        let page: ObservedFileIndexPage = decode(&read(artifact)?)?;
        if page.schema_version != INDEX_PAGE_SCHEMA_VERSION
            || page.added.len() > TRANSCRIPT_PAGE_ENTRIES
            || page.removed.len() > TRANSCRIPT_PAGE_ENTRIES
            || (page.before == page.after && page.previous.is_some())
        {
            return Err(error("unsupported observed-file page schema"));
        }
        current = page.previous;
        pages.push(page);
    }
    pages.reverse();
    for page in pages {
        apply_observed_file_page(&mut files, &page)?;
    }
    Ok(files)
}

fn apply_observed_file_page(
    files: &mut Vec<String>,
    page: &ObservedFileIndexPage,
) -> Result<(), DeveloperLoopError> {
    if index_digest(files, "hash observed-file index")? != page.before
        || page.added.windows(2).any(|pair| pair[0] >= pair[1])
        || page.removed.windows(2).any(|pair| pair[0] >= pair[1])
        || page.removed.iter().any(|path| page.added.binary_search(path).is_ok())
    {
        return Err(error("observed-file page predecessor or canonical order mismatch"));
    }
    let mut index = files.iter().cloned().collect::<BTreeSet<_>>();
    for path in &page.removed {
        if !index.remove(path) {
            return Err(error("observed-file page removes an absent path"));
        }
    }
    for path in &page.added {
        if path.is_empty() || !index.insert(path.clone()) {
            return Err(error("observed-file page repeats an invalid path"));
        }
    }
    *files = index.into_iter().collect();
    if index_digest(files, "hash observed-file index")? != page.after {
        return Err(error("observed-file page does not reach its committed identity"));
    }
    Ok(())
}

fn index_digest<T: serde::Serialize + ?Sized>(
    value: &T,
    operation: &'static str,
) -> Result<[u8; 32], DeveloperLoopError> {
    struct DigestWriter(Sha256);
    impl std::io::Write for DigestWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = DigestWriter(Sha256::new());
    serde_json::to_writer(&mut writer, value).map_err(|_| error(operation))?;
    writer.flush().map_err(|_| error(operation))?;
    Ok(writer.0.finalize().into())
}

fn read_source_chain(
    tail: StoredArtifact,
    stop: Option<StoredArtifact>,
    base_count: u64,
    read: &mut impl FnMut(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<Vec<super::super::record::ArchivedObservation>, DeveloperLoopError> {
    let mut pages = Vec::new();
    let mut current = Some(tail);
    let mut seen = BTreeSet::new();
    while current != stop {
        let artifact = current.ok_or_else(|| error("source page chain is truncated"))?;
        if !seen.insert(artifact.digest) {
            return Err(error("source page chain contains a cycle"));
        }
        let page: SourceIndexPage = decode(&read(artifact)?)?;
        if page.schema_version != INDEX_PAGE_SCHEMA_VERSION
            || page.observations.len() > LEGACY_SOURCE_PAGE_ENTRIES
            || (page.observations.is_empty() && page.previous.is_some())
        {
            return Err(error("unsupported source page schema"));
        }
        current = page.previous;
        pages.push(page);
    }
    pages.reverse();
    let mut sources = Vec::new();
    for page in pages {
        let offset = u64::try_from(sources.len())
            .map_err(|_| error("source page offset overflow"))?;
        let expected = base_count
            .checked_add(offset)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| error("source page sequence overflow"))?;
        if page.first_sequence != expected
            || page.observations.iter().enumerate().any(|(index, source)| {
                u64::try_from(index)
                    .ok()
                    .and_then(|index| expected.checked_add(index))
                    != Some(source.sequence)
            })
        {
            return Err(error("source page sequence is not contiguous"));
        }
        sources.extend(page.observations);
    }
    Ok(sources)
}

fn read_transcript_chain(
    tail: StoredArtifact,
    stop: Option<StoredArtifact>,
    mut transcript: TranscriptManifest,
    mut invocations: BTreeMap<u64, String>,
    read: &mut impl FnMut(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<(TranscriptManifest, BTreeMap<u64, String>), DeveloperLoopError> {
    let pages = read_transcript_pages(tail, stop, read)?;
    for page in pages {
        if page.invocation != 0 {
            observe_invocation(&mut invocations, page.invocation, &page.request_prefix)?;
        }
        apply_transcript_delta(&mut transcript, &page)?;
    }
    Ok((transcript, invocations))
}

fn read_transcript_extension(
    tail: StoredArtifact,
    stop: Option<StoredArtifact>,
    mut transcript: TranscriptManifest,
    mut invocation_count: u64,
    invocations: &BTreeMap<u64, String>,
    read: &mut impl FnMut(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<(TranscriptManifest, u64), DeveloperLoopError> {
    validate_invocation_frontier(invocations, invocation_count, &transcript)?;
    let pages = read_transcript_pages(tail, stop, read)?;
    for page in pages {
        if page.invocation == invocation_count {
            if invocation_count != 0
                && invocations.get(&invocation_count) != Some(&page.request_prefix)
            {
                return Err(error("transcript page invocation identity conflicts with its index"));
            }
        } else if invocation_count.checked_add(1) == Some(page.invocation)
            && invocations.get(&page.invocation) == Some(&page.request_prefix)
        {
            invocation_count = page.invocation;
        } else {
            return Err(error("transcript page does not extend the invocation frontier"));
        }
        apply_transcript_delta(&mut transcript, &page)?;
    }
    validate_invocation_frontier(invocations, invocation_count, &transcript)?;
    Ok((transcript, invocation_count))
}

fn read_transcript_pages(
    tail: StoredArtifact,
    stop: Option<StoredArtifact>,
    read: &mut impl FnMut(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<Vec<TranscriptDeltaPage>, DeveloperLoopError> {
    let mut pages = Vec::new();
    let mut current = Some(tail);
    let mut seen = BTreeSet::new();
    while current != stop {
        let artifact = current.ok_or_else(|| error("transcript page chain is truncated"))?;
        if !seen.insert(artifact.digest) {
            return Err(error("transcript page chain contains a cycle"));
        }
        let page: TranscriptDeltaPage = decode(&read(artifact)?)?;
        if page.schema_version != INDEX_PAGE_SCHEMA_VERSION
            || page.messages.len() > TRANSCRIPT_PAGE_ENTRIES
            || page.pending_upserts.len() > TRANSCRIPT_PAGE_ENTRIES
            || page.pending_removed.len() > TRANSCRIPT_PAGE_ENTRIES
            || page.files_added.len() > TRANSCRIPT_PAGE_ENTRIES
            || page.files_removed.len() > TRANSCRIPT_PAGE_ENTRIES
            || (page.before == page.after && page.previous.is_some())
        {
            return Err(error("unsupported transcript page schema"));
        }
        current = page.previous;
        pages.push(page);
    }
    pages.reverse();
    Ok(pages)
}

fn transcript_delta(
    previous: Option<StoredArtifact>,
    before: &TranscriptManifest,
    after: &TranscriptManifest,
) -> Result<TranscriptDeltaPage, DeveloperLoopError> {
    let reset_messages = before.invocation != after.invocation
        || !after.message_ids.starts_with(&before.message_ids);
    let messages = if reset_messages {
        after.message_ids.clone()
    } else {
        after.message_ids[before.message_ids.len()..].to_vec()
    };
    let old_pending = before.pending.iter().map(|value| (value.key, value)).collect::<BTreeMap<_, _>>();
    let new_pending = after.pending.iter().map(|value| (value.key, value)).collect::<BTreeMap<_, _>>();
    let pending_removed = old_pending
        .keys()
        .filter(|key| !new_pending.contains_key(key))
        .copied()
        .collect();
    let pending_upserts = new_pending
        .iter()
        .filter(|(key, value)| old_pending.get(key).is_none_or(|old| *old != *value))
        .map(|(_, value)| (*value).clone())
        .collect();
    let old_files = before.files.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let new_files = after.files.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let files_added = new_files.difference(&old_files).map(|value| (*value).to_owned()).collect();
    let files_removed = old_files.difference(&new_files).map(|value| (*value).to_owned()).collect();
    Ok(TranscriptDeltaPage {
        schema_version: INDEX_PAGE_SCHEMA_VERSION,
        previous,
        before: transcript_digest(before)?,
        after: transcript_digest(after)?,
        invocation: after.invocation,
        request_prefix: after.request_prefix.clone(),
        reset_messages,
        messages,
        current_inputs: after.current_inputs.clone(),
        pending_upserts,
        pending_removed,
        files_added,
        files_removed,
        facts_through: after.facts_through,
    })
}

fn apply_transcript_delta(
    transcript: &mut TranscriptManifest,
    page: &TranscriptDeltaPage,
) -> Result<(), DeveloperLoopError> {
    if transcript_digest(transcript)? != page.before
        || page.pending_removed.windows(2).any(|pair| pair[0] >= pair[1])
        || page.pending_upserts.windows(2).any(|pair| pair[0].key >= pair[1].key)
        || page.files_added.windows(2).any(|pair| pair[0] >= pair[1])
        || page.files_removed.windows(2).any(|pair| pair[0] >= pair[1])
        || page
            .pending_removed
            .iter()
            .any(|key| page.pending_upserts.iter().any(|value| value.key == *key))
        || page.files_removed.iter().any(|path| page.files_added.contains(path))
    {
        return Err(error("transcript delta predecessor or canonical order mismatch"));
    }
    if page.reset_messages {
        transcript.message_ids.clear();
    } else if transcript.invocation != page.invocation {
        return Err(error("transcript delta invocation changed without reset"));
    }
    transcript.invocation = page.invocation;
    transcript.request_prefix.clone_from(&page.request_prefix);
    for message in &page.messages {
        if transcript.message_ids.last().is_some_and(|prior| prior >= message) {
            return Err(error("transcript message delta is not contiguous"));
        }
        transcript.message_ids.push(*message);
    }
    transcript.current_inputs.clone_from(&page.current_inputs);
    let mut pending = transcript
        .pending
        .iter()
        .cloned()
        .map(|value| (value.key, value))
        .collect::<BTreeMap<_, _>>();
    for key in &page.pending_removed {
        if pending.remove(key).is_none() {
            return Err(error("transcript delta removes an absent pending operation"));
        }
    }
    for value in &page.pending_upserts {
        if pending.get(&value.key) == Some(value) {
            return Err(error("transcript delta repeats an unchanged pending operation"));
        }
        pending.insert(value.key, value.clone());
    }
    transcript.pending = pending.into_values().collect::<Vec<PendingDescriptor>>();
    let mut files = transcript.files.iter().cloned().collect::<BTreeSet<_>>();
    for path in &page.files_removed {
        if !files.remove(path) {
            return Err(error("transcript delta removes an absent observed file"));
        }
    }
    for path in &page.files_added {
        if !files.insert(path.clone()) {
            return Err(error("transcript delta repeats an observed file"));
        }
    }
    transcript.files = files.into_iter().collect();
    transcript.facts_through = page.facts_through;
    if transcript_digest(transcript)? != page.after {
        return Err(error("transcript delta does not reach its committed identity"));
    }
    Ok(())
}

fn transcript_digest(
    transcript: &TranscriptManifest,
) -> Result<[u8; 32], DeveloperLoopError> {
    struct DigestWriter(Sha256);
    impl std::io::Write for DigestWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = DigestWriter(Sha256::new());
    serde_json::to_writer(&mut writer, transcript)
        .map_err(|_| error("hash transcript projection"))?;
    writer.flush().map_err(|_| error("hash transcript projection"))?;
    Ok(writer.0.finalize().into())
}
