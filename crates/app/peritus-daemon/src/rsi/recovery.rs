//! Startup recovery from real C0 attempt admission and exact C2 child ownership.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_eval::*;
use peritus_journal::{OutboxId, SqliteJournal};
use peritus_process::{
    NativeProcessProbe, OsExitObservation, OutputStream, ProcessStore, RecoveryDisposition,
    TerminalResult,
};
use peritus_product_runner::RestrictedProcessContext;
use peritus_types::{ActionId, EventId, ProcessId, RunId, Sha256Digest};
use rusqlite::{Connection, OpenFlags, params};
use std::{
    fs,
    time::{Duration, Instant},
};

use super::{
    evaluation as e,
    execution::{self, LocalProcessSandbox},
    local::{self, ChildReceipt, LocalProcessCatalog, LocalProgram, LocalStageArtifacts},
    scheduling,
};
use crate::{DaemonConfig, DaemonError};

/// Reconciles only RSI attempts with real current claimed C0 execution messages.
///
/// This runs under the exclusive startup journal owner before ordinary intake/dispatch. It never
/// reclaims unrelated work, relaunches an old process, or converts observed absence into success.
pub(crate) fn recover_owned(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    processes: &ProcessStore,
    config: &DaemonConfig,
) -> Result<(), DaemonError> {
    let connection =
        Connection::open_with_flags(config.paths().database(), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(e::error)?;
    let mut query=connection.prepare("SELECT outbox_id FROM outbox WHERE destination=?1 AND state=2 ORDER BY outbox_id LIMIT 16385").map_err(e::error)?;
    let rows = query
        .query_map(params![EXECUTION_DESTINATION], |row| row.get::<_, Vec<u8>>(0))
        .map_err(e::error)?;
    let mut ids = Vec::new();
    for row in rows {
        let bytes: [u8; 16] = row
            .map_err(e::error)?
            .try_into()
            .map_err(|_| e::invalid("stored native execution outbox identity is malformed"))?;
        ids.push(OutboxId::new(bytes).map_err(e::error)?);
        if ids.len() > 16384 {
            return Err(e::invalid(
                "native execution recovery exceeds its independent message bound",
            ));
        }
    }
    drop(query);
    drop(connection);
    for id in ids {
        let message = journal.outbox_message(id).map_err(e::error)?.ok_or_else(|| {
            e::invalid("claimed native execution disappeared during serialized startup")
        })?;
        let claim = ExecutionDirectiveClaim::from_message(&message).map_err(e::error)?;
        recover_claim(journal, store, processes, config, &claim, None)?;
    }
    Ok(())
}

/// Settles an actually admitted attempt after exact C2 reconciliation.
///
/// The requested claim must still be the current durable outbox claim. A diagnostic can identify
/// an infrastructure failure only when neither child has a before-effect ownership receipt;
/// otherwise missing terminal stages remain ambiguous. Existing processes are never relaunched.
pub(crate) fn recover_claim(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    processes: &ProcessStore,
    config: &DaemonConfig,
    requested: &ExecutionDirectiveClaim,
    diagnostic: Option<ArtifactDigest>,
) -> Result<(), DaemonError> {
    let id =
        EvaluationDirectiveClaim::Execution(requested.clone()).outbox_id().map_err(e::error)?;
    let message = journal
        .outbox_message(id)
        .map_err(e::error)?
        .ok_or_else(|| e::invalid("native recovery claim disappeared before reconciliation"))?;
    let claim = ExecutionDirectiveClaim::from_message(&message).map_err(e::error)?;
    if claim != *requested {
        return Err(e::invalid("native recovery claim has been fenced or replaced"));
    }
    let directive = claim.directive();
    let state = e::load(journal, directive.campaign_id())?;
    let Some(progress) = state.rollout(directive.rollout_id()) else {
        return Err(e::invalid("claimed native execution has no planned rollout"));
    };
    let RolloutStatus::Running { attempt } = progress.status() else {
        return Ok(());
    };
    if !matches!(directive.kind(), ExecutionDirectiveKind::Execute { .. }) {
        return Ok(());
    }
    let profile = e::profile(store, &state)?;
    if profile.execution().mode() != EvaluationExecutionMode::LocalProcess {
        return Ok(());
    }
    e::validate_profile(journal, store, &profile)?;
    let plan = EvaluationPlan::build(state.campaign_id(), &profile).map_err(e::error)?;
    let spec = plan
        .specs()
        .iter()
        .find(|s| s.id() == directive.rollout_id())
        .ok_or_else(|| e::invalid("native recovery directive is outside its frozen plan"))?;
    if directive.kind()
        != (ExecutionDirectiveKind::Execute { request_digest: spec.request_digest() })
    {
        return Err(e::invalid("native recovery request differs from its exact frozen rollout"));
    }
    let catalog = LocalProcessCatalog::from_canonical_bytes(&e::read(
        store,
        ArtifactDigest::from_sha256(profile.execution().image_digest()),
        1024 * 1024,
    )?)
    .map_err(e::error)?;
    let candidate_program = match spec.arm() {
        EvaluationArm::Baseline => catalog.baseline(),
        EvaluationArm::Candidate => catalog.candidate(),
    };
    let replay = load_evaluation_replay(journal, state.campaign_id()).map_err(e::error)?;
    let start=replay.events().iter().find(|event|matches!(event.kind(),EvaluationEventKind::Accepted(EvaluationCommandKind::StartRollout {rollout_id,attempt:started,..}) if *rollout_id==spec.id() && *started==attempt)).ok_or_else(||e::invalid("running native attempt has no actual committed StartRollout event"))?;
    let mut observed = Vec::new();
    let mut stages = Vec::new();
    let mut receipts_seen = false;
    for (role, program) in [("candidate", candidate_program), ("evaluator", catalog.evaluator())] {
        let receipt = read_receipt(config, spec, attempt, role)?;
        let stage = if let Some(receipt) = receipt {
            receipts_seen = true;
            validate_receipt(
                &receipt,
                spec,
                attempt,
                role,
                program,
                id,
                start.id(),
                claim.fence(),
                &profile,
                config,
            )?;
            let (observation, terminal) = recover_child(processes, &receipt, config)?;
            observed.push(observation);
            terminal
                .map(|terminal| completed_stage(store, processes, &receipt, terminal, start.id()))
                .transpose()?
                .flatten()
        } else {
            observed.push(serde_json::json!({"role":role,"owner_receipt":"absent-before-effect","reason":"native gateway requires durable child receipt before any process launch"}));
            None
        };
        stages.push(stage);
    }
    let bytes=serde_json::to_vec(&serde_json::json!({"version":1,"campaign":state.campaign_id().as_bytes(),"rollout":spec.id().as_bytes(),"attempt":attempt,"actual_start_event":start.id().as_bytes(),"diagnostic":diagnostic.map(ArtifactDigest::to_hex),"observations":observed})).map_err(e::error)?;
    let trace = e::save(store, &bytes, start.id())?;
    let mut roots = vec![trace];
    roots.extend(diagnostic);
    for stage in stages.iter().flatten() {
        roots.extend([stage.artifacts.stdout, stage.artifacts.stderr, stage.artifacts.terminal]);
    }
    let state = e::retain(journal, store, state.campaign_id(), roots, Some(claim.clone().into()))?;
    if state.phase() == EvaluationPhase::Cancelling {
        // Physical stage artifacts were already retained above; logical cancellation won
        // durably and may not be rewritten into either success or an invented task failure.
        let observation_digest = scheduling::cancel(journal, &state, &profile, spec)?;
        let mut data = trace.as_bytes().to_vec();
        data.extend_from_slice(observation_digest.as_bytes());
        let command = e::command(
            &state,
            EvaluationCommandKind::SettleCancellation {
                rollout_id: spec.id(),
                observation_digest: peritus_codec::sha256(&data),
            },
            "recover-cancelled-child",
        )?;
        let transition = decide(Some(&state), &command).map_err(e::error)?;
        commit_evaluation_settlement(journal, &command, &transition, claim).map_err(e::error)?;
        if e::cancellation_ready(transition.state()) {
            e::ordinary(
                journal,
                transition.state(),
                EvaluationCommandKind::CompleteCancellation,
                "recover-cancel-complete",
            )?;
        }
        return Ok(());
    }
    let record = match (&stages[0], &stages[1]) {
        (Some(candidate), _) if !local::completed_numeric_exit(&candidate.terminal) => {
            RolloutRecord::from_execution_failure(
                spec,
                attempt,
                ExecutionFailure::new(
                    InfrastructureFailureClass::Execution,
                    candidate.artifacts.terminal.sha256(),
                    false,
                ),
                false,
            )
            .map_err(e::error)?
        }
        (Some(candidate), Some(evaluator)) => {
            completed_record(spec, &profile, attempt, candidate, evaluator, trace)?
        }
        _ if !receipts_seen && diagnostic.is_some() => RolloutRecord::from_execution_failure(
            spec,
            attempt,
            ExecutionFailure::new(
                InfrastructureFailureClass::Execution,
                diagnostic
                    .ok_or_else(|| {
                        e::invalid("known pre-effect failure lost its actual diagnostic")
                    })?
                    .sha256(),
                false,
            ),
            false,
        )
        .map_err(e::error)?,
        _ => RolloutRecord::from_interruption(spec, attempt, trace.sha256(), false)
            .map_err(e::error)?,
    };
    super::authority::finish(
        journal,
        store,
        processes,
        &config.paths().state_root().join("rsi"),
        claim,
        record,
    )?;
    Ok(())
}

fn read_receipt(
    config: &DaemonConfig,
    spec: &RolloutSpec,
    attempt: u16,
    role: &str,
) -> Result<Option<ChildReceipt>, DaemonError> {
    let name = format!(
        "{}-{attempt}-{role}.json",
        ArtifactDigest::from_sha256(peritus_codec::sha256(spec.id().as_bytes())).to_hex()
    );
    let path = config.paths().state_root().join("rsi/rsi-child-receipts").join(name);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(e::error(error)),
    };
    if !metadata.file_type().is_file() || metadata.len() > 2 * 1024 * 1024 {
        return Err(e::invalid(
            "native child receipt is aliased or exceeds its independent recovery bound",
        ));
    }
    serde_json::from_slice(&fs::read(path).map_err(e::error)?).map(Some).map_err(e::error)
}

