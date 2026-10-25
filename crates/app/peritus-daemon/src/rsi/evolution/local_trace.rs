//! Imports genuine retained C2 native process observations into C7 without inventing model work.
//!
//! Span timestamps describe observation import. Native execution timings remain the actual C2
//! terminal's measured ticks and resource observations in the finalized source manifest.

use super::{identity, incomplete, owner_error};
use crate::rsi::local::ChildReceipt;
use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_eval::{EvaluationExecutionMode, FrozenEvaluationProfile, RolloutRecord, RolloutSpec};
use peritus_evolution::EvolutionError;
use peritus_journal::SqliteJournal;
use peritus_process::{ProcessResourceDimension, ProcessStore, ResourceFidelity, TerminalResult};
use peritus_product_runner::RestrictedProcessContext;
use peritus_trace::{
    CausalBinding, DiagnosticCode, JournalTraceStore, Observation, ObservationKind, ObservedTime,
    SafeAttribute, SafeAttributeKey, SafeAttributeValue, SpanId, SpanKind, SpanOutcome, StatusCode,
    TraceId,
};
use peritus_types::{AttemptId, CommandId, EnvironmentId, EventId, ProcessId, RunId};
use std::{
    fs,
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Actual C7 subjects and finalized source manifest for a completed native rollout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalTraceReceipt {
    /// Actual candidate observation trace.
    pub candidate_trace: TraceId,
    /// Actual independently executed evaluator observation trace.
    pub evaluator_trace: TraceId,
    /// Candidate environment resolved from its real C2 owner identity.
    pub candidate_environment: EnvironmentId,
    /// Evaluator environment resolved from its real C2 owner identity.
    pub evaluator_environment: EnvironmentId,
    /// Finalized manifest of actual child receipts, C2 terminals, and C7 producing observations.
    pub manifest: ArtifactDigest,
}

/// Imports protected real native child/terminal observations and returns their actual C7 roots.
///
/// The host supplies its configured protected RSI state root and actual process store. The helper
/// accepts no arbitrary receipt path, caller-supplied terminal, or source timestamps.
///
/// # Errors
/// Rejects nonlocal profiles, wrong record/spec, foreign child links, missing actual C2 terminals,
/// incomplete native output/teardown, or any C0/C7/artifact failure.
#[allow(clippy::too_many_arguments, reason = "each real owner and frozen input is explicit")]
pub fn import_local_trace(
    journal: &mut SqliteJournal,
    artifacts: &ArtifactStore,
    processes: &ProcessStore,
    protected_rsi_root: &Path,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
    record: &RolloutRecord,
) -> Result<LocalTraceReceipt, EvolutionError> {
    if !protected_rsi_root.is_absolute()
        || profile.execution().mode() != EvaluationExecutionMode::LocalProcess
    {
        return Err(incomplete("native trace import requires protected local-process owners"));
    }
    record.validate_spec(spec).map_err(owner_error)?;
    let candidate = stage(
        journal,
        artifacts,
        processes,
        protected_rsi_root,
        profile,
        spec,
        record,
        "candidate",
    )?;
    let evaluator = stage(
        journal,
        artifacts,
        processes,
        protected_rsi_root,
        profile,
        spec,
        record,
        "evaluator",
    )?;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schema":"peritus.native-evaluation-trace.v1",
        "campaign":spec.campaign_id().as_bytes(), "rollout":spec.id().as_bytes(),
        "profile":profile.digest().as_bytes(), "request":spec.request_digest().as_bytes(),
        "attempt":record.attempt().number(), "outcome":format!("{:?}",record.outcome()),
        "candidate":candidate.2, "evaluator":evaluator.2,
        "time_scope":"C7 times observe import; native ticks are retained C2 observations",
    }))
    .map_err(owner_error)?;
    let event = EventId::new(identity(&bytes)).map_err(owner_error)?;
    let manifest = peritus_evolution::finalize_evolution_artifact(
        artifacts,
        &bytes,
        peritus_codec::sha256(&bytes),
        event,
    )?
    .artifact_digest();
    Ok(LocalTraceReceipt {
        candidate_trace: candidate.0,
        evaluator_trace: evaluator.0,
        candidate_environment: candidate.1,
        evaluator_environment: evaluator.1,
        manifest,
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "source scope is checked independently for each native role"
)]
fn stage(
    journal: &mut SqliteJournal,
    artifacts: &ArtifactStore,
    processes: &ProcessStore,
    root: &Path,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
    record: &RolloutRecord,
    role: &str,
) -> Result<(TraceId, EnvironmentId, serde_json::Value), EvolutionError> {
    let filename = format!(
        "{}-{}-{role}.json",
        ArtifactDigest::from_sha256(peritus_codec::sha256(spec.id().as_bytes())).to_hex(),
        record.attempt().number()
    );
    let path = root.join("rsi-child-receipts").join(filename);
    let metadata = fs::symlink_metadata(&path).map_err(owner_error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4 * 1024 * 1024
    {
        return Err(incomplete("native child receipt is not a bounded protected regular file"));
    }
    let bytes = fs::read(path).map_err(owner_error)?;
    let child: ChildReceipt = serde_json::from_slice(&bytes).map_err(owner_error)?;
    let context = RestrictedProcessContext::new(
        root,
        RunId::new(child.source_run).map_err(owner_error)?,
        child.ordinal,
    )
    .map_err(owner_error)?;
    let actual = context.identity();
    if child.version != 1
        || child.campaign != *spec.campaign_id().as_bytes()
        || child.rollout != *spec.id().as_bytes()
        || child.profile != *profile.digest().as_bytes()
        || child.request != *spec.request_digest().as_bytes()
        || child.role != role
        || child.attempt != record.attempt().number()
        || child.process != *actual.process_id().as_bytes()
        || child.action != *actual.action_id().as_bytes()
        || child.execution_run != *actual.run_id().as_bytes()
        || child.plan != *peritus_codec::sha256(&child.plan_canonical).as_bytes()
    {
        return Err(incomplete("native child receipt differs from actual E3/C2 owner identities"));
    }
    let terminal = processes
        .terminal_result(ProcessId::new(child.process).map_err(owner_error)?)
        .map_err(owner_error)?;
    verify_terminal(artifacts, &child, &terminal)?;
    let mut key = b"PERITUS-RSI-ACTUAL-NATIVE-C7\0".to_vec();
    key.extend_from_slice(spec.id().as_bytes());
    key.extend_from_slice(role.as_bytes());
    key.extend_from_slice(&child.plan);
    let trace_id = TraceId::new(identity(&key)).map_err(owner_error)?;
    key.extend_from_slice(b"import-span");
    let mut span_bytes = [0_u8; SpanId::LENGTH];
    span_bytes.copy_from_slice(&identity(&key)[..SpanId::LENGTH]);
    if span_bytes == [0; SpanId::LENGTH] {
        span_bytes[SpanId::LENGTH - 1] = 1;
    }
    let span = SpanId::new(span_bytes).map_err(owner_error)?;
    let binding = CausalBinding::session(actual.session_id())
        .with_run(RunId::new(*spec.campaign_id().as_bytes()).map_err(owner_error)?)
        .with_attempt(AttemptId::new(*spec.id().as_bytes()).map_err(owner_error)?)
        .map_err(owner_error)?;
    let existing = peritus_trace::recover_trace(journal, trace_id).map_err(owner_error)?;
    let previous = existing.trace().map_or(0, |trace| trace.observations().len());
    if previous > 3 {
        return Err(incomplete("native observation trace contains unexpected extra events"));
    }
    let base_tick = existing
        .trace()
        .and_then(|trace| trace.observations().last())
        .map_or(0, |row| row.observation().time().monotonic_tick());
    let began = Instant::now();
    let status = if record.outcome().passed() { StatusCode::Success } else { StatusCode::Failure };
    let mut observation_ids = Vec::new();
    let mut positions = Vec::new();
    for sequence_index in 0..3_usize {
        let sequence = u64::try_from(sequence_index + 1).map_err(owner_error)?;
        let mut event_key = key.clone();
        event_key.extend_from_slice(&sequence.to_be_bytes());
        let event = EventId::new(identity(&event_key)).map_err(owner_error)?;
        observation_ids.push(*event.as_bytes());
        if sequence_index < previous {
            let row = &existing
                .trace()
                .ok_or_else(|| incomplete("native imported trace vanished"))?
                .observations()[sequence_index];
            if row.observation().event_id() != event || row.observation().binding() != binding {
                return Err(incomplete(
                    "native imported trace differs from its exact source owner",
                ));
            }
            positions.push(row.journal_position());
            continue;
        }
        let unix = u64::try_from(
            SystemTime::now().duration_since(UNIX_EPOCH).map_err(owner_error)?.as_nanos(),
        )
        .map_err(owner_error)?;
        let tick = base_tick
            .checked_add(u64::try_from(began.elapsed().as_nanos()).map_err(owner_error)?)
            .and_then(|v| v.checked_add(sequence))
            .ok_or_else(|| incomplete("native observation import clock overflowed"))?;
        let kind = match sequence {
            1 => ObservationKind::SpanStarted(SpanKind::Internal),
            2 => ObservationKind::Diagnostic(DiagnosticCode::ResourceObserved),
            _ => ObservationKind::SpanEnded(SpanOutcome::Ok),
        };
        let mut attrs = vec![
            SafeAttribute::new(
                SafeAttributeKey::RetryAttempt,
                SafeAttributeValue::Count(u64::from(child.attempt)),
            ),
            SafeAttribute::new(
                SafeAttributeKey::Recovery,
                SafeAttributeValue::Digest(peritus_codec::sha256(&bytes)),
            ),
            SafeAttribute::new(
                SafeAttributeKey::Status,
                SafeAttributeValue::Status(if sequence == 1 {
                    StatusCode::Pending
                } else {
                    status
                }),
            ),
        ];
        if sequence == 2 {
            if let Some(memory) = terminal
                .resources()
                .iter()
                .filter(|resource| {
                    resource.dimension() == ProcessResourceDimension::MemoryBytes
                        && matches!(
                            resource.fidelity(),
                            ResourceFidelity::Enforced | ResourceFidelity::Sampled
                        )
                })
                .map(|resource| resource.value())
                .max()
            {
                attrs.push(SafeAttribute::new(
                    SafeAttributeKey::MemoryBytes,
                    SafeAttributeValue::Count(memory),
                ));
            }
            if let Some(cpu) = terminal
                .resources()
                .iter()
                .filter(|resource| {
                    resource.dimension() == ProcessResourceDimension::CpuTimeMilliseconds
                        && matches!(
                            resource.fidelity(),
                            ResourceFidelity::Enforced | ResourceFidelity::Sampled
                        )
                })
                .map(|resource| resource.value())
                .max()
            {
                attrs.push(SafeAttribute::new(
                    SafeAttributeKey::CpuNanos,
                    SafeAttributeValue::Count(
                        cpu.checked_mul(1_000_000)
                            .ok_or_else(|| incomplete("native CPU observation overflowed"))?,
                    ),
                ));
            }
        }
        attrs.sort_unstable_by_key(|attribute| attribute.key());
        let causal = if sequence_index == 0 {
            vec![]
        } else {
            vec![EventId::new(observation_ids[sequence_index - 1]).map_err(owner_error)?]
        };
        let observation = Observation::new(
            event,
            trace_id,
            span,
            sequence,
            None,
            causal,
            binding,
            ObservedTime::new(unix, tick).map_err(owner_error)?,
            kind,
            attrs,
            vec![],
        )
        .map_err(owner_error)?;
        let committed = JournalTraceStore::new(journal)
            .record(CommandId::new(identity(&event_key)).map_err(owner_error)?, observation)
            .map_err(owner_error)?;
        positions.push(committed.global_position());
    }
    let outputs=terminal.artifacts().iter().map(|output|serde_json::json!({"stream":format!("{:?}",output.stream()),"sha256":output.digest().as_bytes(),"size":output.size(),"completeness":format!("{:?}",output.completeness())})).collect::<Vec<_>>();
    Ok((
        trace_id,
        actual.environment_id(),
        serde_json::json!({"trace_id":trace_id.as_bytes(),"environment_id":actual.environment_id().as_bytes(),"session_id":actual.session_id().as_bytes(),"child_receipt":child,"child_receipt_sha256":peritus_codec::sha256(&bytes).as_bytes(),"observations":observation_ids,"positions":positions,"terminal":{"process_id":terminal.process_id().as_bytes(),"plan_digest":terminal.plan_digest().as_bytes(),"disposition":format!("{:?}",terminal.disposition()),"started_tick_millis":terminal.started_at().map(peritus_process::ProcessInstant::millis),"ended_tick_millis":terminal.ended_at().millis(),"os_exit":format!("{:?}",terminal.os_exit()),"output_complete":terminal.output().is_complete(),"publication_complete":terminal.artifact_publication_complete(),"tree_cleanup_complete":terminal.tree_cleanup_complete(),"support_tasks_joined":terminal.support_tasks_joined(),"outputs":outputs}}),
    ))
}

fn verify_terminal(
    artifacts: &ArtifactStore,
    child: &ChildReceipt,
    terminal: &TerminalResult,
) -> Result<(), EvolutionError> {
    if terminal.process_id().as_bytes() != &child.process
        || terminal.plan_digest().as_bytes() != &child.plan
        || terminal.started_at().is_none()
        || !terminal.output().is_complete()
        || !terminal.artifact_publication_complete()
        || !terminal.tree_cleanup_complete()
        || !terminal.support_tasks_joined()
    {
        return Err(incomplete(
            "native trace source lacks complete real C2 terminal/output/teardown",
        ));
    }
    for output in terminal.artifacts() {
        let metadata =
            artifacts.verify(ArtifactDigest::from_sha256(output.digest())).map_err(owner_error)?;
        if metadata.size() != output.size() {
            return Err(incomplete("native C2 output artifact size differs"));
        }
    }
    Ok(())
}
