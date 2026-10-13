//! Actual restricted local candidate/evaluator port for an already committed E3 attempt.

mod catalog;

pub use catalog::{
    LocalProcessBackend, LocalProcessCatalog, LocalProcessLimitsDecl, LocalProgram,
    LocalSupportFile,
};

use super::execution::{self, ExecutionArgument, ProcessCompletion, ProcessInputs};
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, WriteRequest,
};
use peritus_eval::{
    CandidateExecutionDirective, CandidateObservation, CommittedEvaluationTransition,
    EvaluationArm, EvaluationExecutionMode, EvaluatorExecutionDirective, EvaluatorObservation,
    EvaluatorVerdict, ExecutionDirectiveClaim, ExecutionDirectiveKind, ExecutionFailure,
    FrozenEvaluationProfile, InfrastructureFailureClass, ResourceObservation, RolloutExecutionPort,
    RolloutSpec, RolloutStatus, TaskFailureClass,
};
use peritus_process::{
    CancellationReason, ExecutionGateway, OsExitObservation, ProcessCursor,
    ProcessResourceDimension, ProcessStore, ResourceFidelity, TerminalDisposition, TerminalResult,
};
use peritus_product_runner::RestrictedProcessContext;
use peritus_types::{EventId, RunId, Sha256Digest};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Role output roots retained for independent evaluator input and complete public evidence.
#[derive(Clone, Copy, Debug)]
pub struct LocalStageArtifacts {
    /// Exact raw standard output, including the empty artifact when the program prints nothing.
    pub stdout: ArtifactDigest,
    /// Exact raw standard error.
    pub stderr: ArtifactDigest,
    /// Actual process terminal, bindings, output roots, and resource observations.
    pub terminal: ArtifactDigest,
    /// Exact terminal envelope bytes.
    pub terminal_bytes: u64,
}

/// Private daemon-owned runtime port. Its constructor requires actual committed attempt authority.
pub(crate) struct LocalProcessPort<'a> {
    artifacts: &'a ArtifactStore,
    gateway: ExecutionGateway,
    profile: &'a FrozenEvaluationProfile,
    spec: &'a RolloutSpec,
    attempt: u16,
    catalog: &'a LocalProcessCatalog,
    scratch_root: &'a Path,
    state_root: &'a Path,
    backend: &'a LocalProcessBackend,
    cancellation: Arc<AtomicBool>,
    retain_stage: &'a (dyn Fn(Vec<ArtifactDigest>) -> Result<(), String> + Send + Sync),
    start_event: EventId,
    claim_fence: u64,
    outbox_id: [u8; 16],
    started: Instant,
    candidate: Option<LocalStageArtifacts>,
    evaluator: Option<LocalStageArtifacts>,
}

