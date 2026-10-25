//! Content-addressed pages keep checkpoint publication proportional to changed pages.

use super::{
    error,
    record::{ArchivedObservation, CheckpointManifest, decode, encode},
    storage::{LocalStore, StoredArtifact},
};
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{
    WorkingBinding, WorkingLimits, WorkingState, WorkingStatePageDecoder, WorkingStatePages,
    decode_working_state,
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

pub(super) fn store(
    store: &LocalStore,
    state: &WorkingState,
    sources: &[ArchivedObservation],
) -> Result<(StoredArtifact, StoredArtifact, Vec<peritus_types::Sha256Digest>), DeveloperLoopError>
{
    let mut roots = Vec::new();
    let mut working = PageManifest {
        version: 1,
        kind: "working".into(),
        records: state.through_observation(),
        pages: Vec::new(),
    };
    for page in WorkingStatePages::new(state) {
        let artifact = store.store(&page.map_err(|_| error("encode working page"))?)?;
        roots.push(artifact.digest);
        working.pages.push(artifact);
    }
    let mut index = PageManifest {
        version: 1,
        kind: "sources".into(),
        records: sources.len() as u64,
        pages: Vec::new(),
    };
    for page in sources.chunks(512) {
        let artifact = store.store(&encode(&page)?)?;
        roots.push(artifact.digest);
        index.pages.push(artifact);
    }
    Ok((store.store(&encode(&working)?)?, store.store(&encode(&index)?)?, roots))
}

pub(super) fn restore(
    manifest: &CheckpointManifest,
    binding: WorkingBinding,
    limits: WorkingLimits,
    read: impl Fn(StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError>,
) -> Result<(WorkingState, Vec<ArchivedObservation>), DeveloperLoopError> {
    if manifest.schema_version < 3 {
        let state = decode_working_state(&read(manifest.working_state)?, binding, limits)
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
        decoder.push(&read(page)?).map_err(|_| error("invalid working checkpoint page"))?;
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
