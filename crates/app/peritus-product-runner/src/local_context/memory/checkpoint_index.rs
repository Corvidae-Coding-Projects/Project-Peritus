//! Append-only checkpoint index pages and verified transcript deltas.

use super::super::{
    checkpoint_validation,
    error,
    record::{
        ArchiveKind, ArchivedObservation, INDEX_PAGE_SCHEMA_VERSION, MemoryRecord,
        PendingDescriptor, SourceIndexPage, TranscriptDeltaPage, TranscriptManifest, decode, encode,
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

pub(in crate::local_context) fn read_complete_checkpoint_indexes(
    source: StoredArtifact,
    transcript: StoredArtifact,
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
    let (transcript, invocations) = read_transcript_chain(
        transcript,
        None,
        TranscriptManifest::default(),
        BTreeMap::new(),
        &mut read,
    )?;
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

        let (transcript_tail, transcript_changed) = self.store_transcript_pages()?;
        let source = source_tail.ok_or_else(|| error("source index tail is missing"))?;
        let transcript =
            transcript_tail.ok_or_else(|| error("transcript index tail is missing"))?;
        if source_changed || transcript_changed {
            self.commit(
                &MemoryRecord::CheckpointIndex { source, transcript },
                &[source.digest, transcript.digest],
            )?;
        }
        self.source_index_tail = Some(source);
        self.transcript_index_tail = Some(transcript);
        self.indexed_source_count = self.sources.len() as u64;
        self.indexed_transcript.clone_from(&self.transcript);
        self.indexed_invocation_count = self.transcript.invocation;
        Ok((source, transcript))
    }

    pub(super) fn publish_transcript_index(&mut self) -> Result<(), DeveloperLoopError> {
        let (tail, changed) = self.store_transcript_pages()?;
        let Some(transcript) = tail else {
            return Err(error("transcript index tail is missing"));
        };
        if changed {
            self.commit(&MemoryRecord::TranscriptIndex { transcript }, &[transcript.digest])?;
        }
        self.transcript_index_tail = Some(transcript);
        self.indexed_transcript.clone_from(&self.transcript);
        self.indexed_invocation_count = self.transcript.invocation;
        Ok(())
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
        validate_invocation_frontier(
            &self.invocations,
            self.transcript.invocation,
            &self.transcript,
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
        while let Some(next) = next_transcript_step(&current, &self.transcript)? {
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

    pub(super) fn restore_checkpoint_indexes(
        &mut self,
        source: StoredArtifact,
        transcript: StoredArtifact,
    ) -> Result<(), DeveloperLoopError> {
        let (sources, restored_transcript, invocations) = read_complete_checkpoint_indexes(
            source,
            transcript,
            |artifact| self.store.read(artifact),
        )?;
        let source_count = u64::try_from(sources.len())
            .map_err(|_| error("checkpoint source count overflow"))?;
        self.sources = sources;
        self.transcript = restored_transcript.clone();
        self.source_index_tail = Some(source);
        self.transcript_index_tail = Some(transcript);
        self.indexed_source_count = source_count;
        self.indexed_transcript = restored_transcript;
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

    pub(super) fn replay_transcript_index(
        &mut self,
        transcript: StoredArtifact,
    ) -> Result<(), DeveloperLoopError> {
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
