//! Content-addressed pages keep checkpoint publication proportional to changed pages.

use super::{
    error,
    record::{ArchivedObservation, CheckpointManifest, decode, encode},
    storage::{LocalStore, StoredArtifact},
};
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{
    WorkingBinding, WorkingLimits, WorkingObservationPage, WorkingState, WorkingStatePageDecoder,
    WorkingStatePages, decode_working_state,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PageManifest {
    version: u16,
    kind: String,
    records: u64,
    pages: Vec<StoredArtifact>,
}

#[derive(Default)]
pub(super) struct PageCache {
    binding: Option<WorkingBinding>,
    pages: Vec<CachedObservationPage>,
    work: PageWork,
}

#[derive(Clone, Copy, Default, Serialize)]
pub(super) struct PageWork {
    encoded_observations: u64,
    encoded_sources: u64,
    reused_observations: u64,
}

impl PageWork {
    pub(super) const fn since(self, previous: Self) -> Self {
        Self {
            encoded_observations: self
                .encoded_observations
                .saturating_sub(previous.encoded_observations),
            encoded_sources: self.encoded_sources.saturating_sub(previous.encoded_sources),
            reused_observations: self
                .reused_observations
                .saturating_sub(previous.reused_observations),
        }
    }
}

#[derive(Clone)]
struct CachedObservationPage {
    identity: WorkingObservationPage,
    begin: u64,
    records: u64,
    working: StoredArtifact,
    sources: StoredArtifact,
}

impl PageCache {
    pub(super) const fn work(&self) -> PageWork {
        self.work
    }

    pub(super) fn restore(
        manifest: &CheckpointManifest,
        state: &WorkingState,
        store: &LocalStore,
    ) -> Result<Self, DeveloperLoopError> {
        if manifest.schema_version < 3 {
            return Ok(Self::default());
        }
        let working: PageManifest = decode(&store.read(manifest.working_state)?)?;
        let sources: PageManifest = decode(&store.read(manifest.source_index)?)?;
        let mut pages = Vec::new();
        let records = sources.records;
        for (index, sources) in sources.pages.into_iter().enumerate() {
            let begin = index as u64 * WorkingStatePages::OBSERVATIONS_PER_PAGE as u64;
            pages.push(CachedObservationPage {
                identity: state
                    .observation_page(state.binding(), index)
                    .map_err(|_| error("restored source page identity unavailable"))?,
                begin,
                records: records
                    .saturating_sub(begin)
                    .min(WorkingStatePages::OBSERVATIONS_PER_PAGE as u64),
                working: *working
                    .pages
                    .get(index + 1)
                    .ok_or_else(|| error("restored working page unavailable"))?,
                sources,
            });
        }
        Ok(Self { binding: Some(state.binding()), pages, work: PageWork::default() })
    }
}

fn next_page(pages: &mut WorkingStatePages<'_>) -> Result<Vec<u8>, DeveloperLoopError> {
    pages
        .next()
        .ok_or_else(|| error("missing working page"))?
        .map_err(|_| error("encode working page"))
}

pub(super) fn store(
    store: &LocalStore,
    state: &WorkingState,
    sources: &[ArchivedObservation],
    cache: &mut PageCache,
    reuse: bool,
) -> Result<(StoredArtifact, StoredArtifact, Vec<peritus_types::Sha256Digest>), DeveloperLoopError>
{
    let mut roots = Vec::new();
    let mut working = PageManifest {
        version: 1,
        kind: "working".into(),
        records: state.through_observation(),
        pages: Vec::new(),
    };
    let mut index = PageManifest {
        version: 1,
        kind: "sources".into(),
        records: sources.len() as u64,
        pages: Vec::new(),
    };
    if !reuse || cache.binding.is_none_or(|binding| !binding.same_lineage(state.binding())) {
        cache.pages.clear();
    }
    // Conversation rebinding changes the header, not immutable source/observation page bytes.
    // Each page below must still match its checked current-state Arc identity.
    cache.binding = Some(state.binding());
    let mut encoder = WorkingStatePages::new(state);
    let header = store.store(&next_page(&mut encoder)?)?;
    working.pages.push(header);
    roots.push(header.digest);
    if sources.is_empty() {
        let bytes = next_page(&mut encoder)?;
        cache.work.encoded_observations = cache.work.encoded_observations.saturating_add(1);
        let empty = store.store(&bytes)?;
        working.pages.push(empty);
        roots.push(empty.digest);
    }
    for (number, page) in sources.chunks(WorkingStatePages::OBSERVATIONS_PER_PAGE).enumerate() {
        let identity = state
            .observation_page(state.binding(), number)
            .map_err(|_| error("source page identity unavailable"))?;
        let begin = number as u64 * WorkingStatePages::OBSERVATIONS_PER_PAGE as u64;
        let records = page.len() as u64;
        let existing = cache.pages.get(number).filter(|cached| {
            cached.begin == begin
                && cached.records == records
                && cached.identity.same_page(&identity)
        });
        let (working_page, source_page) = if let Some(existing) = existing {
            encoder
                .skip_observation_pages(1)
                .map_err(|_| error("invalid reusable working page"))?;
            let reused = (store.reuse(existing.working)?, store.reuse(existing.sources)?);
            cache.work.reused_observations = cache.work.reused_observations.saturating_add(1);
            reused
        } else {
            let bytes = next_page(&mut encoder)?;
            cache.work.encoded_observations = cache.work.encoded_observations.saturating_add(1);
            let working_page = store.store(&bytes)?;
            let bytes = encode(&page)?;
            cache.work.encoded_sources = cache.work.encoded_sources.saturating_add(1);
            let source_page = store.store(&bytes)?;
            if reuse {
                let cached = CachedObservationPage {
                    identity,
                    begin,
                    records,
                    working: working_page,
                    sources: source_page,
                };
                if let Some(previous) = cache.pages.get_mut(number) {
                    *previous = cached;
                } else {
                    cache.pages.push(cached);
                }
            }
            (working_page, source_page)
        };
        roots.extend([working_page.digest, source_page.digest]);
        working.pages.push(working_page);
        index.pages.push(source_page);
    }
    cache.pages.truncate(index.pages.len());
    for page in encoder {
        let artifact = store.store(&page.map_err(|_| error("encode working page"))?)?;
        roots.push(artifact.digest);
        working.pages.push(artifact);
    }
    Ok((store.store(&encode(&working)?)?, store.store(&encode(&index)?)?, roots))
}

fn validate_progress_floor(schema: u16, bytes: &[u8]) -> Result<(), DeveloperLoopError> {
    if schema < super::record::DEPENDENCY_CHECKPOINT_SCHEMA_VERSION
        && (bytes.starts_with(b"PWMP\0\x04") || bytes.starts_with(b"PWMS\0\x04"))
    {
        return Err(error("declared progress entries require a version-6 checkpoint"));
    }
    Ok(())
}

pub(super) fn restore(
    manifest: &CheckpointManifest,
    binding: WorkingBinding,
    limits: WorkingLimits,
    read: impl Fn(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<(WorkingState, Vec<ArchivedObservation>), DeveloperLoopError> {
    if manifest.schema_version < 3 {
        let bytes = read(manifest.working_state)?;
        validate_progress_floor(manifest.schema_version, &bytes)?;
        let state = decode_working_state(&bytes, binding, limits)
            .map_err(|_| error("invalid working checkpoint"))?;
        return Ok((state, decode(&read(manifest.source_index)?)?));
    }
    let working: PageManifest = decode(&read(manifest.working_state)?)?;
    let index: PageManifest = decode(&read(manifest.source_index)?)?;
    if working.version != 1
        || index.version != 1
        || working.kind != "working"
        || index.kind != "sources"
        || working.records != index.records
    {
        return Err(error("invalid checkpoint page manifest"));
    }
    let mut decoder = WorkingStatePageDecoder::new(binding, limits);
    for page in working.pages {
        let bytes = read(page)?;
        validate_progress_floor(manifest.schema_version, &bytes)?;
        decoder.push(&bytes).map_err(|_| error("invalid working checkpoint page"))?;
    }
    let state = decoder.finish().map_err(|_| error("invalid paged working checkpoint"))?;
    if state.through_observation() != working.records {
        return Err(error("working page count mismatch"));
    }
    let mut sources = Vec::new();
    for (number, page) in index.pages.iter().enumerate() {
        let records: Vec<ArchivedObservation> = decode(&read(*page)?)?;
        let remaining = index.records.saturating_sub(sources.len() as u64);
        if records.len() as u64 != remaining.min(512)
            || records.is_empty()
            || (number + 1 < index.pages.len() && records.len() != 512)
        {
            return Err(error("source page count mismatch"));
        }
        sources.extend(records);
    }
    if sources.len() as u64 != index.records {
        return Err(error("incomplete source pages"));
    }
    Ok((state, sources))
}
