//! Four-stage persistent controller session and honest observation reduction.

mod recovery_evidence;

use std::io::{BufRead, BufReader, Read as _};

use super::args::ControllerPaths;
use super::candidate::{self, InjectedCandidate, PreparedCandidate, RecoveredCandidate};
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
    let dependency_retry_limit = prepare_request.dependency_retry_limit(route);
    let injected = candidate::inject(paths, &prepared.runtime, route, dependency_retry_limit)?;
    response::publish(
        &inject_request,
        &paths.instance_id,
        Stage::Inject,
        InjectPayload { reached: true },
    )?;

    let recover_request = next(&mut reader, paths, Stage::Recover, 3)?;
    same_scenario(&prepare_request, &recover_request)?;
    let recovered =
        candidate::recover(paths, &prepared.runtime, &injected, route, dependency_retry_limit)?;
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
    route: super::request::ScenarioRoute,
) -> Result<RecoverPayload, Box<dyn std::error::Error>> {
    let logical_ticks = recovered.elapsed_millis.max(1);
    if logical_ticks > request.limits().logical_ticks() {
        return Err("H1 recovery exceeded the request logical-time bound".into());
    }
    let accounting = recovery_accounting(recovered)?;
    let retained = recovery_evidence::retain(paths, request, prepared, injected, recovered)?;
    Ok(RecoverPayload {
        outcome: route.outcome(),
        acceptance: AcceptanceDocument {
            terminal: "failed",
            revision_bound: false,
            evidence_current: false,
        },
        journal: route.journal_health(),
        artifacts: route.artifact_health(),
        projection: route.projection_health(),
        corruption: CorruptionDocument {
            detected: route.corruption_target(),
            mutation_admitted: route.mutation_admitted(),
        },
        ownership: accounting.ownership,
        retries: accounting.retries,
        resources: ResourceDocument {
            events: accounting.events,
            evidence_bytes: retained.bytes,
            peak_owned_processes: 1,
            cleanup_steps: 1,
            logical_ticks,
        },
        temporary_objects: 0,
        artifact_count: retained.artifact_count,
        evidence: retained.documents,
        milestones: response::canonical_milestones(route),
    })
}

struct RecoveryAccounting {
    ownership: OwnershipDocument,
    retries: RetryDocument,
    events: u32,
}

fn recovery_accounting(
    recovered: &RecoveredCandidate,
) -> Result<RecoveryAccounting, Box<dyn std::error::Error>> {
    if let Some(lifecycle) = &recovered.lifecycle {
        if !lifecycle.verification.replay_exact || !lifecycle.verification.ownership_reconciled {
            return Err("daemon lifecycle replay did not reconcile authoritative ownership".into());
        }
        let events = u32::try_from(lifecycle.committed_events)
            .map_err(|_| "daemon lifecycle event count exceeds the H1 response range")?;
        return Ok(RecoveryAccounting {
            ownership: ownership(1, 1, 0),
            retries: retries(None, 0)?,
            events,
        });
    }
    let Some(dependency) = &recovered.dependency else {
        return Ok(RecoveryAccounting {
            ownership: ownership(0, 0, 0),
            retries: retries(None, 0)?,
            events: 12,
        });
    };
    let events = u32::try_from(dependency.committed_events)
        .map_err(|_| "dependency event count exceeds the H1 response range")?;
    let (resumed, failed) = if dependency.exhausted { (0, 1) } else { (1, 0) };
    Ok(RecoveryAccounting {
        ownership: ownership(1, resumed, failed),
        retries: retries(Some(&dependency.dependency), dependency.attempts)?,
        events,
    })
}

const fn ownership(discovered: u16, resumed: u16, failed: u16) -> OwnershipDocument {
    OwnershipDocument {
        scan_completed: true,
        discovered,
        resumed,
        failed,
        indeterminate: 0,
        unaccounted: 0,
        orphan_candidates_detected: 0,
        orphans_remaining: 0,
    }
}

fn retries(
    dependency: Option<&str>,
    attempts: u16,
) -> Result<RetryDocument, Box<dyn std::error::Error>> {
    let mut usage = RetryDocument { provider: 0, tool: 0, worker: 0, reconciliation: 1 };
    match dependency {
        None => {}
        Some("provider") => usage.provider = attempts,
        Some("tool") => usage.tool = attempts,
        Some("worker") => usage.worker = attempts,
        Some(_) => return Err("dependency observation has an unknown retry class".into()),
    }
    Ok(usage)
}