#[allow(
    clippy::too_many_arguments,
    reason = "every actual parent, child, and immutable input binding remains explicit"
)]
fn validate_receipt(
    receipt: &ChildReceipt,
    spec: &RolloutSpec,
    attempt: u16,
    role: &str,
    program: &LocalProgram,
    outbox: OutboxId,
    start_event: EventId,
    fence: u64,
    profile: &FrozenEvaluationProfile,
    config: &DaemonConfig,
) -> Result<(), DaemonError> {
    let ordinal = u64::from(attempt) * 2 - u64::from(role == "candidate");
    let digest = peritus_codec::sha256(
        &[b"peritus.rsi.local-child-source.v1\0".as_slice(), spec.id().as_bytes()].concat(),
    );
    let mut source = [0_u8; 16];
    source.copy_from_slice(&digest.as_bytes()[..16]);
    source[0] |= 1;
    let source_run = RunId::new(source).map_err(e::error)?;
    let context =
        RestrictedProcessContext::new(config.paths().state_root().join("rsi"), source_run, ordinal)
            .map_err(e::error)?;
    let expected = context.identity();
    let scratch_root = config.paths().workspace_root().join("rsi");
    if receipt.version != 1
        || receipt.campaign != *spec.campaign_id().as_bytes()
        || receipt.outbox != *outbox.as_bytes()
        || receipt.rollout != *spec.id().as_bytes()
        || receipt.attempt != attempt
        || receipt.role != role
        || receipt.request != *spec.request_digest().as_bytes()
        || receipt.profile != *profile.digest().as_bytes()
        || receipt.claim_fence == 0
        || receipt.claim_fence > fence
        || receipt.start_event != *start_event.as_bytes()
        || receipt.source_run != source
        || receipt.ordinal != ordinal
        || receipt.execution_run != *expected.run_id().as_bytes()
        || receipt.action != *expected.action_id().as_bytes()
        || receipt.process != *expected.process_id().as_bytes()
        || receipt.program != *program.verifier_digest().map_err(e::error)?.as_bytes()
        || peritus_codec::sha256(&receipt.plan_canonical).as_bytes() != &receipt.plan
        || receipt.backend != *profile.execution().backend_admission_digest().as_bytes()
        || receipt.scratch.parent() != Some(scratch_root.as_path())
        || !receipt.scratch.file_name().is_some_and(|n| n.to_string_lossy().starts_with("rollout-"))
    {
        return Err(e::invalid(
            "native child receipt differs from its actual admitted parent or frozen executable",
        ));
    }
    // Version-one C2 plans end in the native binding. Compare field positions rather than
    // accepting a digest that happens to appear in an unrelated argument or identity field.
    let bytes = &receipt.plan_canonical;
    let mut suffix = vec![0, 1]; // no C4 caller binding, Restricted execution
    suffix.extend_from_slice(&receipt.sandbox);
    for token in ["peritus-linux", "1"] {
        suffix.extend_from_slice(&(token.len() as u32).to_be_bytes());
        suffix.extend_from_slice(token.as_bytes());
    }
    suffix.push(1); // native backend
    let length = suffix.len() + 1 + 96;
    if !bytes.starts_with(b"peritus.execution-plan.v1\0")
        || bytes.len() < length
        || bytes[bytes.len() - length..bytes.len() - 97] != suffix
        || !matches!(bytes[bytes.len() - 97], 1 | 2)
        || bytes[bytes.len() - 96..bytes.len() - 64] != receipt.backend
        || bytes[bytes.len() - 32..] != receipt.preparation
    {
        return Err(e::invalid(
            "native child security fields differ from the exact canonical C2 native binding",
        ));
    }
    Ok(())
}

