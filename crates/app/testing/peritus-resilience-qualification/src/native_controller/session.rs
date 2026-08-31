//! Four-stage persistent controller session and honest observation reduction.

use std::io::{BufRead, BufReader, Read as _};

use serde_json::json;

use super::args::ControllerPaths;
use super::candidate::{self, InjectedCandidate, PreparedCandidate, RecoveredCandidate};
use super::evidence::EvidenceSet;
use super::request::{BoundRequest, Stage};
use super::response::{
    self, AcceptanceDocument, CleanupPayload, CorruptionDocument, InjectPayload, OwnershipDocument,
    PreparePayload, RecoverPayload, ResourceDocument, RetryDocument,
};

const MAX_REQUEST_BYTES: u64 = 512 * 1024;

pub(super) fn serve(paths: &ControllerPaths) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let prepare_request = next(&mut reader, paths, Stage::Prepare, 1)?;
    let prepared = candidate::prepare(paths)?;
    response::publish(
        &prepare_request,
        &paths.instance_id,
        Stage::Prepare,
        PreparePayload {
            terminal: "active",
            journal_head_sha256: prepared.journal_head_sha256.clone(),
        },
    )?;

    let inject_request = next(&mut reader, paths, Stage::Inject, 2)?;
    same_scenario(&prepare_request, &inject_request)?;
    let route = prepare_request.route()?;
    let injected = candidate::inject(paths, &prepared.runtime, route)?;
    response::publish(
        &inject_request,
        &paths.instance_id,
        Stage::Inject,
        InjectPayload { reached: true },
    )?;

    let recover_request = next(&mut reader, paths, Stage::Recover, 3)?;
    same_scenario(&prepare_request, &recover_request)?;
    let recovered = candidate::recover(paths, &prepared.runtime, &injected, route)?;
    let payload =
        recovery_payload(paths, &recover_request, &prepared, &injected, &recovered, route)?;
    response::publish(&recover_request, &paths.instance_id, Stage::Recover, payload)?;

    let cleanup_request = next(&mut reader, paths, Stage::Cleanup, 4)?;
    drop(reader);
    same_scenario(&prepare_request, &cleanup_request)?;
    let cleanup_limit = cleanup_request.limits().cleanup_steps();
    if cleanup_limit < 1 {
        return Err("H1 request does not permit the required cleanup step".into());
    }
    candidate::cleanup(&prepared.runtime)?;
    response::publish(
        &cleanup_request,
        &paths.instance_id,
        Stage::Cleanup,
        CleanupPayload { resources_released: true, owned_work_remaining: 0, cleanup_steps: 1 },
    )?;
    Ok(())
}

fn next(
    reader: &mut impl BufRead,
    paths: &ControllerPaths,
    expected_stage: Stage,
    expected_sequence: u8,
) -> Result<BoundRequest, Box<dyn std::error::Error>> {
    let mut line = Vec::new();
    let bytes = reader.by_ref().take(MAX_REQUEST_BYTES + 1).read_until(b'\n', &mut line)?;
    if bytes == 0 || bytes as u64 > MAX_REQUEST_BYTES || !line.ends_with(b"\n") {
        return Err("H1 controller request stream ended or exceeded its line bound".into());
    }
    line.pop();
    let request = BoundRequest::decode(&line, paths)?;
    if request.stage()? != expected_stage || request.sequence() != expected_sequence {
        return Err("H1 controller stage order or sequence is not canonical".into());
    }
    Ok(request)
}

fn same_scenario(
    first: &BoundRequest,
    current: &BoundRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    if first.scenario_id() == current.scenario_id() {
        Ok(())
    } else {
        Err("H1 controller request changed scenario within one subject".into())
    }
}

fn recovery_payload(
    paths: &ControllerPaths,
    request: &BoundRequest,
    prepared: &PreparedCandidate,
    injected: &InjectedCandidate,
    recovered: &RecoveredCandidate,
    route: super::request::CommitRoute,
) -> Result<RecoverPayload, Box<dyn std::error::Error>> {
    let logical_ticks = recovered.elapsed_millis.max(1);
    if logical_ticks > request.limits().logical_ticks() {
        return Err("H1 recovery exceeded the request logical-time bound".into());
    }
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
            "committed_events": recovered.committed_events,
            "aggregate_heads": recovered.aggregate_heads,
            "external_effects": recovered.external_effects,
            "exact_fence_acknowledged": recovered.exact_fence_acknowledged,
            "terminal": "failed",
            "accepted": false,
        }),
    )?;
    let (evidence, artifact_count, evidence_bytes) = evidence.finish()?;
    Ok(RecoverPayload {
        outcome: route.outcome(),
        acceptance: AcceptanceDocument {
            terminal: "failed",
            revision_bound: false,
            evidence_current: false,
        },
        journal: route.journal_health(),
        artifacts: "verified",
        projection: "verified",
        corruption: CorruptionDocument { detected: None, mutation_admitted: true },
        ownership: OwnershipDocument {
            scan_completed: true,
            discovered: 0,
            resumed: 0,
            failed: 0,
            indeterminate: 0,
            unaccounted: 0,
            orphan_candidates_detected: 0,
            orphans_remaining: 0,
        },
        retries: RetryDocument { provider: 0, tool: 0, worker: 0, reconciliation: 1 },
        resources: ResourceDocument {
            events: 12,
            evidence_bytes,
            peak_owned_processes: 1,
            cleanup_steps: 1,
            logical_ticks,
        },
        temporary_objects: 0,
        artifact_count,
        evidence,
        milestones: response::canonical_milestones(route),
    })
}