impl<'a> LocalProcessPort<'a> {
    /// Checks actual C0 StartRollout admission and the immutable local image before preparing effects.
    #[allow(
        clippy::too_many_arguments,
        reason = "all admitted external owners and frozen bindings stay explicit"
    )]
    pub(crate) fn new(
        artifacts: &'a ArtifactStore,
        processes: ProcessStore,
        profile: &'a FrozenEvaluationProfile,
        spec: &'a RolloutSpec,
        attempt: u16,
        claim: &ExecutionDirectiveClaim,
        started: &CommittedEvaluationTransition,
        catalog: &'a LocalProcessCatalog,
        scratch_root: &'a Path,
        state_root: &'a Path,
        backend: &'a LocalProcessBackend,
        cancellation: Arc<AtomicBool>,
        retain_stage: &'a (dyn Fn(Vec<ArtifactDigest>) -> Result<(), String> + Send + Sync),
    ) -> Result<Self, String> {
        let state = started.state();
        let directive = claim.directive();
        if directive.campaign_id() != spec.campaign_id()
            || directive.rollout_id() != spec.id()
            || directive.kind()
                != (ExecutionDirectiveKind::Execute { request_digest: spec.request_digest() })
            || state.campaign_id() != spec.campaign_id()
            || state.profile_digest() != profile.digest()
            || state
                .rollout(spec.id())
                .is_none_or(|p| p.status() != RolloutStatus::Running { attempt })
            || attempt == 0
            || spec.profile_digest() != profile.digest()
        {
            return Err(
                "local RSI port requires its exact claimed and durably started rollout attempt"
                    .into(),
            );
        }
        let e = profile.execution();
        if e.mode() != EvaluationExecutionMode::LocalProcess
            || !e.require_restricted_isolation()
            || !e.require_complete_teardown()
            || e.image_digest() != catalog.digest()?
            || e.sandbox_plan_digest() != catalog.sandbox_policy_digest()?
            || e.environment_digest() != catalog.environment_digest()
            || e.backend_admission_digest() != backend.descriptor_digest(scratch_root)?
        {
            return Err("local RSI profile differs from the actual frozen catalog/native private-isolation policy".into());
        }
        catalog.validate_resources(e.resource_request(), e.deadline_micros())?;
        catalog.validate_task_isolation(profile)?;
        for p in [catalog.baseline(), catalog.candidate(), catalog.evaluator()] {
            p.verify(artifacts)?;
        }
        let task = profile
            .dataset()
            .tasks()
            .iter()
            .find(|t| t.id() == spec.task_id())
            .ok_or("local RSI task is absent from frozen dataset")?;
        if artifacts.verify(spec.public_input().artifact()).map_err(detail)?.size()
            != spec.public_input().byte_length()
            || artifacts.verify(task.evaluator_input().artifact()).map_err(detail)?.size()
                != task.evaluator_input().byte_length()
            || task.evaluator_input().verifier_digest() != catalog.evaluator().verifier_digest()?
        {
            return Err("local RSI public/sealed input size or independent verifier descriptor differs from frozen task".into());
        }
        Ok(Self {
            artifacts,
            gateway: ExecutionGateway::new(processes),
            profile,
            spec,
            attempt,
            catalog,
            scratch_root,
            state_root,
            backend,
            cancellation,
            retain_stage,
            start_event: state.last_event_id(),
            claim_fence: claim.fence(),
            outbox_id: *peritus_eval::EvaluationDirectiveClaim::Execution(claim.clone())
                .outbox_id()
                .map_err(detail)?
                .as_bytes(),
            started: Instant::now(),
            candidate: None,
            evaluator: None,
        })
    }
    pub(crate) const fn candidate_artifacts(&self) -> Option<LocalStageArtifacts> {
        self.candidate
    }
    pub(crate) const fn evaluator_artifacts(&self) -> Option<LocalStageArtifacts> {
        self.evaluator
    }

    fn stage(
        &self,
        role: &str,
        program: &LocalProgram,
        inputs: ProcessInputs,
    ) -> Result<(ProcessCompletion, LocalStageArtifacts), String> {
        if self.cancellation.load(Ordering::Acquire) {
            return Err("local RSI cancelled before the next process effect".into());
        }
        let mut executable = program.input();
        executable.arguments = self.arguments(&executable.arguments, role == "candidate")?;
        let ordinal =
            u64::from(self.attempt).checked_mul(2).ok_or("local RSI child ordinal overflow")?
                - u64::from(role == "candidate");
        let source_digest = peritus_codec::sha256(
            &[b"peritus.rsi.local-child-source.v1\0".as_slice(), self.spec.id().as_bytes()]
                .concat(),
        );
        let mut run = [0_u8; 16];
        run.copy_from_slice(&source_digest.as_bytes()[..16]);
        run[0] |= 1;
        let source_run = RunId::new(run)
            .map_err(|error| format!("local RSI child source identity: {error:?}"))?;
        let context =
            RestrictedProcessContext::new(self.state_root, source_run, ordinal).map_err(detail)?;
        let mut limits = self.catalog.limits();
        let elapsed = u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let remaining = self
            .profile
            .execution()
            .deadline_micros()
            .checked_sub(elapsed)
            .filter(|r| *r > 0)
            .ok_or("local RSI frozen rollout deadline elapsed before process admission")?;
        limits.wall_millis = limits.wall_millis.min(remaining.div_ceil(1000));
        let prepared = execution::prepare(
            self.artifacts,
            self.scratch_root,
            self.backend.native(),
            context.identity(),
            &executable,
            &inputs,
            limits,
        )?;
        if prepared.plan().backend().descriptor_digest()
            != self.profile.execution().backend_admission_digest()
        {
            return Err("local RSI native backend changed after frozen campaign admission".into());
        }
        self.persist_child_link(role, &prepared, program.verifier_digest()?, source_run, ordinal)?;
        if self.cancellation.load(Ordering::Acquire) {
            return Err("local RSI cancelled before child authority commit".into());
        }
        let authority = context.authorize(prepared.plan()).map_err(detail)?;
        let authorization = authority.request(prepared.plan()).map_err(detail)?;
        if self.cancellation.load(Ordering::Acquire) {
            return Err("local RSI cancelled before native child launch".into());
        }
        let owned = prepared.launch(&self.gateway, &authorization)?;
        let control = owned.control();
        let mut cancel_sent = false;
        while control.terminal_result().is_none() {
            if self.cancellation.load(Ordering::Acquire) && !cancel_sent {
                if let Err(error) = control.cancel(CancellationReason::User)
                    && control.terminal_result().is_none()
                {
                    return Err(detail(error));
                }
                cancel_sent = true;
            }
            let _ =
                control.wait_events(ProcessCursor::after(u64::MAX), 1, Duration::from_millis(50));
        }
        let completion = owned.wait_and_publish(self.artifacts, self.start_event)?;
        if !completion.terminal.tree_cleanup_complete()
            || !completion.terminal.support_tasks_joined()
        {
            return Err(format!(
                "local RSI {role} did not complete native process ownership duties: {:?}",
                completion.terminal.disposition()
            ));
        }
        let stdout = publish(
            self.artifacts,
            &completion.stdout,
            self.start_event,
            "application/octet-stream",
        )?;
        let stderr = publish(
            self.artifacts,
            &completion.stderr,
            self.start_event,
            "application/octet-stream",
        )?;
        let terminal = terminal_envelope(&completion.terminal, stdout, stderr)?;
        let terminal_bytes = terminal.len() as u64;
        let terminal = publish(self.artifacts, &terminal, self.start_event, "application/json")?;
        Ok((completion, LocalStageArtifacts { stdout, stderr, terminal, terminal_bytes }))
    }

    fn arguments(
        &self,
        template: &[ExecutionArgument],
        candidate: bool,
    ) -> Result<Vec<ExecutionArgument>, String> {
        let mut arguments = Vec::new();
        for value in template {
            let ExecutionArgument::Descriptor(value) = value else {
                arguments.push(value.clone());
                continue;
            };
            if value == "{{task_arguments}}" {
                if !candidate {
                    return Err(
                        "independent evaluator cannot use candidate public task argument expansion"
                            .into(),
                    );
                }
                let bytes = self
                    .artifacts
                    .read(self.spec.public_input().artifact(), 16 * 1024 * 1024)
                    .map_err(detail)?;
                let input: serde_json::Value = serde_json::from_slice(&bytes).map_err(detail)?;
                let task_args: Vec<String> = serde_json::from_value(
                    input.get("arguments").cloned().ok_or("public local task lacks arguments")?,
                )
                .map_err(detail)?;
                if task_args.len() > 256
                    || task_args.iter().any(|a| a.len() > 16 * 1024 || a.contains('\0'))
                {
                    return Err(
                        "public local task argument expansion exceeds its literal bounds".into()
                    );
                }
                arguments.extend(task_args.into_iter().map(ExecutionArgument::Literal));
            } else if value == "{{seed}}" {
                arguments.push(ExecutionArgument::Literal(
                    ArtifactDigest::new(*self.spec.seed().as_bytes()).to_hex(),
                ));
            } else if value == "{{rollout_ordinal}}" {
                arguments.push(ExecutionArgument::Literal(self.spec.ordinal().to_string()));
            } else {
                arguments.push(ExecutionArgument::Descriptor(value.clone()));
            }
        }
        if arguments.len() > 256
            || arguments
                .iter()
                .map(|argument| match argument {
                    ExecutionArgument::Descriptor(value) | ExecutionArgument::Literal(value) => {
                        value.len()
                    }
                })
                .sum::<usize>()
                > 64 * 1024
        {
            return Err(
                "expanded local program arguments exceed their frozen operational bound".into()
            );
        }
        Ok(arguments)
    }

    fn persist_child_link(
        &self,
        role: &str,
        prepared: &execution::PreparedExecution,
        program: Sha256Digest,
        source_run: RunId,
        ordinal: u64,
    ) -> Result<(), String> {
        let plan = prepared.plan();
        let id = plan.identity();
        let bytes = serde_json::to_vec(&ChildReceipt {
            version: 1,
            campaign: *self.spec.campaign_id().as_bytes(),
            outbox: self.outbox_id,
            rollout: *self.spec.id().as_bytes(),
            attempt: self.attempt,
            role: role.to_owned(),
            request: *self.spec.request_digest().as_bytes(),
            profile: *self.profile.digest().as_bytes(),
            claim_fence: self.claim_fence,
            start_event: *self.start_event.as_bytes(),
            source_run: *source_run.as_bytes(),
            ordinal,
            execution_run: *id.run_id().as_bytes(),
            action: *id.action_id().as_bytes(),
            process: *id.process_id().as_bytes(),
            plan: *plan.digest().as_bytes(),
            plan_canonical: plan.canonical_bytes().to_vec(),
            sandbox: *plan.sandbox_digest().as_bytes(),
            backend: *plan.backend().descriptor_digest().as_bytes(),
            preparation: *plan.backend().preparation_digest().as_bytes(),
            program: *program.as_bytes(),
            scratch: prepared.scratch_directory().to_path_buf(),
        })
        .map_err(detail)?;
        let directory = self.state_root.join("rsi-child-receipts");
        std::fs::create_dir_all(&directory).map_err(detail)?;
        let mut name =
            ArtifactDigest::from_sha256(peritus_codec::sha256(self.spec.id().as_bytes())).to_hex();
        name.push_str(&format!("-{}-{role}.json", self.attempt));
        let path = directory.join(name);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file=options.open(path).map_err(|e|format!("persist exact RSI child owner (existing owner requires C2 recovery, never relaunch): {e}"))?;
        file.write_all(&bytes).map_err(detail)?;
        file.sync_all().map_err(detail)?;
        std::fs::File::open(directory).map_err(detail)?.sync_all().map_err(detail)
    }

    fn failure(&self, class: InfrastructureFailureClass, message: String) -> ExecutionFailure {
        let bytes = serde_json::to_vec(
            &serde_json::json!({"version":1,"rollout":self.spec.id().as_bytes(),"attempt":self.attempt,"class":format!("{class:?}"),"detail":message}),
        );
        let digest = match bytes {
            Ok(bytes) => publish(self.artifacts, &bytes, self.start_event, "application/json")
                .map(ArtifactDigest::sha256)
                .unwrap_or_else(|_| peritus_codec::sha256(&bytes)),
            Err(_) => peritus_codec::sha256(b"local RSI failure serialization failed"),
        };
        ExecutionFailure::new(class, digest, false)
    }
}

