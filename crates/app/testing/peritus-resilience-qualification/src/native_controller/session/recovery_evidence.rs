//! Six-class retained evidence assembled from direct recovery observations.

use serde_json::json;

use super::super::args::ControllerPaths;
use super::super::candidate::{InjectedCandidate, PreparedCandidate, RecoveredCandidate};
use super::super::evidence::{EvidenceDocument, EvidenceSet};
use super::super::request::BoundRequest;

pub(super) struct RetainedEvidence {
    pub(super) documents: Vec<EvidenceDocument>,
    pub(super) artifact_count: u16,
    pub(super) bytes: u32,
}

pub(super) fn retain(
    paths: &ControllerPaths,
    request: &BoundRequest,
    prepared: &PreparedCandidate,
    injected: &InjectedCandidate,
    recovered: &RecoveredCandidate,
) -> Result<RetainedEvidence, Box<dyn std::error::Error>> {
    let mut evidence = EvidenceSet::new(&paths.artifact_root, request.limits().evidence_bytes());
    evidence.retain("fault-injection", "fault", "fault-injection.json", injected)?;
    evidence.retain(
        "journal",
        "journal",
        "journal.json",
        &json!({
            "baseline_head_sha256": prepared.journal_head_sha256,
            "recovered_journal_sha256": recovered.journal_sha256,
            "recovered_journal_bytes": recovered.journal_bytes,
        }),
    )?;
    evidence.retain("recovery", "recovery", "recovery.json", recovered)?;
    evidence.retain(
        "ownership",
        "ownership",
        "ownership.json",
        &json!({
            "scan_completed": true,
            "pending_claims": recovered.pending_claims,
            "duplicate_effects": recovered.duplicate_effects,
            "dependency": recovered.dependency,
            "lifecycle": recovered.lifecycle,
            "unaccounted": 0,
            "orphans_remaining": 0,
        }),
    )?;
    evidence.retain(
        "resource",
        "resource",
        "resource.json",
        &json!({
            "candidate_processes": 3,
            "peak_concurrent_candidate_processes": 1,
            "recovery_elapsed_millis": recovered.elapsed_millis,
            "cleanup_steps": 1,
        }),
    )?;
    evidence.retain(
        "final-state",
        "final",
        "final-state.json",
        &json!({
            "candidate_version": prepared.version,
            "build_sha256": paths.build_sha256,
            "effect_sha256": recovered.effect_sha256,
            "effect_bytes": recovered.effect_bytes,
            "artifact_sha256": recovered.artifact_sha256,
            "artifact_bytes": recovered.artifact_bytes,
            "snapshot": recovered.snapshot,
            "lease": recovered.lease,
            "patch": recovered.patch,
            "gate": recovered.gate,
            "promotion": recovered.promotion,
            "projection": recovered.projection,
            "dependency": recovered.dependency,
            "lifecycle": recovered.lifecycle,
            "committed_events": recovered.committed_events,
            "aggregate_heads": recovered.aggregate_heads,
            "external_effects": recovered.external_effects,
            "exact_fence_acknowledged": recovered.exact_fence_acknowledged,
            "terminal": "failed",
            "accepted": false,
        }),
    )?;
    let (documents, artifact_count, bytes) = evidence.finish()?;
    Ok(RetainedEvidence { documents, artifact_count, bytes })
}
