//! Deterministic immutable C7/C0 evidence selection.

mod canonical;
mod engine;
mod manifest;
mod provenance;

pub use engine::select_evidence;
pub use manifest::{SelectedArtifact, SelectedEvidence, SelectionCounts, TraceSelectionManifest};