impl RolloutExecutionPort for LocalProcessPort<'_> {
    fn execute_candidate(
        &mut self,
        directive: &CandidateExecutionDirective,
    ) -> Result<CandidateObservation, ExecutionFailure> {
        let result = (|| {
            if directive.rollout_id() != self.spec.id()
                || directive.attempt() != self.attempt
                || directive.request_digest() != self.spec.request_digest()
                || directive.profile_digest() != self.profile.digest()
                || directive.execution() != self.profile.execution()
                || directive.harness() != self.profile.arm(self.spec.arm())
            {
                return Err("candidate directive differs from admitted local rollout".into());
            }
            let program = match directive.arm() {
                EvaluationArm::Baseline => self.catalog.baseline(),
                EvaluationArm::Candidate => self.catalog.candidate(),
            };
            let (completion, artifacts) = self.stage(
                "candidate",
                program,
                ProcessInputs::Candidate { public_task: directive.input().artifact() },
            )?;
            self.candidate = Some(artifacts);
            (self.retain_stage)(vec![artifacts.stdout, artifacts.stderr, artifacts.terminal])?;
            if self.cancellation.load(Ordering::Acquire) {
                return Err("local RSI cancellation won before candidate settlement".into());
            }
            if !completed_numeric_exit(&completion.terminal) {
                return Err(format!(
                    "local RSI candidate stopped without an ordinary complete numeric exit: {:?}",
                    completion.terminal.disposition()
                ));
            }
            CandidateObservation::new(
                directive.rollout_id(),
                directive.attempt(),
                directive.request_digest(),
                artifacts.terminal,
                artifacts.terminal_bytes,
                self.profile.execution().digest(),
                self.profile.provider().digest(),
                resources(&completion.terminal),
            )
            .map_err(detail)
        })();
        result.map_err(|m| self.failure(InfrastructureFailureClass::Execution, m))
    }
    fn execute_evaluator(
        &mut self,
        directive: &EvaluatorExecutionDirective,
    ) -> Result<EvaluatorObservation, ExecutionFailure> {
        let result = (|| {
            let candidate = self
                .candidate
                .ok_or("independent evaluator lacks actual finalized candidate output")?;
            let task = self
                .profile
                .dataset()
                .tasks()
                .iter()
                .find(|t| t.id() == self.spec.task_id())
                .ok_or("sealed local task disappeared")?;
            if directive.rollout_id() != self.spec.id()
                || directive.attempt() != self.attempt
                || directive.request_digest() != self.spec.request_digest()
                || directive.candidate_output() != candidate.terminal
                || directive.candidate_output_bytes() != candidate.terminal_bytes
                || directive.sealed_input() != task.evaluator_input()
                || directive.execution() != self.profile.execution()
            {
                return Err("evaluator directive differs from its separately admitted sealed task and finalized output".into());
            }
            let (completion, artifacts) = self.stage(
                "evaluator",
                self.catalog.evaluator(),
                ProcessInputs::Evaluator {
                    sealed_task: directive.sealed_input().artifact(),
                    candidate_stdout: candidate.stdout,
                    candidate_stderr: candidate.stderr,
                    candidate_terminal: candidate.terminal,
                },
            )?;
            self.evaluator = Some(artifacts);
            (self.retain_stage)(vec![artifacts.stdout, artifacts.stderr, artifacts.terminal])?;
            if self.cancellation.load(Ordering::Acquire) {
                return Err("local RSI cancellation won before evaluator settlement".into());
            }
            if !completed_numeric_exit(&completion.terminal)
                || completion.terminal.os_exit() != &OsExitObservation::Code(0)
            {
                return Err("independent local verifier did not exit successfully".into());
            }
            let verdict: VerifierResult =
                serde_json::from_slice(&completion.stdout).map_err(detail)?;
            let verdict = match (verdict.passed, verdict.class.as_deref()) {
                (true, None) => EvaluatorVerdict::Passed,
                (false, None | Some("incorrect")) => {
                    EvaluatorVerdict::Failed(TaskFailureClass::Incorrect)
                }
                (false, Some("safety")) => EvaluatorVerdict::Failed(TaskFailureClass::Safety),
                (false, Some("incomplete")) => {
                    EvaluatorVerdict::Failed(TaskFailureClass::Incomplete)
                }
                _ => {
                    return Err(
                        "local evaluator verdict has an inconsistent or unknown class".into()
                    );
                }
            };
            EvaluatorObservation::new(
                directive.rollout_id(),
                directive.attempt(),
                directive.request_digest(),
                candidate.terminal,
                verdict,
                artifacts.terminal.sha256(),
                self.profile.execution().digest(),
                resources(&completion.terminal),
            )
            .map_err(detail)
        })();
        result.map_err(|m| self.failure(InfrastructureFailureClass::Evaluator, m))
    }
}

