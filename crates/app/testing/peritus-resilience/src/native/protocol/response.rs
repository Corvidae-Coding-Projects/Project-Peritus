//! Strict native H1 controller response decoding.

use std::path::Path;

use serde::Deserialize;

use crate::{
    AcceptanceObservation, ArtifactHealth, CleanupObservation, CorruptTarget,
    CorruptionObservation, DisruptionObservation, JournalHealth, Milestone, MilestoneKind,
    OwnershipObservation, OwnershipResolution, PreparationObservation, ProjectionHealth,
    QualificationText, RecoveredStateObservation, RecoveryAccounting, RecoveryObservation,
    RecoveryOutcome, ResourceUsage, RetryUsage, ScenarioSpec, SubjectErrorCode, TerminalState,
};

use super::super::subject_error;
use super::artifact;
use super::request::Stage;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseDocument {
    schema_version: u8,
    stage: String,
    sequence: u8,
    instance_id: String,
    scenario_id: String,
    request_sha256: String,
    payload: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparePayload {
    terminal: String,
    journal_head_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InjectPayload {
    reached: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoverPayload {
    outcome: String,
    acceptance: AcceptanceDocument,
    journal: String,
    artifacts: String,
    projection: String,
    corruption: CorruptionDocument,
    ownership: OwnershipDocument,
    retries: RetryDocument,
    resources: ResourceDocument,
    temporary_objects: u16,
    artifact_count: u16,
    evidence: Vec<EvidenceDocument>,
    milestones: Vec<MilestoneDocument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupPayload {
    resources_released: bool,
    owned_work_remaining: u16,
    cleanup_steps: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptanceDocument {
    terminal: String,
    revision_bound: bool,
    evidence_current: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CorruptionDocument {
    detected: Option<String>,
    mutation_admitted: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnershipDocument {
    scan_completed: bool,
    discovered: u16,
    resumed: u16,
    failed: u16,
    indeterminate: u16,
    unaccounted: u16,
    orphan_candidates_detected: u16,
    orphans_remaining: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetryDocument {
    provider: u16,
    tool: u16,
    worker: u16,
    reconciliation: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceDocument {
    events: u32,
    evidence_bytes: u32,
    peak_owned_processes: u16,
    cleanup_steps: u16,
    logical_ticks: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceDocument {
    pub(super) kind: String,
    pub(super) id: String,
    pub(super) path: String,
    pub(super) sha256: String,
    pub(super) bytes: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MilestoneDocument {
    sequence: u16,
    kind: String,
    detail: String,
}

pub(in crate::native) enum ValidatedStage {
    Preparation(PreparationObservation),
    Injection(DisruptionObservation),
    Recovery(RecoveryObservation),
    Cleanup(CleanupObservation),
}

pub(in crate::native) fn parse_response(
    bytes: &[u8],
    expected_stage: Stage,
    expected_sequence: u8,
    expected_request_sha256: &str,
    instance_id: &str,
    scenario: &ScenarioSpec,
    artifact_root: &Path,
) -> Result<ValidatedStage, crate::SubjectError> {
    let document: ResponseDocument = serde_json::from_slice(bytes)
        .map_err(|error| invalid(format!("decode native controller response JSON: {error}")))?;
    validate_identity(
        &document,
        expected_stage,
        expected_sequence,
        expected_request_sha256,
        instance_id,
        scenario,
    )?;
    match expected_stage {
        Stage::Prepare => {
            let payload: PreparePayload = decode_payload(document.payload)?;
            Ok(ValidatedStage::Preparation(PreparationObservation::new(
                scenario.id().clone(),
                terminal(&payload.terminal)?,
                artifact::parse_sha256(&payload.journal_head_sha256)?,
            )))
        }
        Stage::Inject => {
            let payload: InjectPayload = decode_payload(document.payload)?;
            Ok(ValidatedStage::Injection(DisruptionObservation::new(
                scenario.id().clone(),
                scenario.fault(),
                payload.reached,
            )))
        }
        Stage::Recover => {
            let payload: RecoverPayload = decode_payload(document.payload)?;
            recover(payload, scenario, artifact_root).map(ValidatedStage::Recovery)
        }
        Stage::Cleanup => {
            let payload: CleanupPayload = decode_payload(document.payload)?;
            Ok(ValidatedStage::Cleanup(CleanupObservation::new(
                payload.resources_released,
                payload.owned_work_remaining,
                payload.cleanup_steps,
            )))
        }
    }
}

fn validate_identity(
    document: &ResponseDocument,
    expected_stage: Stage,
    expected_sequence: u8,
    expected_request_sha256: &str,
    instance_id: &str,
    scenario: &ScenarioSpec,
) -> Result<(), crate::SubjectError> {
    if document.schema_version != 1
        || document.stage != expected_stage.code()
        || document.sequence != expected_sequence
        || document.instance_id != instance_id
        || document.scenario_id != scenario.id().as_str()
        || document.request_sha256 != expected_request_sha256
    {
        return Err(invalid(
            "controller response is stale or not bound to the exact stage request",
        ));
    }
    Ok(())
}

fn recover(
    payload: RecoverPayload,
    scenario: &ScenarioSpec,
    artifact_root: &Path,
) -> Result<RecoveryObservation, crate::SubjectError> {
    let evidence = artifact::validate(
        payload.evidence,
        artifact_root,
        payload.artifact_count,
        payload.resources.evidence_bytes,
    )?;
    let milestones = payload
        .milestones
        .into_iter()
        .map(|milestone| {
            Ok(Milestone::new(
                milestone.sequence,
                milestone_kind(&milestone.kind)?,
                QualificationText::new(milestone.detail)
                    .map_err(|error| invalid(error.to_string()))?,
            ))
        })
        .collect::<Result<Vec<_>, crate::SubjectError>>()?;
    let state = RecoveredStateObservation::new(
        AcceptanceObservation::new(
            terminal(&payload.acceptance.terminal)?,
            payload.acceptance.revision_bound,
            payload.acceptance.evidence_current,
        ),
        journal_health(&payload.journal)?,
        artifact_health(&payload.artifacts)?,
        projection_health(&payload.projection)?,
        CorruptionObservation::new(
            payload.corruption.detected.as_deref().map(corrupt_target).transpose()?,
            payload.corruption.mutation_admitted,
        ),
        payload.temporary_objects,
    );
    let ownership = payload.ownership;
    let accounting = RecoveryAccounting::new(
        OwnershipObservation::new(
            ownership.scan_completed,
            ownership.discovered,
            OwnershipResolution::new(
                ownership.resumed,
                ownership.failed,
                ownership.indeterminate,
                ownership.unaccounted,
            ),
            ownership.orphan_candidates_detected,
            ownership.orphans_remaining,
        ),
        RetryUsage::new(
            payload.retries.provider,
            payload.retries.tool,
            payload.retries.worker,
            payload.retries.reconciliation,
        ),
        ResourceUsage::new(
            payload.resources.events,
            payload.resources.evidence_bytes,
            payload.resources.peak_owned_processes,
            payload.resources.cleanup_steps,
            payload.resources.logical_ticks,
        ),
    );
    RecoveryObservation::new(
        scenario.id().clone(),
        recovery_outcome(&payload.outcome)?,
        state,
        accounting,
        evidence,
        milestones,
    )
    .map_err(|error| invalid(error.to_string()))
}

fn decode_payload<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
) -> Result<T, crate::SubjectError> {
    serde_json::from_value(value)
        .map_err(|error| invalid(format!("decode native controller stage payload: {error}")))
}

fn terminal(value: &str) -> Result<TerminalState, crate::SubjectError> {
    match value {
        "active" => Ok(TerminalState::Active),
        "paused" => Ok(TerminalState::Paused),
        "blocked" => Ok(TerminalState::Blocked),
        "failed" => Ok(TerminalState::Failed),
        "cancelled" => Ok(TerminalState::Cancelled),
        "exhausted" => Ok(TerminalState::Exhausted),
        "accepted" => Ok(TerminalState::Accepted),
        _ => Err(invalid("terminal state is not a canonical H1 value")),
    }
}

fn recovery_outcome(value: &str) -> Result<RecoveryOutcome, crate::SubjectError> {
    match value {
        "rolled-back-uncommitted" => Ok(RecoveryOutcome::RolledBackUncommitted),
        "replayed-committed" => Ok(RecoveryOutcome::ReplayedCommitted),
        "rebuilt-projection" => Ok(RecoveryOutcome::RebuiltProjection),
        "quarantined-corruption" => Ok(RecoveryOutcome::QuarantinedCorruption),
        "failed-closed" => Ok(RecoveryOutcome::FailedClosed),
        "discarded-unreferenced" => Ok(RecoveryOutcome::DiscardedUnreferenced),
        "reconciled-owned-work" => Ok(RecoveryOutcome::ReconciledOwnedWork),
        "retry-budget-exhausted" => Ok(RecoveryOutcome::RetryBudgetExhausted),
        _ => Err(invalid("recovery outcome is not a canonical H1 value")),
    }
}

fn journal_health(value: &str) -> Result<JournalHealth, crate::SubjectError> {
    match value {
        "verified" => Ok(JournalHealth::Verified),
        "recovered-and-verified" => Ok(JournalHealth::RecoveredAndVerified),
        "hash-divergence-detected" => Ok(JournalHealth::HashDivergenceDetected),
        "unavailable" => Ok(JournalHealth::Unavailable),
        _ => Err(invalid("journal health is not a canonical H1 value")),
    }
}

fn artifact_health(value: &str) -> Result<ArtifactHealth, crate::SubjectError> {
    match value {
        "verified" => Ok(ArtifactHealth::Verified),
        "divergence-detected" => Ok(ArtifactHealth::DivergenceDetected),
        "unavailable" => Ok(ArtifactHealth::Unavailable),
        _ => Err(invalid("artifact health is not a canonical H1 value")),
    }
}

fn projection_health(value: &str) -> Result<ProjectionHealth, crate::SubjectError> {
    match value {
        "verified" => Ok(ProjectionHealth::Verified),
        "rebuilt-and-verified" => Ok(ProjectionHealth::RebuiltAndVerified),
        "divergent" => Ok(ProjectionHealth::Divergent),
        "unavailable" => Ok(ProjectionHealth::Unavailable),
        _ => Err(invalid("projection health is not a canonical H1 value")),
    }
}

fn corrupt_target(value: &str) -> Result<CorruptTarget, crate::SubjectError> {
    match value {
        "journal" => Ok(CorruptTarget::Journal),
        "blob" => Ok(CorruptTarget::Blob),
        "snapshot" => Ok(CorruptTarget::Snapshot),
        "projection" => Ok(CorruptTarget::Projection),
        "acceptance-evidence" => Ok(CorruptTarget::AcceptanceEvidence),
        "harness-promotion" => Ok(CorruptTarget::HarnessPromotion),
        _ => Err(invalid("corruption target is not a canonical H1 value")),
    }
}

fn milestone_kind(value: &str) -> Result<MilestoneKind, crate::SubjectError> {
    match value {
        "prepared" => Ok(MilestoneKind::Prepared),
        "fault-armed" => Ok(MilestoneKind::FaultArmed),
        "fault-observed" => Ok(MilestoneKind::FaultObserved),
        "recovery-started" => Ok(MilestoneKind::RecoveryStarted),
        "reconciled" => Ok(MilestoneKind::Reconciled),
        "inspected" => Ok(MilestoneKind::Inspected),
        _ => Err(invalid("milestone kind is not a canonical H1 value")),
    }
}

fn invalid(detail: impl Into<String>) -> crate::SubjectError {
    subject_error(SubjectErrorCode::Observation, detail, false)
}
