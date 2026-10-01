//! Human and JSON projections of authoritative product-run observations.

use std::fmt::Write as _;

use peritus_app_protocol::{ProductDeliverable, ProductRunPhase, ProductRunSnapshot};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateStage, EvidenceStatus, QualificationEvidence,
};

use crate::id::hex;

use super::ObservedRun;

pub(super) fn observed_json(run: &ObservedRun) -> serde_json::Value {
    let snapshot = &run.snapshot;
    let deliverable = snapshot.deliverable();
    serde_json::json!({
        "run_id": hex(snapshot.run_id().as_bytes()),
        "workspace_id": hex(snapshot.workspace_id().as_bytes()),
        "state": state_name(snapshot),
        "phase": format!("{:?}", snapshot.phase()),
        "cycle": snapshot.cycle(),
        "task": snapshot.task(),
        "status": snapshot.status(),
        "summary": snapshot.summary(),
        "diff": snapshot.diff(),
        "checks": snapshot.gates(),
        "review": snapshot.review(),
        "operation": {
            "kind": format!("{:?}", snapshot.operation().kind()),
            "state": format!("{:?}", snapshot.operation().state()),
            "identity": snapshot.operation().identity(),
            "known": snapshot.operation().known(),
            "uncertainty": snapshot.operation().uncertainty(),
            "legal_controls": legal_controls_json(snapshot),
        },
        "settlement": run.settlement.as_ref().map(|value| serde_json::json!({
            "disposition": format!("{:?}", value.disposition()),
            "cause": format!("{:?}", value.cause()),
            "repository_digest": value.checkpoint().map(|checkpoint| hex(checkpoint.identity().repository_digest().as_bytes())),
            "evidence": value.checkpoint().map(evidence_json),
        })),
        "deliverable": deliverable.map(deliverable_json),
    })
}

fn deliverable_json(value: &ProductDeliverable) -> serde_json::Value {
    serde_json::json!({
        "workspace": value.workspace_path(),
        "changed_paths": value.changed_paths(),
        "successful_commands": value.successful_commands(),
        "run_instructions": value.run_instructions(),
        "qualification": qualification_name(value.qualification()),
        "accepted": value.accepted(),
        "commit_revision": value.commit_revision(),
        "export_path": value.export_path(),
        "discarded": value.discarded(),
    })
}

fn evidence_json(value: &CandidateCheckpoint) -> serde_json::Value {
    serde_json::json!({
        "checks": evidence_name(value.gates()),
        "requirements": evidence_name(value.obligations()),
        "review": evidence_name(value.review()),
    })
}

pub(super) fn observed_human(run: &ObservedRun) -> String {
    let snapshot = &run.snapshot;
    let mut text = format!(
        "{}  {}\n{}\n{}",
        hex(snapshot.run_id().as_bytes()),
        state_name(snapshot),
        snapshot.task(),
        snapshot.status(),
    );
    if !snapshot.summary().is_empty() {
        text.push_str("\n\n");
        text.push_str(snapshot.summary());
    }
    let operation = snapshot.operation();
    let _ = write!(
        text,
        "\n\nOperation: {:?} {:?}\nIdentity: {}\nKnown: {}\nUncertain: {}\nLegal controls: {}",
        operation.kind(),
        operation.state(),
        operation.identity(),
        operation.known(),
        if operation.uncertainty().is_empty() {
            "nothing material"
        } else {
            operation.uncertainty()
        },
        legal_controls_human(snapshot),
    );
    if let Some(settlement) = &run.settlement {
        let _ = write!(
            text,
            "\n\nStopped because: {:?}\nDisposition: {:?}",
            settlement.cause(),
            settlement.disposition(),
        );
        if let Some(checkpoint) = settlement.checkpoint() {
            let _ = write!(
                text,
                "\nRepository digest: {}\nChecks: {}; requirements: {}; review: {}",
                hex(checkpoint.identity().repository_digest().as_bytes()),
                evidence_name(checkpoint.gates()),
                evidence_name(checkpoint.obligations()),
                evidence_name(checkpoint.review()),
            );
        }
    }
    if let Some(deliverable) = snapshot.deliverable() {
        let _ = write!(
            text,
            "\n\nWorkspace: {}\nQualification: {}\nChanged paths:\n{}\nSuccessful commands:\n{}\nRun: {}",
            deliverable.workspace_path(),
            qualification_name(deliverable.qualification()),
            display_list(deliverable.changed_paths()),
            display_list(deliverable.successful_commands()),
            deliverable.run_instructions(),
        );
    }
    text
}

fn legal_controls_json(snapshot: &ProductRunSnapshot) -> serde_json::Value {
    let value = snapshot.operation().legal_controls();
    serde_json::json!({
        "cancel": value.cancel(),
        "retry": value.retry(),
        "accept": value.accept(),
        "commit": value.commit(),
        "export": value.export(),
        "discard": value.discard(),
        "acknowledge": value.acknowledge(),
    })
}

fn legal_controls_human(snapshot: &ProductRunSnapshot) -> String {
    let value = snapshot.operation().legal_controls();
    let mut controls = Vec::new();
    for (allowed, name) in [
        (value.cancel(), "cancel"),
        (value.retry(), "exact retry"),
        (value.accept(), "accept"),
        (value.commit(), "commit"),
        (value.export(), "export"),
        (value.discard(), "discard"),
        (value.acknowledge(), "acknowledge uncertainty"),
    ] {
        if allowed {
            controls.push(name);
        }
    }
    if controls.is_empty() { "none".to_owned() } else { controls.join(", ") }
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() { "  (none)".to_owned() } else { format!("  {}", values.join("\n  ")) }
}

const fn state_name(snapshot: &ProductRunSnapshot) -> &'static str {
    match (snapshot.phase(), snapshot.deliverable()) {
        (ProductRunPhase::Complete, _) => "Accepted",
        (ProductRunPhase::WaitingForUser, _) => "Waiting for you",
        (ProductRunPhase::Failed, Some(_)) => "Candidate available",
        (ProductRunPhase::Failed, None) => "Stopped with no candidate",
        (ProductRunPhase::Cancelled, Some(_)) => "Cancelled; candidate available",
        (ProductRunPhase::Cancelled, None) => "Cancelled",
        (ProductRunPhase::RecoveryRequired, _) => "Recovery required",
        _ => "Running",
    }
}

const fn qualification_name(value: CandidateStage) -> &'static str {
    match value {
        CandidateStage::Observed => "observed",
        CandidateStage::Changed => "changed",
        CandidateStage::SelfChecked => "self-checked",
        CandidateStage::GatesPassed => "checks passed",
        CandidateStage::ReviewPending => "review pending",
        CandidateStage::Qualified => "qualified",
    }
}

pub(super) fn missing_evidence(value: &CandidateCheckpoint) -> String {
    [("checks", value.gates()), ("requirements", value.obligations()), ("review", value.review())]
        .into_iter()
        .filter_map(|(name, evidence)| {
            let state = evidence_name(evidence);
            (state != "passed").then(|| format!("{name} {state}"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

const fn evidence_name(value: &EvidenceStatus<QualificationEvidence>) -> &'static str {
    match value {
        EvidenceStatus::Missing => "missing",
        EvidenceStatus::Failed(_) => "failed",
        EvidenceStatus::Stale(_) => "stale",
        EvidenceStatus::Current(record) => {
            if record.value().satisfied() {
                "passed"
            } else {
                "failed"
            }
        }
    }
}
