//! Native E3 scheduling through replayed, durably committed D3 coordination ownership.
//!
//! These adapters run under the serialized daemon owner. A scheduler start acknowledgement is
//! ownership of a committed reservation, not evidence that a candidate process has succeeded.

use peritus_codec::CodecLimits;
use peritus_eval::{
    EvaluationState, FrozenEvaluationProfile, RolloutOutcome, RolloutRecord, RolloutSpec,
};
use peritus_journal::SqliteJournal;
use peritus_scheduler::{
    DispatchId, ExecutionClass, FailureDisposition, SchedulerBinding, SchedulerCommand,
    SchedulerCommandKind, SchedulerEvent, SchedulerEventKind, SchedulerId, SchedulerLimits,
    SchedulerReservation, SchedulerState, WorkPhase, WorkSpec, WorkTerminal, WorkerDescriptor,
    WorkerId, WorkerPhase,
};
use peritus_types::{ActorId, CommandId, EventId, RunId, Sha256Digest};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

/// Returns independent bounds for the complete frozen native rollout plan.
pub(crate) fn limits(profile: &FrozenEvaluationProfile) -> Result<SchedulerLimits, DaemonError> {
    let selected = profile
        .dataset()
        .tasks()
        .iter()
        .filter(|task| profile.partitions().contains(&task.partition()))
        .count();
    let count = u32::try_from(
        selected
            .checked_mul(2)
            .and_then(|n| n.checked_mul(usize::from(profile.rollouts_per_task())))
            .ok_or_else(|| corrupt("native rollout cardinality overflowed"))?,
    )
    .map_err(owner)?;
    let dimensions =
        u16::try_from(profile.execution().resource_request().entries().len()).map_err(owner)?;
    SchedulerLimits::new(
        count,
        count,
        1,
        1,
        dimensions,
        1,
        profile.retry().maximum_attempts(),
        1,
        1,
        SchedulerLimits::MAX_PAYLOAD_BYTES,
        SchedulerLimits::MAX_STATE_BYTES,
    )
    .map_err(owner)
}

/// Returns the campaign's one native coordination actor, without granting execution authority.
pub fn actor(state: &EvaluationState) -> Result<ActorId, DaemonError> {
    ActorId::new(identity(state, b"actor", &[])).map_err(owner)
}

/// Returns the dedicated campaign scheduler aggregate identity.
pub fn run_id(state: &EvaluationState) -> Result<RunId, DaemonError> {
    RunId::new(identity(state, b"run", &[])).map_err(owner)
}

/// Admits the exact frozen work and returns its real durable admission-event digest.
pub fn schedule(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
    work: &WorkSpec,
) -> Result<Sha256Digest, DaemonError> {
    validate_profile(state, profile, spec)?;
    let expected = expected_work(state, profile, spec)?;
    if work != &expected {
        return Err(corrupt("schedule request differs from the exact frozen native work"));
    }
    let mut scheduler = ensure(journal, state, profile, spec)?;
    match scheduler.work_item(spec.work_id()) {
        Some(retained) if retained.spec() != work => {
            return Err(corrupt("retained native scheduler work differs from its request"));
        }
        Some(_) => {}
        None => {
            scheduler = apply(
                journal,
                state,
                spec,
                &scheduler,
                b"admit",
                SchedulerCommandKind::AdmitWork { spec: work.clone() },
            )?;
        }
    }
    let replay =
        peritus_scheduler::load_scheduler_replay(journal, scheduler.run_id()).map_err(owner)?;
    let event = replay
        .events()
        .iter()
        .find(|event| {
            matches!(event.kind(), SchedulerEventKind::WorkAdmitted { spec } if spec == work)
        })
        .ok_or_else(|| corrupt("native work admission has no retained scheduler event"))?;
    event_digest(event)
}

