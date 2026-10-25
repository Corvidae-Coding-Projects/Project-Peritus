//! First-order observation import from actual published native terminal records.

use super::{identity, incomplete, owner_error};
use peritus_artifact_store::ArtifactStore;
use peritus_eval::{
    EvaluationCommandKind, EvaluationEventKind, EvaluationExecutionMode, EvaluationPhase,
    FrozenEvaluationProfile, RolloutOutcome, RolloutRecord, RolloutSpec, RolloutStatus,
};
use peritus_evolution::EvolutionError;
use peritus_journal::SqliteJournal;
use peritus_trace::{
    CausalBinding, DiagnosticCode, JournalTraceStore, Observation, ObservationKind, ObservedTime,
    SafeAttribute, SafeAttributeKey, SafeAttributeValue, SpanId, SpanKind, SpanOutcome, StatusCode,
    TraceId,
};
use peritus_types::{AttemptId, CommandId, EnvironmentId, EventId, RunId, SessionId};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub(super) struct LocalFailureObservation {
    pub trace: TraceId,
    pub environment: EnvironmentId,
}

/// Observes an actual retained C0 infrastructure terminal. This changes no E3 record or metric.
/// C7 identifies this observer import, never a missing C2 child, output, measurement, or session.
pub(super) fn import_published_failure_trace(
    journal: &mut SqliteJournal,
    artifacts: &ArtifactStore,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
    record: &RolloutRecord,
) -> Result<LocalFailureObservation, EvolutionError> {
    record.validate_spec(spec).map_err(owner_error)?;
    if profile.execution().mode() != EvaluationExecutionMode::LocalProcess
        || record.trace_root().is_some()
        || !matches!(
            record.outcome(),
            RolloutOutcome::InfrastructureFailed { .. } | RolloutOutcome::Ambiguous { .. }
        )
    {
        return Err(incomplete(
            "native error observation requires an actual untraced infrastructure outcome",
        ));
    }
    let replay =
        peritus_eval::load_evaluation_replay(journal, spec.campaign_id()).map_err(owner_error)?;
    let state = replay
        .rebuild()
        .map_err(owner_error)?
        .ok_or_else(|| incomplete("native error has no actual E3 owner"))?;
    if state.phase() != EvaluationPhase::Published || state.profile_digest() != profile.digest() {
        return Err(incomplete("native error observation requires exact published E3 inputs"));
    }
    let terminal = match state.rollout(spec.id()).map(peritus_eval::RolloutProgress::status) {
        Some(RolloutStatus::Settled(terminal)) => terminal,
        _ => return Err(incomplete("native error has no actual settled terminal")),
    };
    let bytes =
        artifacts.read(terminal.artifact(), terminal.artifact_bytes()).map_err(owner_error)?;
    let actual = RolloutRecord::from_canonical_bytes(&bytes).map_err(owner_error)?;
    if actual != *record || actual.digest() != terminal.record_digest() {
        return Err(incomplete("native error differs from its actual protected terminal record"));
    }
    let error = terminal.artifact();
    let event = replay.events().iter().rev().find(|event| matches!(event.kind(),
        EvaluationEventKind::Accepted(EvaluationCommandKind::SettleRollout { rollout_id, terminal: settled })
            if *rollout_id == spec.id() && *settled == terminal))
        .ok_or_else(|| incomplete("native error has no actual owning settlement event"))?;
    let source_event = event.id();
    let mut key = b"PERITUS-RSI-ACTUAL-FAILURE-OBSERVATION-C7\0".to_vec();
    key.extend_from_slice(spec.id().as_bytes());
    key.extend_from_slice(error.as_bytes());
    key.extend_from_slice(source_event.as_bytes());
    let trace = TraceId::new(identity(&key)).map_err(owner_error)?;
    key.extend_from_slice(b"observer-session");
    let session = SessionId::new(identity(&key)).map_err(owner_error)?;
    key.extend_from_slice(b"observer-environment");
    let environment = EnvironmentId::new(identity(&key)).map_err(owner_error)?;
    key.extend_from_slice(b"observer-span");
    let mut span_bytes = [0; SpanId::LENGTH];
    span_bytes.copy_from_slice(&identity(&key)[..SpanId::LENGTH]);
    if span_bytes == [0; SpanId::LENGTH] {
        span_bytes[SpanId::LENGTH - 1] = 1;
    }
    let span = SpanId::new(span_bytes).map_err(owner_error)?;
    let binding = CausalBinding::session(session)
        .with_run(RunId::new(*spec.campaign_id().as_bytes()).map_err(owner_error)?)
        .with_attempt(AttemptId::new(*spec.id().as_bytes()).map_err(owner_error)?)
        .map_err(owner_error)?;
    let recovered = peritus_trace::recover_trace(journal, trace).map_err(owner_error)?;
    let previous = recovered.trace().map_or(0, |value| value.observations().len());
    if previous > 3 {
        return Err(incomplete("native failure observation trace has unexpected extra events"));
    }
    let clock = Instant::now();
    let base = recovered
        .trace()
        .and_then(|value| value.observations().last())
        .map_or(0, |row| row.observation().time().monotonic_tick());
    let mut events = Vec::new();
    for number in 1u64..=3 {
        let mut event_key = key.clone();
        event_key.extend_from_slice(&number.to_be_bytes());
        let event = EventId::new(identity(&event_key)).map_err(owner_error)?;
        let kind = match number {
            1 => ObservationKind::SpanStarted(SpanKind::Internal),
            // The observer has recovered the exact durable C0 terminal, irrespective of the
            // original process outcome. This describes the readback, never a repaired process.
            2 => ObservationKind::Diagnostic(DiagnosticCode::RecoveryCompleted),
            _ => ObservationKind::SpanEnded(SpanOutcome::Ok),
        };
        let mut attributes = vec![
            SafeAttribute::new(
                SafeAttributeKey::RetryAttempt,
                SafeAttributeValue::Count(u64::from(record.attempt().number())),
            ),
            SafeAttribute::new(
                SafeAttributeKey::Recovery,
                SafeAttributeValue::Digest(error.sha256()),
            ),
            SafeAttribute::new(
                SafeAttributeKey::Status,
                SafeAttributeValue::Status(StatusCode::InfrastructureFailure),
            ),
        ];
        attributes.sort_unstable_by_key(|value| value.key());
        let observation_index = usize::try_from(number - 1).map_err(owner_error)?;
        if observation_index < previous {
            let row = &recovered
                .trace()
                .ok_or_else(|| incomplete("native failure trace disappeared"))?
                .observations()[observation_index];
            if row.observation().event_id() != event
                || row.observation().binding() != binding
                || row.observation().kind() != kind
                || row.observation().attributes() != attributes
            {
                return Err(incomplete(
                    "native failure C7 observations differ from actual retained error",
                ));
            }
            events.push(*event.as_bytes());
            continue;
        }
        let unix = u64::try_from(
            SystemTime::now().duration_since(UNIX_EPOCH).map_err(owner_error)?.as_nanos(),
        )
        .map_err(owner_error)?;
        let tick = base
            .checked_add(u64::try_from(clock.elapsed().as_nanos()).map_err(owner_error)?)
            .and_then(|v| v.checked_add(number))
            .ok_or_else(|| incomplete("native failure observer clock overflow"))?;
        let parents = events
            .last()
            .map(|bytes| EventId::new(*bytes).map_err(owner_error))
            .transpose()?
            .into_iter()
            .collect();
        let observation = Observation::new(
            event,
            trace,
            span,
            number,
            None,
            parents,
            binding,
            ObservedTime::new(unix, tick).map_err(owner_error)?,
            kind,
            attributes,
            vec![],
        )
        .map_err(owner_error)?;
        let command = CommandId::new(identity(&event_key)).map_err(owner_error)?;
        JournalTraceStore::new(journal).record(command, observation).map_err(owner_error)?;
        events.push(*event.as_bytes());
    }
    Ok(LocalFailureObservation { trace, environment })
}