pub(super) fn completed_numeric_exit(terminal: &TerminalResult) -> bool {
    terminal.disposition() == TerminalDisposition::Exited
        && matches!(terminal.os_exit(), OsExitObservation::Code(_))
        && terminal.started_at().is_some()
        && terminal.output().is_complete()
        && terminal.artifact_publication_complete()
        && terminal.tree_cleanup_complete()
        && terminal.support_tasks_joined()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifierResult {
    passed: bool,
    #[serde(default)]
    class: Option<String>,
}

/// Exact pre-effect owner link, retained before committing child authority or launching C2.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChildReceipt {
    pub(crate) version: u16,
    pub(crate) campaign: [u8; 16],
    pub(crate) outbox: [u8; 16],
    pub(crate) rollout: [u8; 16],
    pub(crate) attempt: u16,
    pub(crate) role: String,
    pub(crate) request: [u8; 32],
    pub(crate) profile: [u8; 32],
    pub(crate) claim_fence: u64,
    pub(crate) start_event: [u8; 16],
    pub(crate) source_run: [u8; 16],
    pub(crate) ordinal: u64,
    pub(crate) execution_run: [u8; 16],
    pub(crate) action: [u8; 16],
    pub(crate) process: [u8; 16],
    pub(crate) plan: [u8; 32],
    pub(crate) plan_canonical: Vec<u8>,
    pub(crate) sandbox: [u8; 32],
    pub(crate) backend: [u8; 32],
    pub(crate) preparation: [u8; 32],
    pub(crate) program: [u8; 32],
    pub(crate) scratch: PathBuf,
}