/// Reserves and acknowledges only the requested exact work under the native worker.
///
/// An existing reservation is checked and reused on replay. The process owner must separately
/// reconcile whether any C2 effect started before attempting another launch.
pub fn begin(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
) -> Result<DispatchId, DaemonError> {
    validate_profile(state, profile, spec)?;
    let mut scheduler = ensure(journal, state, profile, spec)?;
    let work = scheduler
        .work_item(spec.work_id())
        .ok_or_else(|| corrupt("execution has no durably admitted scheduler work"))?;
    if work.spec() != &expected_work(state, profile, spec)? {
        return Err(corrupt("execution work differs from the frozen native request"));
    }
    if let Some(reservation) = reservation_for(&scheduler, spec) {
        validate_reservation(state, spec, &scheduler, reservation)?;
        let dispatch = reservation.dispatch_id();
        if !reservation.started() {
            scheduler = apply(
                journal,
                state,
                spec,
                &scheduler,
                b"start",
                SchedulerCommandKind::AcknowledgeStart { dispatch_id: dispatch },
            )?;
        }
        let retained = scheduler
            .reservation(dispatch)
            .ok_or_else(|| corrupt("acknowledged native reservation disappeared"))?;
        validate_reservation(state, spec, &scheduler, retained)?;
        return Ok(dispatch);
    }
    if work.phase() != WorkPhase::Queued {
        return Err(corrupt("native execution work is neither queued nor reserved"));
    }
    let selected = peritus_scheduler::select_next(&scheduler)
        .ok_or_else(|| waiting("native scheduler has no available capacity for this rollout"))?;
    if selected.work_id() != spec.work_id() || selected.worker_id() != worker_id(state)? {
        return Err(waiting("another retained rollout precedes this native dispatch"));
    }
    let dispatch = DispatchId::new(identity(
        state,
        b"dispatch",
        &[spec.request_digest().as_bytes(), &scheduler.sequence().get().to_be_bytes()],
    ))
    .map_err(owner)?;
    let token = peritus_codec::sha256(&identity(
        state,
        b"dispatch-token",
        &[dispatch.as_bytes(), spec.request_digest().as_bytes()],
    ));
    scheduler = apply(
        journal,
        state,
        spec,
        &scheduler,
        b"reserve",
        SchedulerCommandKind::DispatchNext { dispatch_id: dispatch, dispatch_token: token },
    )?;
    let reservation = scheduler
        .reservation(dispatch)
        .ok_or_else(|| corrupt("committed native dispatch has no reservation"))?;
    validate_reservation(state, spec, &scheduler, reservation)?;
    scheduler = apply(
        journal,
        state,
        spec,
        &scheduler,
        b"start",
        SchedulerCommandKind::AcknowledgeStart { dispatch_id: dispatch },
    )?;
    let reservation = scheduler
        .reservation(dispatch)
        .ok_or_else(|| corrupt("native start acknowledgement lost its reservation"))?;
    validate_reservation(state, spec, &scheduler, reservation)?;
    Ok(dispatch)
}

/// Settles native coordination from the genuine terminal record, preserving exact replay.
///
/// Both evaluator-confirmed task outcomes mean the coordination completed. Infrastructure failure
/// is a D3 failure, and an ambiguous external outcome remains ambiguous. A numeric candidate exit
/// status is not a task verdict and is deliberately absent from this decision.
pub fn finish(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    spec: &RolloutSpec,
    record: &RolloutRecord,
) -> Result<(), DaemonError> {
    record.validate_spec(spec).map_err(owner)?;
    validate_spec(state, spec)?;
    let scheduler = load(journal, state)?;
    let work = scheduler
        .work_item(spec.work_id())
        .ok_or_else(|| corrupt("terminal execution has no native scheduler work"))?;
    validate_retained_work(state, spec, work.spec())?;
    if let Some(terminal) = work.terminal() {
        return if terminal_matches(terminal, record)
            && retained_terminal_matches(journal, state, terminal, record)?
        {
            Ok(())
        } else {
            Err(corrupt("native scheduler terminal differs from the execution record"))
        };
    }
    let reservation = reservation_for(&scheduler, spec)
        .ok_or_else(|| corrupt("terminal execution has no owned native reservation"))?;
    validate_reservation(state, spec, &scheduler, reservation)?;
    if reservation.attempt().get() != record.attempt().number() {
        return Err(corrupt("native reservation attempt differs from execution settlement"));
    }
    let dispatch = reservation.dispatch_id();
    let kind = match record.outcome() {
        RolloutOutcome::TaskPassed { .. } | RolloutOutcome::TaskFailed { .. } => {
            SchedulerCommandKind::CompleteWork {
                dispatch_id: dispatch,
                result_digest: record.digest(),
            }
        }
        RolloutOutcome::InfrastructureFailed { .. } => SchedulerCommandKind::FailWork {
            dispatch_id: dispatch,
            failure_digest: record.digest(),
            disposition: FailureDisposition::Failed,
        },
        RolloutOutcome::Ambiguous { .. } => SchedulerCommandKind::FailWork {
            dispatch_id: dispatch,
            failure_digest: record.digest(),
            disposition: FailureDisposition::Ambiguous,
        },
        RolloutOutcome::Cancelled => {
            let cancelling = if work.phase() == WorkPhase::Cancelling {
                scheduler
            } else {
                apply(
                    journal,
                    state,
                    spec,
                    &scheduler,
                    b"cancel",
                    SchedulerCommandKind::CancelWork { work_id: spec.work_id() },
                )?
            };
            apply(
                journal,
                state,
                spec,
                &cancelling,
                b"cancel-ack",
                SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: dispatch },
            )?;
            return Ok(());
        }
    };
    apply(journal, state, spec, &scheduler, b"finish", kind)?;
    Ok(())
}

