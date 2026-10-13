//! Observational native E3 subjects; no model-loop states or causal inference are manufactured.

use peritus_artifact_store::ArtifactStore;
use peritus_eval::{
    EvaluationCampaignId, EvaluationExecutionMode, EvaluationPhase, FrozenEvaluationProfile,
    RolloutId, RolloutRecord, RolloutStatus, ValidatedEvaluationReport,
};
use peritus_harness::GoverningHarnessBinding;
use peritus_journal::SqliteJournal;
use peritus_trace::TraceId;
use peritus_types::{AttemptId, EnvironmentId, EventId, RunId, Sha256Digest};

use super::{AnalysisSubject, AnalysisSubjectSource, SUBJECT_ID_DOMAIN, binding_error};
use crate::{DebuggerError, SubjectId};

/// Exact actual published E3/terminal/C7 provenance retained by a local-process subject.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalEvaluationSource {
    campaign: EvaluationCampaignId,
    rollout: RolloutId,
    trace: TraceId,
    profile_digest: Sha256Digest,
    report_digest: Sha256Digest,
    terminal_digest: Sha256Digest,
    state_digest: Sha256Digest,
    publication_event: EventId,
    report_commit_position: u64,
    trace_event: EventId,
    trace_position: u64,
    trace_manifest: Sha256Digest,
    failure_observation: bool,
}

impl LocalEvaluationSource {
    /// Actual native E3 campaign.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.campaign
    }
    /// Actual settled logical rollout.
    #[must_use]
    pub const fn rollout_id(&self) -> RolloutId {
        self.rollout
    }
    /// Recovered genuine C7 trace.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace
    }
    /// Whether this subject describes a retained error observation rather than a C2 process.
    #[must_use]
    pub const fn is_failure_observation(&self) -> bool {
        self.failure_observation
    }
    /// Actual immutable terminal record digest.
    #[must_use]
    pub const fn terminal_digest(&self) -> Sha256Digest {
        self.terminal_digest
    }
    /// Actual producing C0 report position.
    #[must_use]
    pub const fn report_commit_position(&self) -> u64 {
        self.report_commit_position
    }
    pub(super) fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.campaign.as_bytes());
        bytes.extend_from_slice(self.rollout.as_bytes());
        bytes.extend_from_slice(self.trace.as_bytes());
        bytes.extend_from_slice(self.profile_digest.as_bytes());
        bytes.extend_from_slice(self.report_digest.as_bytes());
        bytes.extend_from_slice(self.terminal_digest.as_bytes());
        bytes.extend_from_slice(self.state_digest.as_bytes());
        bytes.extend_from_slice(self.publication_event.as_bytes());
        bytes.extend_from_slice(&self.report_commit_position.to_be_bytes());
        bytes.extend_from_slice(self.trace_event.as_bytes());
        bytes.extend_from_slice(&self.trace_position.to_be_bytes());
        bytes.extend_from_slice(self.trace_manifest.as_bytes());
        if self.failure_observation {
            bytes.extend_from_slice(b"failure-observation\0");
        }
        bytes
    }
}