pub(super) fn publish(
    store: &ArtifactStore,
    bytes: &[u8],
    event: EventId,
    media: &str,
) -> Result<ArtifactDigest, String> {
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
    let request = WriteRequest::new(
        digest,
        bytes.len() as u64,
        (bytes.len() as u64).max(1),
        MediaType::new(media).map_err(detail)?,
        EncryptionMetadata::unencrypted(),
        event,
    );
    let mut writer = store.begin_write(request).map_err(detail)?;
    writer.write_chunk(bytes).map_err(detail)?;
    Ok(writer.finalize().map_err(detail)?.digest())
}

pub(super) fn terminal_envelope(
    terminal: &TerminalResult,
    stdout: ArtifactDigest,
    stderr: ArtifactDigest,
) -> Result<Vec<u8>, String> {
    let exit_code = match terminal.os_exit() {
        OsExitObservation::Code(code) => Some(*code),
        _ => None,
    };
    serde_json::to_vec(&serde_json::json!({"version":1,"process_id":terminal.process_id().as_bytes(),"plan_digest":terminal.plan_digest().as_bytes(),"disposition":format!("{:?}",terminal.disposition()),"exit_code":exit_code,"os_exit":format!("{:?}",terminal.os_exit()),"stdout_artifact":stdout.to_hex(),"stderr_artifact":stderr.to_hex(),"elapsed_millis":terminal.ended_at().millis(),"output_complete":terminal.output().is_complete(),"publication_complete":terminal.artifact_publication_complete(),"tree_cleanup_complete":terminal.tree_cleanup_complete(),"support_tasks_joined":terminal.support_tasks_joined(),"resource_observations":terminal.resources().iter().map(|r|serde_json::json!({"dimension":format!("{:?}",r.dimension()),"value":r.value(),"ceiling":r.ceiling(),"fidelity":format!("{:?}",r.fidelity())})).collect::<Vec<_>>()})).map_err(detail)
}

pub(super) fn resources(terminal: &TerminalResult) -> ResourceObservation {
    let observed = |dimension| {
        terminal
            .resources()
            .iter()
            .find(|r| {
                r.dimension() == dimension
                    && matches!(
                        r.fidelity(),
                        ResourceFidelity::Enforced | ResourceFidelity::Sampled
                    )
            })
            .map(|r| r.value())
    };
    ResourceObservation::new(
        terminal.ended_at().millis().saturating_mul(1000),
        Some(0),
        Some(0),
        Some(0),
        observed(ProcessResourceDimension::MemoryBytes),
        observed(ProcessResourceDimension::CpuTimeMilliseconds).and_then(|v| v.checked_mul(1000)),
        observed(ProcessResourceDimension::ProcessCount).and_then(|v| u32::try_from(v).ok()),
        terminal.output().is_complete() && terminal.artifact_publication_complete(),
        terminal.tree_cleanup_complete() && terminal.support_tasks_joined(),
    )
}
fn detail(error: impl std::fmt::Display) -> String {
    format!("local RSI execution: {error}")
}