/// Cancels retained native work only after the caller has joined any owned C2 process effects.
///
/// Planned work is genuinely admitted and cancelled without launching a process. Already terminal
/// work retains its observed outcome; cancellation cannot rewrite success, failure, or ambiguity.
pub fn cancel(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
) -> Result<Sha256Digest, DaemonError> {
    let work = expected_work(state, profile, spec)?;
    schedule(journal, state, profile, spec, &work)?;
    let mut scheduler = load(journal, state)?;
    let retained = scheduler
        .work_item(spec.work_id())
        .ok_or_else(|| corrupt("cancellation has no retained native work"))?;
    if retained.terminal().is_none() {
        if retained.phase() != WorkPhase::Cancelling {
            scheduler = apply(
                journal,
                state,
                spec,
                &scheduler,
                b"cancel",
                SchedulerCommandKind::CancelWork { work_id: spec.work_id() },
            )?;
        }
        if let Some(reservation) = reservation_for(&scheduler, spec) {
            validate_reservation(state, spec, &scheduler, reservation)?;
            let dispatch = reservation.dispatch_id();
            scheduler = apply(
                journal,
                state,
                spec,
                &scheduler,
                b"cancel-ack",
                SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: dispatch },
            )?;
        }
    }
    if scheduler.work_item(spec.work_id()).and_then(|work| work.terminal()).is_none()
        || reservation_for(&scheduler, spec).is_some()
    {
        return Err(corrupt("native cancellation did not settle retained ownership"));
    }
    terminal_event_digest(journal, state, spec)
}

fn ensure(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
) -> Result<SchedulerState, DaemonError> {
    let binding = binding(state, profile)?;
    let replay =
        peritus_scheduler::load_scheduler_replay(journal, binding.run_id()).map_err(owner)?;
    let mut scheduler = if let Some(retained) = replay.rebuild().map_err(owner)? {
        if retained.binding() != &binding {
            return Err(corrupt("retained native scheduler has another frozen binding"));
        }
        retained
    } else {
        let (command_id, event_id) = operation_ids(state, spec, None, b"genesis")?;
        let command = SchedulerCommand::new(
            command_id,
            event_id,
            binding.run_id(),
            0,
            None,
            Sha256Digest::new([0; 32]),
            *state.revision(),
            SchedulerCommandKind::StartScheduler { binding: binding.clone() },
        )
        .map_err(owner)?;
        let transition = peritus_scheduler::start(&command).map_err(owner)?;
        peritus_scheduler::commit_scheduler_transition(journal, &command, &transition)
            .map_err(owner)?;
        transition.state().clone()
    };
    let descriptor = WorkerDescriptor::new(
        worker_id(state)?,
        actor(state)?,
        vec![ExecutionClass::Coordination],
        profile.execution().resource_request().clone(),
        1,
        binding.limits(),
    )
    .map_err(owner)?;
    match scheduler.worker(descriptor.id()) {
        Some(retained) if retained.descriptor() != &descriptor => {
            return Err(corrupt("retained native worker differs from its frozen descriptor"));
        }
        Some(retained)
            if !matches!(retained.phase(), WorkerPhase::Available | WorkerPhase::Busy) =>
        {
            return Err(corrupt("retained native worker no longer owns executable work"));
        }
        Some(_) => {}
        None => {
            scheduler = apply(
                journal,
                state,
                spec,
                &scheduler,
                b"worker",
                SchedulerCommandKind::RegisterWorker { descriptor },
            )?;
        }
    }
    if scheduler.workers().len() != 1 {
        return Err(corrupt("native campaign scheduler has unexpected additional workers"));
    }
    Ok(scheduler)
}

fn binding(
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
) -> Result<SchedulerBinding, DaemonError> {
    let count = state
        .plan()
        .ok_or_else(|| corrupt("native scheduler requires a complete frozen rollout plan"))?
        .expected_rollouts();
    let limits = limits(profile)?;
    if count != limits.retained_work() {
        return Err(corrupt("retained native plan cardinality differs from its frozen profile"));
    }
    SchedulerBinding::new(
        run_id(state)?,
        SchedulerId::new(identity(state, b"scheduler", &[])).map_err(owner)?,
        *state.revision(),
        limits,
        profile.execution().resource_request().clone(),
    )
    .map_err(owner)
}

