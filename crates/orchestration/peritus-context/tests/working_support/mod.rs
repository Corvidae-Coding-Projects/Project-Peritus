#![allow(dead_code, reason = "working-state matrices share fixture helpers")]

use peritus_codec::sha256;
use peritus_context::working::{
    ObservationId, ObservationKind, ObservationSource, WorkingBinding, WorkingDelta, WorkingEntry,
    WorkingEntryKind, WorkingEnvironment, WorkingError, WorkingFileDigest, WorkingLimits,
    WorkingLinks, WorkingState, WorkingValidity, apply_working_delta, ingest_working_observation,
};
use peritus_context::{ContextContent, ContextLimits, ContextNodeId, bind_context_content};
use peritus_role::HarnessRole;
use peritus_types::{RunId, WorkspaceId};

pub fn id(byte: u8) -> ContextNodeId {
    ContextNodeId::new([byte; 16]).unwrap()
}
pub fn obs(sequence: u64) -> ObservationId {
    ObservationId::new(sequence).unwrap()
}
pub fn limits() -> WorkingLimits {
    WorkingLimits::new(32, 16, 256, 8, 8).unwrap()
}
pub fn binding() -> WorkingBinding {
    WorkingBinding::new(
        RunId::new([1; 16]).unwrap(),
        WorkspaceId::new([2; 16]).unwrap(),
        id(3),
        HarnessRole::Writer,
        0,
    )
}
pub fn environment(candidate: &[u8], files: Vec<WorkingFileDigest>) -> WorkingEnvironment {
    WorkingEnvironment::new(binding(), sha256(candidate), files, limits()).unwrap()
}
pub fn file(key: u8, bytes: &[u8]) -> WorkingFileDigest {
    WorkingFileDigest::new(id(key), sha256(bytes))
}
pub fn source(sequence: u64) -> ObservationSource {
    ObservationSource::new(
        obs(sequence),
        sha256(&sequence.to_le_bytes()),
        8,
        0,
        8,
        ObservationKind::ToolOutput,
    )
    .unwrap()
}
pub fn state() -> WorkingState {
    let initial =
        WorkingState::new(environment(b"initial", vec![file(10, b"old")]), limits()).unwrap();
    ingest_working_observation(&initial, binding(), source(1)).unwrap()
}
pub fn content(text: &[u8]) -> ContextContent {
    bind_context_content(
        text.to_vec(),
        sha256(text),
        ContextLimits::new(32, 4_096, 16, 11).unwrap(),
    )
    .unwrap()
}
pub fn validity(files: Vec<WorkingFileDigest>) -> WorkingValidity {
    WorkingValidity::new(None, None, files, limits()).unwrap()
}
pub fn entry(
    key: u8,
    sources: Vec<ObservationId>,
    dependencies: Vec<ContextNodeId>,
    valid: WorkingValidity,
) -> WorkingEntry {
    let links = WorkingLinks::new(sources, vec![], dependencies, limits()).unwrap();
    WorkingEntry::new(
        id(key),
        WorkingEntryKind::Hypothesis,
        content(b"source-backed hypothesis"),
        links,
        valid,
        limits(),
    )
    .unwrap()
}
pub fn apply(
    state: &WorkingState,
    entries: Vec<WorkingEntry>,
) -> Result<WorkingState, WorkingError> {
    let delta = WorkingDelta::new(state.binding(), state.revision(), entries, limits()).unwrap();
    apply_working_delta(state, &delta)
}