fn recover_child(
    processes: &ProcessStore,
    receipt: &ChildReceipt,
    config: &DaemonConfig,
) -> Result<(serde_json::Value, Option<TerminalResult>), DaemonError> {
    let run = RunId::new(receipt.execution_run).map_err(e::error)?;
    let action = ActionId::new(receipt.action).map_err(e::error)?;
    let process = ProcessId::new(receipt.process).map_err(e::error)?;
    let name: String = receipt.process.iter().map(|b| format!("{b:02x}")).collect();
    let absent = |path: &std::path::Path| -> Result<bool, DaemonError> {
        match fs::symlink_metadata(path) {
            Ok(_) => Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(e::error(error)),
        }
    };
    if absent(&processes.root().join("claims-v1").join(format!("{name}.claim")))?
        && absent(&processes.root().join("manifests-v1").join(format!("{name}.manifest")))?
        && processes.quarantined_records().is_empty()
    {
        // No current durable consumption exists: the process either never reached C2 or its
        // already-settled terminal was retired. Neither absence proves a successful task.
        // A populated native leaf contradicts absence and the cleanup helper rejects it.
        cleanup_private(receipt, config)?;
        return Ok((
            serde_json::json!({"role":receipt.role,"process":receipt.process,
            "disposition":"NoCurrentC2Record","claim_absent":true,"manifest_absent":true,
            "quarantine_empty":true,"task_outcome":"unobserved"}),
            None,
        ));
    }
    let mut probe = NativeProcessProbe::new();
    let began = Instant::now();
    let observed = loop {
        let observed =
            processes.reconcile_exact(run, action, process, &mut probe).map_err(e::error)?;
        if matches!(
            observed.disposition(),
            RecoveryDisposition::Terminal | RecoveryDisposition::AbsentUnobserved
        ) {
            break observed;
        }
        if began.elapsed() >= Duration::from_secs(3) {
            return Err(e::invalid(
                "native child process ownership remains indeterminate after bounded exact recovery",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    cleanup_private(receipt, config)?;
    let terminal = if observed.disposition() == RecoveryDisposition::Terminal {
        let terminal = processes.terminal_result(process).map_err(e::error)?;
        if terminal.plan_digest().as_bytes() != &receipt.plan {
            return Err(e::invalid("native child terminal belongs to another consumed plan"));
        }
        Some(terminal)
    } else {
        None
    };
    Ok((
        serde_json::json!({"role":receipt.role,"run":receipt.execution_run,"action":receipt.action,"process":receipt.process,"disposition":format!("{:?}",observed.disposition()),"signal_sent":observed.signal_sent(),"cgroup_drained":true}),
        terminal,
    ))
}

fn cleanup_private(receipt: &ChildReceipt, config: &DaemonConfig) -> Result<(), DaemonError> {
    let native = config.rsi().ok_or_else(|| {
        e::invalid("interrupted native child requires its original delegated native configuration")
    })?;
    #[cfg(target_os = "linux")]
    execution::cleanup_empty_preparation(
        &LocalProcessSandbox {
            bubblewrap: native.bubblewrap.clone(),
            helper: native.linux_helper.clone(),
            cgroup_root: native.cgroup_root.clone(),
        },
        Sha256Digest::new(receipt.preparation),
    )
    .map_err(e::error)?;
    #[cfg(not(target_os = "linux"))]
    {
        let _ = native;
        return Err(e::invalid("native local child recovery requires its original Linux host"));
    }
    // Only this exact private scratch owner is retired after native absence and cgroup drain.
    match fs::symlink_metadata(&receipt.scratch) {
        Ok(m) if m.file_type().is_dir() && !m.file_type().is_symlink() => {
            fs::remove_dir_all(&receipt.scratch).map_err(e::error)?
        }
        Ok(_) => {
            return Err(e::invalid("native child scratch owner changed identity during recovery"));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(e::error(error)),
    }
    Ok(())
}

struct RecoveredStage {
    terminal: TerminalResult,
    stdout: Vec<u8>,
    artifacts: LocalStageArtifacts,
}
fn completed_stage(
    store: &ArtifactStore,
    processes: &ProcessStore,
    receipt: &ChildReceipt,
    terminal: TerminalResult,
    event: EventId,
) -> Result<Option<RecoveredStage>, DaemonError> {
    if !terminal.tree_cleanup_complete()
        || !terminal.support_tasks_joined()
        || !terminal.output().is_complete()
    {
        return Ok(None);
    }
    let process = ProcessId::new(receipt.process).map_err(e::error)?;
    let terminal = if terminal.artifact_publication_complete() {
        terminal
    } else {
        processes.retry_artifact_publication(process, store, event).map_err(e::error)?
    };
    let run = RunId::new(receipt.execution_run).map_err(e::error)?;
    let action = ActionId::new(receipt.action).map_err(e::error)?;
    let read = |stream| -> Result<Vec<u8>, DaemonError> {
        let (total, bytes) = processes
            .spooled_stream_range_exact(run, action, process, stream, 0, 16 * 1024 * 1024)
            .map_err(e::error)?;
        if total != bytes.len() as u64 {
            return Err(e::invalid("recovered native stream exceeds its exact bounded output"));
        }
        Ok(bytes)
    };
    let stdout = read(OutputStream::Stdout)?;
    let stderr = read(OutputStream::Stderr)?;
    let stdout_root =
        local::publish(store, &stdout, event, "application/octet-stream").map_err(e::error)?;
    let stderr_root =
        local::publish(store, &stderr, event, "application/octet-stream").map_err(e::error)?;
    let bytes = local::terminal_envelope(&terminal, stdout_root, stderr_root).map_err(e::error)?;
    let terminal_root =
        local::publish(store, &bytes, event, "application/json").map_err(e::error)?;
    Ok(Some(RecoveredStage {
        terminal,
        stdout,
        artifacts: LocalStageArtifacts {
            stdout: stdout_root,
            stderr: stderr_root,
            terminal: terminal_root,
            terminal_bytes: bytes.len() as u64,
        },
    }))
}

fn completed_record(
    spec: &RolloutSpec,
    profile: &FrozenEvaluationProfile,
    attempt: u16,
    candidate: &RecoveredStage,
    evaluator: &RecoveredStage,
    trace: ArtifactDigest,
) -> Result<RolloutRecord, DaemonError> {
    let task = profile
        .dataset()
        .tasks()
        .iter()
        .find(|t| t.id() == spec.task_id())
        .ok_or_else(|| e::invalid("recovered local task is absent"))?;
    if !local::completed_numeric_exit(&candidate.terminal)
        || !local::completed_numeric_exit(&evaluator.terminal)
        || evaluator.terminal.os_exit() != &OsExitObservation::Code(0)
    {
        return RolloutRecord::from_interruption(spec, attempt, trace.sha256(), false)
            .map_err(e::error);
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Verdict {
        passed: bool,
        #[serde(default)]
        class: Option<String>,
    }
    let verdict: Verdict = match serde_json::from_slice(&evaluator.stdout) {
        Ok(v) => v,
        Err(_) => {
            return RolloutRecord::from_interruption(spec, attempt, trace.sha256(), false)
                .map_err(e::error);
        }
    };
    let evaluator_verdict = match (verdict.passed, verdict.class.as_deref()) {
        (true, None) => EvaluatorVerdict::Passed,
        (false, None | Some("incorrect")) => EvaluatorVerdict::Failed(TaskFailureClass::Incorrect),
        (false, Some("safety")) => EvaluatorVerdict::Failed(TaskFailureClass::Safety),
        (false, Some("incomplete")) => EvaluatorVerdict::Failed(TaskFailureClass::Incomplete),
        _ => {
            return RolloutRecord::from_interruption(spec, attempt, trace.sha256(), false)
                .map_err(e::error);
        }
    };
    let candidate_observation = CandidateObservation::new(
        spec.id(),
        attempt,
        spec.request_digest(),
        candidate.artifacts.terminal,
        candidate.artifacts.terminal_bytes,
        profile.execution().digest(),
        profile.provider().digest(),
        local::resources(&candidate.terminal),
    )
    .map_err(e::error)?;
    let evaluator_observation = EvaluatorObservation::new(
        spec.id(),
        attempt,
        spec.request_digest(),
        candidate.artifacts.terminal,
        evaluator_verdict,
        evaluator.artifacts.terminal.sha256(),
        profile.execution().digest(),
        local::resources(&evaluator.terminal),
    )
    .map_err(e::error)?;
    // Sealed task verification is already performed by the frozen executable, whose exact
    // descriptor and input were matched before its real C2 effect in the retained receipt.
    let _ = task;
    let executed =
        ExecutedRollout::from_observed_stages(candidate_observation, evaluator_observation)
            .map_err(e::error)?;
    RolloutRecord::from_execution(spec, executed, Some(trace), Some(trace)).map_err(e::error)
}