fn expected_work(
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
) -> Result<WorkSpec, DaemonError> {
    spec.work_spec(
        actor(state)?,
        *state.revision(),
        profile.execution().resource_request().clone(),
        profile.retry().maximum_attempts(),
        binding(state, profile)?.limits(),
    )
    .map_err(owner)
}

fn validate_profile(
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
) -> Result<(), DaemonError> {
    validate_spec(state, spec)?;
    if state.profile_digest() != profile.digest()
        || state.dataset_digest() != profile.dataset().digest()
    {
        return Err(corrupt("native scheduling differs from the frozen evaluation profile"));
    }
    Ok(())
}

fn validate_spec(state: &EvaluationState, spec: &RolloutSpec) -> Result<(), DaemonError> {
    let retained = state
        .rollout(spec.id())
        .ok_or_else(|| corrupt("rollout is absent from the complete evaluation plan"))?
        .binding();
    if state.campaign_id() != spec.campaign_id()
        || state.profile_digest() != spec.profile_digest()
        || retained.work_id() != spec.work_id()
        || retained.request_digest() != spec.request_digest()
    {
        return Err(corrupt("rollout differs from its exact retained evaluation plan"));
    }
    Ok(())
}

fn validate_retained_work(
    state: &EvaluationState,
    spec: &RolloutSpec,
    work: &WorkSpec,
) -> Result<(), DaemonError> {
    if work.id() != spec.work_id()
        || work.payload_digest() != spec.request_digest()
        || work.owner() != actor(state)?
        || work.revision() != *state.revision()
        || work.class() != ExecutionClass::Coordination
    {
        return Err(corrupt("retained native work identity or owner differs"));
    }
    Ok(())
}

fn validate_reservation(
    state: &EvaluationState,
    spec: &RolloutSpec,
    scheduler: &SchedulerState,
    reservation: &SchedulerReservation,
) -> Result<(), DaemonError> {
    let worker = scheduler
        .worker(worker_id(state)?)
        .ok_or_else(|| corrupt("native reservation has no retained worker"))?;
    let work = scheduler
        .work_item(spec.work_id())
        .ok_or_else(|| corrupt("native reservation has no retained work"))?;
    validate_retained_work(state, spec, work.spec())?;
    if reservation.work_id() != spec.work_id()
        || reservation.worker_id() != worker.descriptor().id()
        || reservation.owner() != actor(state)?
        || reservation.revision() != *state.revision()
        || reservation.resources() != work.spec().request()
        || reservation.attempt().get() != work.attempts_started()
        || !matches!(worker.phase(), WorkerPhase::Available | WorkerPhase::Busy)
        || !matches!(work.phase(), WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling)
    {
        return Err(corrupt("native reservation lost exact active worker ownership"));
    }
    Ok(())
}

fn reservation_for<'a>(
    scheduler: &'a SchedulerState,
    spec: &RolloutSpec,
) -> Option<&'a SchedulerReservation> {
    scheduler.reservations().iter().find(|reservation| reservation.work_id() == spec.work_id())
}

fn terminal_matches(terminal: &WorkTerminal, record: &RolloutRecord) -> bool {
    match (terminal, record.outcome()) {
        (
            WorkTerminal::Succeeded { result_digest },
            RolloutOutcome::TaskPassed { .. } | RolloutOutcome::TaskFailed { .. },
        ) => *result_digest == record.digest(),
        (WorkTerminal::Failed { failure_digest }, RolloutOutcome::InfrastructureFailed { .. }) => {
            *failure_digest == record.digest()
        }
        (WorkTerminal::Cancelled, RolloutOutcome::Cancelled) => true,
        (WorkTerminal::Ambiguous { .. }, RolloutOutcome::Ambiguous { .. }) => true,
        _ => false,
    }
}

fn retained_terminal_matches(
    journal: &SqliteJournal,
    state: &EvaluationState,
    terminal: &WorkTerminal,
    record: &RolloutRecord,
) -> Result<bool, DaemonError> {
    let WorkTerminal::Ambiguous { dispatch_id } = terminal else {
        return Ok(true);
    };
    let replay =
        peritus_scheduler::load_scheduler_replay(journal, run_id(state)?).map_err(owner)?;
    Ok(replay.events().iter().any(|event| {
        matches!(event.kind(), SchedulerEventKind::WorkFailed {
            dispatch_id: actual,
            failure_digest,
            disposition: FailureDisposition::Ambiguous,
        } if actual == dispatch_id && *failure_digest == record.digest())
    }))
}

