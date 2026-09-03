//! Verified terminal settlement and unconditional publication guard.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceRecord, EvidenceStatus,
    QualificationEvidence, RunDisposition, SettlementCause, SettlementError, SettlementErrorKind,
    SettlementReducer,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

use crate::{
    BenchmarkAgentIdentity, BenchmarkError,
    candidate::{self, CandidateSnapshot},
    evidence::{
        BenchmarkSuite, CandidateReport, ExternalEvaluation, HandshakeReport, InvocationReport,
        ProviderRouteReport, QualificationReport, RelocatablePaths, ResourceReport, TraceUsage,
    },
    publication::AtomicPublisher,
    workspace::Baseline,
};

/// Fixed admitted facts and evolving evidence for one invocation.
pub struct ReportSeed {
    pub suite: BenchmarkSuite,
    pub handshake: HandshakeReport,
    pub agent_identity: BenchmarkAgentIdentity,
    pub task_id: String,
    pub session_id: String,
    pub harness_model_id: String,
    pub workspace: PathBuf,
    pub trace_path: PathBuf,
    pub conversation_turn: usize,
    pub writer: String,
    pub reviewer: String,
    pub run_id: RunId,
    pub workspace_id: WorkspaceId,
    pub baseline: Option<Baseline>,
    pub provider_routes: Vec<ProviderRouteReport>,
    pub session_trace_paths: Vec<PathBuf>,
    pub usage_proxy: Option<PathBuf>,
    pub projected_responses: usize,
    pub usage: TraceUsage,
    pub resources: ResourceReport,
    pub last_observation_path: Option<PathBuf>,
    pub relocatable_paths: Option<RelocatablePaths>,
}

/// Terminal observations reduced into one honest report.
pub struct TerminalFacts {
    pub cause: SettlementCause,
    pub snapshot: Option<CandidateSnapshot>,
    pub qualified: bool,
    pub qualification: QualificationReport,
    pub summary: Option<String>,
    pub failure_kind: Option<String>,
    pub failure: Option<String>,
}

impl TerminalFacts {
    pub fn failure(
        cause: SettlementCause,
        snapshot: Option<CandidateSnapshot>,
        error: &BenchmarkError,
    ) -> Self {
        let qualification = snapshot.as_ref().map_or_else(QualificationReport::missing, |_| {
            QualificationReport::candidate("changed", None, None)
        });
        Self {
            cause,
            snapshot,
            qualified: false,
            qualification,
            summary: None,
            failure_kind: Some(error.stable_kind().to_owned()),
            failure: Some(error.to_string()),
        }
    }
}

/// Guard installed only after all paths required for terminal evidence are prepared.
pub struct InvocationGuard {
    seed: ReportSeed,
    publisher: AtomicPublisher,
    started: Instant,
    settled: bool,
}

impl InvocationGuard {
    pub fn new(seed: ReportSeed, publisher: AtomicPublisher) -> Self {
        Self { seed, publisher, started: Instant::now(), settled: false }
    }

    pub const fn seed(&self) -> &ReportSeed {
        &self.seed
    }

    pub const fn seed_mut(&mut self) -> &mut ReportSeed {
        &mut self.seed
    }

    pub fn finalize(&mut self, facts: TerminalFacts) -> Result<InvocationReport, BenchmarkError> {
        if self.settled {
            return Err(BenchmarkError::DuplicateFinalization);
        }
        self.settled = true;
        let mut report = build_report(&self.seed, self.started, facts)?;
        self.publisher.publish(&mut report)?;
        Ok(report)
    }

    pub fn fail(
        &mut self,
        cause: SettlementCause,
        error: &BenchmarkError,
    ) -> Result<InvocationReport, BenchmarkError> {
        let snapshot = candidate::capture(
            &self.seed.workspace,
            self.seed.baseline.as_ref().map(|value| value.head.as_str()),
        )
        .ok()
        .filter(|value| !value.changed_paths.is_empty());
        self.finalize(TerminalFacts::failure(cause, snapshot, error))
    }
}

impl Drop for InvocationGuard {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        self.settled = true;
        let snapshot = candidate::capture(
            &self.seed.workspace,
            self.seed.baseline.as_ref().map(|value| value.head.as_str()),
        )
        .ok()
        .filter(|value| !value.changed_paths.is_empty());
        let error = BenchmarkError::Workspace(
            "admitted invocation left scope before explicit settlement".to_owned(),
        );
        if let Ok(mut report) = build_report(
            &self.seed,
            self.started,
            TerminalFacts::failure(SettlementCause::InternalInvariant, snapshot, &error),
        ) {
            let _ = self.publisher.publish(&mut report);
        }
    }
}