impl AnalysisSubject {
    /// Captures one actual published native rollout and its independently recovered C7 trace.
    ///
    /// Native C7 producers correlate evaluation campaign bytes as `RunId` and logical rollout
    /// bytes as `AttemptId`. These are descriptive correlation aliases, not fabricated E0/D0
    /// aggregates. The C2 session/environment come from the actual native child identity.
    ///
    /// # Errors
    /// Rejects model-backed modes, unpublished/drifted reports, missing actual terminal/C7 bytes,
    /// wrong correlation, or an E1 binding without its actual retained materialization receipt.
    #[allow(
        clippy::too_many_arguments,
        reason = "actual owning inputs and native identity remain explicit"
    )]
    pub fn from_published_local_evaluation(
        journal: &SqliteJournal,
        artifacts: &ArtifactStore,
        profile: &FrozenEvaluationProfile,
        report: &ValidatedEvaluationReport,
        rollout: RolloutId,
        trace_id: TraceId,
        environment: EnvironmentId,
    ) -> Result<Self, DebuggerError> {
        Self::capture_published_local(
            journal,
            artifacts,
            profile,
            report,
            rollout,
            trace_id,
            environment,
            false,
        )
    }

    /// Captures a C7 observation of an actual published infrastructure terminal.
    ///
    /// Observer identities are derived from its real C0 settlement event and terminal artifact.
    /// Missing process traces, children, outputs and measurements remain absent.
    ///
    /// # Errors
    /// Rejects task verdicts, unpublished/drifted owners, or C7 source/observer identity drift.
    #[allow(clippy::too_many_arguments, reason = "actual owning inputs stay explicit")]
    pub fn from_published_local_failure(
        journal: &SqliteJournal,
        artifacts: &ArtifactStore,
        profile: &FrozenEvaluationProfile,
        report: &ValidatedEvaluationReport,
        rollout: RolloutId,
        trace_id: TraceId,
        environment: EnvironmentId,
    ) -> Result<Self, DebuggerError> {
        Self::capture_published_local(
            journal,
            artifacts,
            profile,
            report,
            rollout,
            trace_id,
            environment,
            true,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "actual source scope is explicit")]
    fn capture_published_local(
        journal: &SqliteJournal,
        artifacts: &ArtifactStore,
        profile: &FrozenEvaluationProfile,
        report: &ValidatedEvaluationReport,
        rollout: RolloutId,
        trace_id: TraceId,
        environment: EnvironmentId,
        failure_observation: bool,
    ) -> Result<Self, DebuggerError> {
        if profile.execution().mode() != EvaluationExecutionMode::LocalProcess {
            return Err(binding_error(
                "native subject requires an explicit frozen local-process mode",
            ));
        }
        let campaign = report.report().campaign_id();
        let state = peritus_eval::load_evaluation_replay(journal, campaign)
            .map_err(native_error)?
            .rebuild()
            .map_err(native_error)?
            .ok_or_else(|| binding_error("native evaluation is absent"))?;
        let publication =
            state.publication().ok_or_else(|| binding_error("native evaluation is unpublished"))?;
        let durable_report =
            state.report().ok_or_else(|| binding_error("native evaluation has no report"))?;
        if state.phase() != EvaluationPhase::Published
            || state.profile_digest() != profile.digest()
            || state.dataset_digest() != profile.dataset().digest()
            || durable_report.id() != report.id()
            || durable_report.payload_digest() != report.digest()
            || publication.report_id() != report.id()
            || artifacts
                .read(durable_report.artifact(), durable_report.size())
                .map_err(native_error)?
                != report.bytes()
        {
            return Err(binding_error(
                "native subject profile/report differs from actual publication",
            ));
        }
        let profile_bytes = profile.canonical_bytes().map_err(native_error)?;
        if artifacts
            .read(
                state.profile_artifact(),
                u64::try_from(profile_bytes.len()).map_err(native_error)?,
            )
            .map_err(native_error)?
            != profile_bytes
        {
            return Err(binding_error(
                "native subject profile differs from its actual frozen artifact",
            ));
        }
        let plan = peritus_eval::EvaluationPlan::build(campaign, profile).map_err(native_error)?;
        let spec = plan
            .specs()
            .iter()
            .find(|spec| spec.id() == rollout)
            .ok_or_else(|| binding_error("native subject rollout is not planned"))?;
        let Some(progress) = state.rollout(rollout) else {
            return Err(binding_error("native subject rollout is absent"));
        };
        let RolloutStatus::Settled(reference) = progress.status() else {
            return Err(binding_error("native subject rollout has no actual terminal"));
        };
        let bytes = artifacts
            .read(reference.artifact(), reference.artifact_bytes())
            .map_err(native_error)?;
        let record = RolloutRecord::from_canonical_bytes(&bytes).map_err(native_error)?;
        record.validate_spec(spec).map_err(native_error)?;
        if record.digest() != reference.record_digest() {
            return Err(binding_error(
                "native terminal differs from its protected owning-state digest",
            ));
        }
        let arm = profile.arm(spec.arm());
        let harness = peritus_harness::load_harness_replay(journal, arm.revision().harness_id())
            .map_err(native_error)?
            .rebuild()
            .map_err(native_error)?
            .ok_or_else(|| binding_error("native E1 harness is absent"))?;
        let receipt = harness
            .receipts()
            .values()
            .find(|receipt| receipt.digest() == arm.receipt_digest())
            .ok_or_else(|| binding_error("native E1 arm has no actual retained materialization"))?;
        let revision = harness
            .history()
            .revision(arm.harness_revision().digest())
            .ok_or_else(|| binding_error("native E1 revision is absent"))?;
        if revision.identity() != arm.harness_revision() {
            return Err(binding_error("native E1 revision identity differs"));
        }
        GoverningHarnessBinding::new(arm.revision(), revision, receipt).map_err(native_error)?;
        let recovered = peritus_trace::recover_trace(journal, trace_id).map_err(native_error)?;
        let trace = recovered.trace().ok_or_else(|| binding_error("native C7 trace is absent"))?;
        let observed = trace
            .observations()
            .last()
            .ok_or_else(|| binding_error("native C7 trace has no observations"))?;
        let binding = observed.observation().binding();
        let trace_manifest = if failure_observation {
            if record.trace_root().is_some()
                || !matches!(
                    record.outcome(),
                    peritus_eval::RolloutOutcome::InfrastructureFailed { .. }
                        | peritus_eval::RolloutOutcome::Ambiguous { .. }
                )
            {
                return Err(binding_error("native failure subject invents a process outcome"));
            }
            let replay =
                peritus_eval::load_evaluation_replay(journal, campaign).map_err(native_error)?;
            let settled = replay.events().iter().rev().find(|event| matches!(event.kind(),
                peritus_eval::EvaluationEventKind::Accepted(peritus_eval::EvaluationCommandKind::SettleRollout {rollout_id, terminal})
                    if *rollout_id == rollout && *terminal == reference))
                .ok_or_else(|| binding_error("native failure lacks its actual C0 settlement"))?;
            let mut key = b"PERITUS-RSI-ACTUAL-FAILURE-OBSERVATION-C7\0".to_vec();
            key.extend_from_slice(rollout.as_bytes());
            key.extend_from_slice(reference.artifact().as_bytes());
            key.extend_from_slice(settled.id().as_bytes());
            let expected_trace = TraceId::new(native_identity(&key)).map_err(native_error)?;
            key.extend_from_slice(b"observer-session");
            let expected_session =
                peritus_types::SessionId::new(native_identity(&key)).map_err(native_error)?;
            key.extend_from_slice(b"observer-environment");
            let expected_environment =
                EnvironmentId::new(native_identity(&key)).map_err(native_error)?;
            if trace_id != expected_trace
                || environment != expected_environment
                || binding.session_id() != expected_session
                || trace.observations().iter().any(|row| {
                    !row.observation().attributes().iter().any(|attr| {
                        attr.key() == peritus_trace::SafeAttributeKey::Recovery
                            && attr.value()
                                == peritus_trace::SafeAttributeValue::Digest(
                                    reference.artifact().sha256(),
                                )
                    })
                })
            {
                return Err(binding_error(
                    "native failure C7 observer differs from actual C0 terminal source",
                ));
            }
            reference.artifact()
        } else {
            let manifest_root = record
                .trace_root()
                .ok_or_else(|| binding_error("native process has no real C7 manifest"))?;
            let bytes = artifacts.read(manifest_root, 16 * 1024 * 1024).map_err(native_error)?;
            let manifest: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(native_error)?;
            let expected_trace = serde_json::to_value(trace_id.as_bytes()).map_err(native_error)?;
            let stage = ["candidate", "evaluator"]
                .into_iter()
                .filter_map(|key| manifest.get(key))
                .find(|stage| stage.get("trace_id") == Some(&expected_trace))
                .ok_or_else(|| {
                    binding_error("native process trace is absent from its protected manifest")
                })?;
            if manifest.get("schema").and_then(serde_json::Value::as_str)
                != Some("peritus.native-evaluation-trace.v1")
                || manifest.get("campaign")
                    != Some(&serde_json::to_value(campaign.as_bytes()).map_err(native_error)?)
                || manifest.get("rollout")
                    != Some(&serde_json::to_value(rollout.as_bytes()).map_err(native_error)?)
                || stage.get("environment_id")
                    != Some(&serde_json::to_value(environment.as_bytes()).map_err(native_error)?)
                || stage.get("session_id")
                    != Some(
                        &serde_json::to_value(binding.session_id().as_bytes())
                            .map_err(native_error)?,
                    )
            {
                return Err(binding_error(
                    "native process C7 manifest differs from its actual E3/C2 source",
                ));
            }
            manifest_root
        };
        let run_id = RunId::new(*campaign.as_bytes()).map_err(native_error)?;
        let attempt_id = AttemptId::new(*rollout.as_bytes()).map_err(native_error)?;
        if binding.run_id() != Some(run_id)
            || binding.attempt_id() != Some(attempt_id)
            || trace.observations().iter().any(|row| {
                row.observation().binding().session_id() != binding.session_id()
                    || row.observation().binding().run_id() != Some(run_id)
                    || row.observation().binding().attempt_id() != Some(attempt_id)
            })
        {
            return Err(binding_error("native C7 trace belongs to another evaluation/rollout"));
        }
        let source = LocalEvaluationSource {
            campaign,
            rollout,
            trace: trace_id,
            profile_digest: Sha256Digest::new(*profile.digest().as_bytes()),
            report_digest: report.digest(),
            terminal_digest: record.digest(),
            state_digest: state.state_digest(),
            publication_event: state.last_event_id(),
            report_commit_position: publication.report_commit_position(),
            trace_event: observed.observation().event_id(),
            trace_position: observed.journal_position(),
            trace_manifest: trace_manifest.sha256(),
            failure_observation,
        };
        let mut subject = Self {
            id: SubjectId::new([1; 16])?,
            run_id,
            attempt_id,
            session_id: binding.session_id(),
            environment_id: environment,
            revision: arm.revision(),
            harness_revision: arm.harness_revision(),
            source: AnalysisSubjectSource::LocalProcess(source),
        };
        subject.id = SubjectId::derive(SUBJECT_ID_DOMAIN, &subject.canonical_binding_bytes())?;
        Ok(subject)
    }

    /// Returns actual native E3 provenance, or absence for a model-loop subject.
    #[must_use]
    pub const fn local_evaluation(&self) -> Option<&LocalEvaluationSource> {
        match &self.source {
            AnalysisSubjectSource::LocalProcess(source) => Some(source),
            AnalysisSubjectSource::ModelLoop(_) => None,
        }
    }
}

fn native_error<T>(_: T) -> DebuggerError {
    binding_error("native E3/C7/E1 owning evidence could not be recovered exactly")
}

fn native_identity(bytes: &[u8]) -> [u8; 16] {
    let digest = peritus_codec::sha256(bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    if id == [0; 16] {
        id[15] = 1;
    }
    id
}