fn load(journal: &SqliteJournal, state: &EvaluationState) -> Result<SchedulerState, DaemonError> {
    peritus_scheduler::load_scheduler_replay(journal, run_id(state)?)
        .map_err(owner)?
        .rebuild()
        .map_err(owner)?
        .ok_or_else(|| corrupt("native campaign scheduler is absent"))
}

fn apply(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    spec: &RolloutSpec,
    scheduler: &SchedulerState,
    label: &[u8],
    kind: SchedulerCommandKind,
) -> Result<SchedulerState, DaemonError> {
    let (command_id, event_id) = operation_ids(state, spec, Some(scheduler), label)?;
    let command =
        SchedulerCommand::from_state(scheduler, command_id, event_id, kind).map_err(owner)?;
    let transition = peritus_scheduler::decide(scheduler, &command).map_err(owner)?;
    peritus_scheduler::commit_scheduler_transition(journal, &command, &transition)
        .map_err(owner)?;
    Ok(transition.state().clone())
}

fn terminal_event_digest(
    journal: &SqliteJournal,
    state: &EvaluationState,
    spec: &RolloutSpec,
) -> Result<Sha256Digest, DaemonError> {
    let replay =
        peritus_scheduler::load_scheduler_replay(journal, run_id(state)?).map_err(owner)?;
    let dispatches = replay
        .events()
        .iter()
        .filter_map(|event| match event.kind() {
            SchedulerEventKind::WorkReserved { reservation }
                if reservation.work_id() == spec.work_id() =>
            {
                Some(reservation.dispatch_id())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let event = replay
        .events()
        .iter()
        .rev()
        .find(|event| match event.kind() {
            SchedulerEventKind::WorkCancelled { affected, .. } => {
                affected.contains(&spec.work_id())
            }
            SchedulerEventKind::CancellationAcknowledged { dispatch_id }
            | SchedulerEventKind::WorkSucceeded { dispatch_id, .. }
            | SchedulerEventKind::WorkFailed { dispatch_id, .. }
            | SchedulerEventKind::DispatchAbandoned { dispatch_id, .. } => {
                dispatches.contains(dispatch_id)
            }
            _ => false,
        })
        .ok_or_else(|| corrupt("native terminal work has no retained terminal event"))?;
    event_digest(event)
}

fn event_digest(event: &SchedulerEvent) -> Result<Sha256Digest, DaemonError> {
    peritus_scheduler::encode_scheduler_event(event, CodecLimits::PRODUCTION)
        .map(|bytes| peritus_codec::sha256(&bytes))
        .map_err(owner)
}

fn worker_id(state: &EvaluationState) -> Result<WorkerId, DaemonError> {
    WorkerId::new(identity(state, b"worker", &[])).map_err(owner)
}

fn operation_ids(
    state: &EvaluationState,
    spec: &RolloutSpec,
    scheduler: Option<&SchedulerState>,
    label: &[u8],
) -> Result<(CommandId, EventId), DaemonError> {
    let sequence = scheduler.map_or(0, |value| value.sequence().get()).to_be_bytes();
    let predecessor = scheduler.map_or(Sha256Digest::new([0; 32]), SchedulerState::state_digest);
    let request_digest = spec.request_digest();
    let parts = [request_digest.as_bytes().as_slice(), &sequence, predecessor.as_bytes()];
    let command = CommandId::new(identity(state, label, &parts)).map_err(owner)?;
    let event = EventId::new(identity(state, b"event", &[command.as_bytes()])).map_err(owner)?;
    Ok((command, event))
}

fn identity(state: &EvaluationState, label: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let mut bytes = b"PERITUS-RSI-NATIVE-D3\0".to_vec();
    bytes.extend_from_slice(state.campaign_id().as_bytes());
    bytes.extend_from_slice(state.profile_digest().as_bytes());
    bytes.extend_from_slice(&(label.len() as u64).to_be_bytes());
    bytes.extend_from_slice(label);
    for part in parts {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    let digest = peritus_codec::sha256(&bytes);
    let mut result = [0_u8; 16];
    result.copy_from_slice(&digest.as_bytes()[..16]);
    result[0] |= 0x40;
    result
}

fn corrupt(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "reconcile native evaluation scheduler",
        detail,
    )
}

fn waiting(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::ResourceLimit,
        DaemonRecovery::Retry,
        "dispatch native evaluation work",
        detail,
    )
}

fn owner(error: impl std::fmt::Debug) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        "commit native evaluation scheduler",
        format!("{error:?}"),
    )
}