fn build_report(
    seed: &ReportSeed,
    started: Instant,
    facts: TerminalFacts,
) -> Result<InvocationReport, BenchmarkError> {
    let settlement =
        verified_settlement(seed, facts.snapshot.as_ref(), facts.qualified, facts.cause)?;
    let disposition = disposition_name(settlement.disposition());
    let terminal_cause = cause_name(settlement.cause());
    let candidate = facts
        .snapshot
        .as_ref()
        .map(|snapshot| CandidateReport::from_snapshot(snapshot, facts.qualification.stage));
    let changed_paths =
        candidate.as_ref().map_or_else(Vec::new, |candidate| candidate.changed_paths.clone());
    let baseline = seed.baseline.as_ref();
    Ok(InvocationReport {
        schema_version: crate::identity::INVOCATION_REPORT_SCHEMA_VERSION,
        suite: seed.suite,
        handshake: seed.handshake.clone(),
        agent_identity: seed.agent_identity.clone(),
        success: settlement.is_accepted(),
        disposition,
        terminal_cause,
        candidate,
        qualification: facts.qualification,
        provider_routes: seed.provider_routes.clone(),
        external_evaluation: ExternalEvaluation::default(),
        task_id: seed.task_id.clone(),
        session_id: seed.session_id.clone(),
        harness_model_id: seed.harness_model_id.clone(),
        workspace: seed.workspace.clone(),
        baseline_head: baseline.map(|value| value.head.clone()),
        initialized_repository: baseline.is_some_and(|value| value.initialized_repository),
        created_artifact_manifest: baseline.is_some_and(|value| value.created_artifact_manifest),
        writer: seed.writer.clone(),
        reviewer: seed.reviewer.clone(),
        elapsed_ms: started.elapsed().as_millis(),
        trace_path: seed.trace_path.clone(),
        conversation_turn: seed.conversation_turn,
        session_trace_paths: seed.session_trace_paths.clone(),
        usage_proxy: seed.usage_proxy.clone(),
        projected_responses: seed.projected_responses,
        usage: seed.usage,
        resources: seed.resources,
        last_observation_path: seed.last_observation_path.clone(),
        relocatable_paths: seed.relocatable_paths.clone(),
        summary: facts.summary,
        changed_paths,
        failure_kind: facts.failure_kind,
        failure: facts.failure,
    })
}

fn verified_settlement(
    seed: &ReportSeed,
    snapshot: Option<&CandidateSnapshot>,
    qualified: bool,
    cause: SettlementCause,
) -> Result<peritus_run_settlement::RunSettlement, BenchmarkError> {
    let mut reducer = SettlementReducer::new();
    if let Some(snapshot) = snapshot {
        let identity = CandidateIdentity::new(
            seed.run_id,
            seed.workspace_id,
            Sha256Digest::new(snapshot.digest_bytes),
            seed.conversation_turn as u64,
            1,
        )
        .map_err(settlement_error)?;
        let (stage, gates, obligations, review) = if qualified {
            let satisfied = EvidenceRecord::new(identity, QualificationEvidence::Satisfied);
            (
                CandidateStage::Qualified,
                EvidenceStatus::Current(satisfied),
                EvidenceStatus::Current(satisfied),
                EvidenceStatus::Current(satisfied),
            )
        } else {
            (
                CandidateStage::Changed,
                EvidenceStatus::Missing,
                EvidenceStatus::Missing,
                EvidenceStatus::Missing,
            )
        };
        let checkpoint = CandidateCheckpoint::new(identity, stage, gates, obligations, review)
            .map_err(settlement_error)?;
        reducer.observe(checkpoint).map_err(settlement_error)?;
    }
    reducer.settle(cause).map_err(settlement_error)
}

fn settlement_error(error: SettlementError) -> BenchmarkError {
    let kind = match error.kind() {
        SettlementErrorKind::ZeroCheckpointSequence => "zero_checkpoint_sequence",
        SettlementErrorKind::CandidateLineageMismatch => "candidate_lineage_mismatch",
        SettlementErrorKind::CheckpointDidNotAdvance => "checkpoint_did_not_advance",
        SettlementErrorKind::CandidateStageRegressed => "candidate_stage_regressed",
        SettlementErrorKind::CurrentEvidenceBindingMismatch => "current_evidence_binding_mismatch",
        SettlementErrorKind::StaleEvidenceBindingMismatch => "stale_evidence_binding_mismatch",
        SettlementErrorKind::CandidateStageEvidenceMismatch => "candidate_stage_evidence_mismatch",
        SettlementErrorKind::AlreadySettled => "already_settled",
    };
    BenchmarkError::Workspace(format!("run settlement rejected: {kind}"))
}

const fn disposition_name(value: RunDisposition) -> &'static str {
    match value {
        RunDisposition::Accepted => "accepted",
        RunDisposition::CandidateAvailable => "candidate_available",
        RunDisposition::WaitingForUser => "waiting_for_user",
        RunDisposition::FailedNoCandidate => "failed_no_candidate",
        RunDisposition::Cancelled => "cancelled",
        RunDisposition::RecoveryRequired => "recovery_required",
    }
}

const fn cause_name(value: SettlementCause) -> &'static str {
    match value {
        SettlementCause::Completed => "completed",
        SettlementCause::UserWait => "user_wait",
        SettlementCause::Cancellation => "cancellation",
        SettlementCause::Deadline => "deadline",
        SettlementCause::Provider => "provider",
        SettlementCause::Context => "context",
        SettlementCause::Gate => "gate",
        SettlementCause::Review => "review",
        SettlementCause::Repository => "repository",
        SettlementCause::Adapter => "adapter",
        SettlementCause::Recovery => "recovery",
        SettlementCause::InternalInvariant => "internal_invariant",
    }
}

pub fn workspace_id(path: &Path) -> Result<WorkspaceId, BenchmarkError> {
    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    WorkspaceId::new(bytes)
        .map_err(|_| BenchmarkError::Workspace("derived workspace identity is zero".to_owned()))
}

#[cfg(test)]
#[path = "settlement/tests.rs"]
pub mod tests;
