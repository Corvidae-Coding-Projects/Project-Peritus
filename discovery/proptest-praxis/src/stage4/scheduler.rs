//! Exhaustive feasible-pair oracle and durable scheduler history consumer.
use std::collections::{BTreeMap, BTreeSet};

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_scheduler::{
    AttemptNumber, DispatchId, ExecutionClass, FailureDisposition, RecoveryPolicy, ResourceEntry,
    ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding, SchedulerCommand,
    SchedulerCommandKind as Command, SchedulerErrorKind as Error, SchedulerId, SchedulerLimits,
    SchedulerPhase, SchedulerSession, SchedulerState, WorkId, WorkPhase, WorkSpec, WorkTerminal,
    WorkerDescriptor, WorkerId, WorkerPhase, commit_scheduler_transition, decide,
    load_scheduler_replay, pending_directives, select_next, start,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub capacity: [u8; 2],
    pub concurrency: u8,
    pub active_limit: u8,
    pub bypass_limit: u8,
    pub operations: Vec<Operation>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: u8,
    pub owner: u8,
    pub class: u8,
    pub priority: u8,
    pub request: [u8; 2],
    pub dependencies: Vec<u8>,
    pub parent: Option<u8>,
    #[serde(default = "default_recovery")]
    pub recovery: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Admit(Job),
    Dispatch(u8),
    Start(u8),
    Finish { dispatch: u8, outcome: u8 },
    Retry(u8),
    Cancel { work: u8, tree: bool },
    AcknowledgeCancellation(u8),
    Worker { worker: u8, available: bool },
    LoseWorker(u8),
    Pause(bool),
    ExternalToggle,
    StaleFence,
    ReplayLast,
    Reopen,
    Read,
}
fn job_strategy() -> BoxedStrategy<Job> {
    (
        0_u8..16,
        0_u8..3,
        0_u8..5,
        any::<u8>(),
        1_u8..10,
        0_u8..10,
        prop::collection::vec(0_u8..16, 0..=3),
        prop::option::of(0_u8..16),
        0_u8..3,
    )
        .prop_map(|(id, owner, class, priority, cpu, memory, dependencies, parent, recovery)| Job {
            id,
            owner,
            class,
            priority,
            request: [cpu, memory],
            dependencies: dependencies.into_iter().collect::<BTreeSet<_>>().into_iter().collect(),
            parent,
            recovery,
        })
        .boxed()
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let operation = prop_oneof![
        8=>job_strategy().prop_map(Operation::Admit),
        5=>(0_u8..16).prop_map(Operation::Dispatch),
        4=>(0_u8..16).prop_map(Operation::Start),
        5=>(0_u8..16,0_u8..4).prop_map(|(dispatch,outcome)|Operation::Finish{dispatch,outcome}),
        2=>(0_u8..16).prop_map(Operation::Retry),
        2=>(0_u8..16,any::<bool>()).prop_map(|(work,tree)|Operation::Cancel{work,tree}),
        2=>(0_u8..16).prop_map(Operation::AcknowledgeCancellation),
        2=>(0_u8..4,any::<bool>()).prop_map(|(worker,available)|Operation::Worker{worker,available}),
        2=>(0_u8..4).prop_map(Operation::LoseWorker),
        1=>any::<bool>().prop_map(Operation::Pause),1=>Just(Operation::ExternalToggle),
        1=>Just(Operation::StaleFence),1=>Just(Operation::ReplayLast),
        2=>Just(Operation::Reopen),1=>Just(Operation::Read),
    ];
    let mixed = (
        1_u8..9,
        1_u8..9,
        1_u8..4,
        1_u8..5,
        1_u8..4,
        prop::collection::vec(operation.clone(), 0..=max_steps.min(128)),
    )
        .prop_map(|(cpu, memory, concurrency, active_limit, bypass_limit, operations)| Case {
            capacity: [cpu, memory],
            concurrency: concurrency.min(active_limit),
            active_limit,
            bypass_limit,
            operations,
        });
    let constructive = (
        2_u8..9,
        2_u8..9,
        any::<bool>(),
        any::<bool>(),
        prop::collection::vec(operation.clone(), 0..=max_steps.saturating_sub(14).min(114)),
    )
        .prop_map(|(cpu, memory, reopen, retry, mut suffix)| {
            let mut case = calibration();
            case.capacity = [cpu, memory];
            if retry {
                case.operations = vec![
                    Operation::Admit(job(0, 1)),
                    Operation::Dispatch(0),
                    Operation::Finish { dispatch: 0, outcome: 0 },
                    Operation::Finish { dispatch: 0, outcome: 1 },
                    Operation::Retry(0),
                    Operation::Dispatch(1),
                    Operation::Start(1),
                    Operation::Cancel { work: 0, tree: false },
                    Operation::AcknowledgeCancellation(1),
                    Operation::Reopen,
                    Operation::ReplayLast,
                ];
            }
            if reopen {
                case.operations.insert(1, Operation::Reopen);
            }
            case.operations.append(&mut suffix);
            case
        });
    let aging = (
        1_u8..4,
        0_u8..64,
        128_u8..=255,
        prop::collection::vec(operation.clone(), 0..=max_steps.saturating_sub(18).min(110)),
    )
        .prop_map(|(bypass, low, high, mut suffix)| {
            let mut case = aging_case(bypass, low, high);
            case.operations.append(&mut suffix);
            case
        });
    let loss = (
        0_u8..3,
        any::<bool>(),
        any::<bool>(),
        prop::collection::vec(operation, 0..=max_steps.saturating_sub(32).min(96)),
    )
        .prop_map(|(recovery, cancel, started, mut suffix)| {
            let mut case = loss_case(recovery, cancel, started);
            case.operations.append(&mut suffix);
            case
        });
    if max_steps < 18 {
        mixed.boxed()
    } else if max_steps < 32 {
        prop_oneof![2=>mixed,1=>constructive,1=>aging].boxed()
    } else {
        prop_oneof![2=>mixed,1=>constructive,1=>aging,1=>loss].boxed()
    }
}
fn bytes(value: u64) -> [u8; 16] {
    u128::from(value + 1).to_be_bytes()
}
fn debug(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}
fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value.wrapping_add(1); 32])
}
fn actor(value: u8) -> Result<ActorId, String> {
    ActorId::new(bytes(u64::from(value))).map_err(debug)
}
fn work(value: u8) -> Result<WorkId, String> {
    WorkId::new(bytes(u64::from(value))).map_err(debug)
}
fn worker(value: u8) -> Result<WorkerId, String> {
    WorkerId::new(bytes(u64::from(value))).map_err(debug)
}
fn dispatch(value: u8) -> Result<DispatchId, String> {
    DispatchId::new(bytes(u64::from(value))).map_err(debug)
}
fn revision() -> Result<RevisionTuple, String> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(bytes(0)).map_err(debug)?,
        HarnessId::new(bytes(0)).map_err(debug)?,
        WorkspaceId::new(bytes(0)).map_err(debug)?,
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(0)).map_err(debug)?,
        ProviderProfileId::new(bytes(0)).map_err(debug)?,
    ))
}
fn vector(value: [u8; 2]) -> Result<ResourceVector, String> {
    ResourceVector::new(
        value
            .into_iter()
            .enumerate()
            .filter(|(_, quantity)| *quantity > 0)
            .map(|(index, quantity)| {
                Ok(ResourceEntry::new(
                    if index == 0 { ResourceKind::CPU } else { ResourceKind::MEMORY_BYTES },
                    ResourceQuantity::new(u64::from(quantity)).map_err(debug)?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?,
        2,
    )
    .map_err(debug)
}
fn class(value: u8) -> ExecutionClass {
    match value {
        0 => ExecutionClass::Model,
        1 => ExecutionClass::Tool,
        2 => ExecutionClass::Gate,
        3 => ExecutionClass::Review,
        _ => ExecutionClass::Coordination,
    }
}
fn default_recovery() -> u8 {
    1
}
fn recovery(value: u8) -> RecoveryPolicy {
    match value {
        0 => RecoveryPolicy::RetrySafe,
        1 => RecoveryPolicy::Ambiguous,
        _ => RecoveryPolicy::Fail,
    }
}
fn limits(case: &Case) -> Result<SchedulerLimits, String> {
    SchedulerLimits::new(
        12,
        24,
        4,
        4,
        2,
        u16::from(case.active_limit),
        3,
        u16::from(case.bypass_limit),
        4,
        1_048_576,
        1_048_576,
    )
    .map_err(debug)
}
fn spec(job: &Job, case: &Case) -> Result<WorkSpec, String> {
    WorkSpec::new(
        work(job.id)?,
        actor(job.owner)?,
        revision()?,
        class(job.class),
        job.priority,
        vector(job.request)?,
        None,
        job.dependencies.iter().copied().map(work).collect::<Result<_, _>>()?,
        job.parent.map(work).transpose()?,
        AttemptNumber::new(3).map_err(debug)?,
        recovery(job.recovery),
        digest(job.id),
        limits(case)?,
    )
    .map_err(debug)
}
fn bump(metrics: &mut crate::Metrics, name: &str) {
    *metrics.entry(name.into()).or_default() += 1;
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum End {
    Success(u8),
    Failed(u8),
    Ambiguous(u8),
    Exhausted(u8),
    Cancelled,
    Dependency(u8),
    LostFailed(u8),
    LostExhausted(u8),
}
#[derive(Clone)]
struct Entry {
    job: Job,
    phase: u8,
    ordinal: u64,
    bypass: u16,
    attempts: u16,
    end: Option<End>,
    retry: Option<u8>,
}
#[derive(Clone)]
struct Reservation {
    work: u8,
    worker: u8,
    started: bool,
    attempt: u16,
}
#[derive(Clone)]
struct Model {
    jobs: BTreeMap<u8, Entry>,
    reservations: BTreeMap<u8, Reservation>,
    used: BTreeSet<u8>,
    draining: [bool; 3],
    lost: [bool; 3],
    lost_dispatches: BTreeSet<u8>,
    paused: bool,
    sequence: u64,
    enqueue: u64,
    dispatches: u64,
}
impl Model {
    fn new() -> Self {
        Self {
            jobs: BTreeMap::new(),
            reservations: BTreeMap::new(),
            used: BTreeSet::new(),
            draining: [false; 3],
            lost: [false; 3],
            lost_dispatches: BTreeSet::new(),
            paused: false,
            sequence: 1,
            enqueue: 0,
            dispatches: 0,
        }
    }
    fn usage(&self, only: Option<u8>) -> [u64; 2] {
        let mut sum = [0; 2];
        for reservation in
            self.reservations.values().filter(|r| only.is_none_or(|id| r.worker == id))
        {
            let request = self.jobs[&reservation.work].job.request;
            for index in 0..2 {
                sum[index] += u64::from(request[index]);
            }
        }
        sum
    }
    fn worker_capacity(case: &Case, id: u8) -> [u8; 2] {
        if id == 0 { [case.capacity[0], case.capacity[1].div_ceil(2)] } else { case.capacity }
    }
    fn feasible(&self, case: &Case) -> Vec<(u8, u8)> {
        let used = self.usage(None);
        let mut pairs = Vec::new();
        for (id, entry) in &self.jobs {
            if entry.phase != 1 {
                continue;
            }
            for worker in 0_u8..3 {
                let owner = if worker == 1 { 1 } else { 0 };
                let local = self.usage(Some(worker));
                let capacity = Self::worker_capacity(case, worker);
                let count = self.reservations.values().filter(|r| r.worker == worker).count();
                let fits = (0..2).all(|i| {
                    used[i] + u64::from(entry.job.request[i]) <= u64::from(case.capacity[i])
                        && local[i] + u64::from(entry.job.request[i]) <= u64::from(capacity[i])
                });
                if !self.draining[usize::from(worker)]
                    && !self.lost[usize::from(worker)]
                    && entry.job.owner == owner
                    && (worker != 0 || entry.job.class < 2)
                    && count < usize::from(case.concurrency)
                    && fits
                {
                    pairs.push((*id, worker));
                }
            }
        }
        pairs
    }
    fn selected(&self, case: &Case) -> Option<(u8, u8)> {
        self.feasible(case).into_iter().min_by_key(|(id, worker)| {
            let e = &self.jobs[id];
            (
                e.bypass < u16::from(case.bypass_limit),
                std::cmp::Reverse(e.job.priority),
                e.ordinal,
                *id,
                *worker,
            )
        })
    }
    fn refresh(&mut self) {
        loop {
            let mut changes = Vec::new();
            for (id, entry) in &self.jobs {
                if entry.phase > 1 {
                    continue;
                }
                if let Some(dependency) = entry.job.dependencies.iter().find(|dependency| {
                    self.jobs.get(dependency).is_some_and(|e| {
                        e.end.as_ref().is_some_and(|end| !matches!(end, End::Success(_)))
                    })
                }) {
                    changes.push((*id, Some(*dependency)));
                } else if entry.phase == 0
                    && entry.job.dependencies.iter().all(|dependency| {
                        self.jobs
                            .get(dependency)
                            .is_some_and(|e| matches!(e.end, Some(End::Success(_))))
                    })
                {
                    changes.push((*id, None));
                }
            }
            if changes.is_empty() {
                break;
            }
            for (id, failed) in changes {
                if let Some(e) = self.jobs.get_mut(&id) {
                    if let Some(dependency) = failed {
                        e.phase = 6;
                        e.end = Some(End::Dependency(dependency));
                    } else {
                        e.phase = 1;
                    }
                }
            }
        }
    }
    fn apply(&mut self, case: &Case, operation: &Operation) -> Result<(), Error> {
        match operation {
            Operation::Admit(job) => {
                if job.dependencies.windows(2).any(|pair| pair[0] >= pair[1])
                    || job.dependencies.contains(&job.id)
                    || job.parent == Some(job.id)
                {
                    return Err(Error::NonCanonical);
                }
                if self.jobs.len() >= 24
                    || self
                        .jobs
                        .values()
                        .filter(|e| {
                            matches!(e.phase, 0 | 1 | 4)
                                || (matches!(e.phase, 2 | 3) && e.attempts < 3)
                        })
                        .count()
                        >= 12
                {
                    return Err(Error::LimitExceeded);
                }
                if self.jobs.contains_key(&job.id) {
                    return Err(Error::IdentityConflict);
                }
                if (0..2).any(|i| job.request[i] > case.capacity[i]) {
                    return Err(Error::ResourceConflict);
                }
                if job.dependencies.iter().any(|id| !self.jobs.contains_key(id))
                    || job.parent.is_some_and(|id| !self.jobs.contains_key(&id))
                {
                    return Err(Error::UnknownIdentity);
                }
                if job.owner > 1 {
                    return Err(Error::InvalidInput);
                }
                self.enqueue += 1;
                self.jobs.insert(
                    job.id,
                    Entry {
                        job: job.clone(),
                        phase: if job.dependencies.is_empty() { 1 } else { 0 },
                        ordinal: self.enqueue,
                        bypass: 0,
                        attempts: 0,
                        end: None,
                        retry: None,
                    },
                );
            }
            Operation::Dispatch(id) => {
                if self.paused {
                    return Err(Error::IllegalTransition);
                }
                if self.reservations.len() >= usize::from(case.active_limit) {
                    return Err(Error::LimitExceeded);
                }
                if self.used.contains(id) {
                    return Err(Error::IdentityConflict);
                }
                let feasible = self.feasible(case);
                let (id_work, id_worker) = self.selected(case).ok_or(Error::NoFeasibleWork)?;
                let e = self.jobs.get_mut(&id_work).ok_or(Error::UnknownIdentity)?;
                if e.attempts >= 3 {
                    return Err(Error::LimitExceeded);
                }
                e.attempts += 1;
                e.phase = 2;
                e.retry = None;
                self.reservations.insert(
                    *id,
                    Reservation {
                        work: id_work,
                        worker: id_worker,
                        started: false,
                        attempt: e.attempts,
                    },
                );
                self.used.insert(*id);
                self.dispatches += 1;
                for (id_work, entry) in &mut self.jobs {
                    if feasible.iter().any(|(id, _)| id == id_work) {
                        entry.bypass = if *id_work == self.reservations[id].work {
                            0
                        } else {
                            (entry.bypass + 1).min(u16::from(case.bypass_limit))
                        };
                    }
                }
            }
            Operation::Start(id) => {
                let r = self.reservations.get_mut(id).ok_or(Error::UnknownIdentity)?;
                if r.started {
                    return Err(Error::IllegalTransition);
                }
                r.started = true;
                self.jobs.get_mut(&r.work).ok_or(Error::UnknownIdentity)?.phase = 3;
            }
            Operation::Finish { dispatch: id, outcome } => {
                let r = self.reservations.get(id).ok_or(Error::UnknownIdentity)?.clone();
                let e = self.jobs.get_mut(&r.work).ok_or(Error::UnknownIdentity)?;
                if (*outcome == 0 && (!r.started || e.phase != 3))
                    || (*outcome != 0 && !matches!(e.phase, 2 | 3))
                {
                    return Err(Error::IllegalTransition);
                }
                self.reservations.remove(id);
                e.retry = None;
                if *outcome == 1 && e.attempts < 3 {
                    e.phase = 4;
                    e.retry = Some(*id);
                } else {
                    e.phase = 6;
                    e.end = Some(match outcome {
                        0 => End::Success(*id),
                        1 => End::Exhausted(*id),
                        2 => End::Failed(*id),
                        _ => End::Ambiguous(*id),
                    });
                }
            }
            Operation::Retry(id) => {
                let e = self.jobs.get_mut(id).ok_or(Error::UnknownIdentity)?;
                if e.phase != 4 || e.attempts >= 3 {
                    return Err(Error::IllegalTransition);
                }
                e.phase = 1;
                e.retry = None;
            }
            Operation::Cancel { work: id, tree } => {
                let e = self.jobs.get(id).ok_or(Error::UnknownIdentity)?;
                if e.phase == 6 {
                    return Err(Error::IllegalTransition);
                }
                let mut affected = BTreeSet::from([*id]);
                if *tree {
                    loop {
                        let old = affected.len();
                        for (id, e) in &self.jobs {
                            if e.job.parent.is_some_and(|parent| affected.contains(&parent)) {
                                affected.insert(*id);
                            }
                        }
                        if old == affected.len() {
                            break;
                        }
                    }
                }
                for id in affected {
                    let e = self.jobs.get_mut(&id).ok_or(Error::UnknownIdentity)?;
                    if e.phase == 6 {
                        continue;
                    }
                    if matches!(e.phase, 2 | 3 | 5) {
                        e.phase = 5;
                    } else {
                        e.phase = 6;
                        e.end = Some(End::Cancelled);
                        e.retry = None;
                    }
                }
            }
            Operation::AcknowledgeCancellation(id) => {
                let r = self.reservations.get(id).ok_or(Error::UnknownIdentity)?.clone();
                let e = self.jobs.get_mut(&r.work).ok_or(Error::UnknownIdentity)?;
                if e.phase != 5 {
                    return Err(Error::IllegalTransition);
                }
                e.phase = 6;
                e.end = Some(End::Cancelled);
                self.reservations.remove(id);
            }
            Operation::Worker { worker: id, available } => {
                if *id >= 3 {
                    return Err(Error::UnknownIdentity);
                }
                let index = usize::from(*id);
                if *available {
                    if (!self.draining[index] && !self.lost[index])
                        || self.reservations.values().any(|r| r.worker == *id)
                    {
                        return Err(Error::IllegalTransition);
                    }
                    self.draining[index] = false;
                    self.lost[index] = false;
                } else {
                    if self.draining[index] || self.lost[index] {
                        return Err(Error::IllegalTransition);
                    }
                    self.draining[index] = true;
                }
            }
            Operation::LoseWorker(id) => {
                if *id >= 3 {
                    return Err(Error::UnknownIdentity);
                }
                if self.lost[usize::from(*id)] {
                    return Err(Error::IllegalTransition);
                }
                let released = self
                    .reservations
                    .iter()
                    .filter(|(_, r)| r.worker == *id)
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>();
                for dispatch in released {
                    let reservation =
                        self.reservations.remove(&dispatch).ok_or(Error::UnknownIdentity)?;
                    let row = self.jobs.get_mut(&reservation.work).ok_or(Error::UnknownIdentity)?;
                    self.lost_dispatches.insert(dispatch);
                    if row.phase == 5 {
                        row.phase = 6;
                        row.end = Some(End::Cancelled);
                        row.retry = None;
                    } else {
                        match row.job.recovery {
                            0 if row.attempts < 3 => row.phase = 1,
                            0 => {
                                row.phase = 6;
                                row.end = Some(End::LostExhausted(dispatch));
                                row.retry = None;
                            }
                            1 => {
                                row.phase = 6;
                                row.end = Some(End::Ambiguous(dispatch));
                                row.retry = None;
                            }
                            _ => {
                                row.phase = 6;
                                row.end = Some(End::LostFailed(dispatch));
                                row.retry = None;
                            }
                        }
                    }
                }
                self.lost[usize::from(*id)] = true;
                self.draining[usize::from(*id)] = false;
            }
            Operation::Pause(paused) => {
                if self.paused == *paused {
                    return Err(Error::IllegalTransition);
                }
                self.paused = *paused;
            }
            Operation::ExternalToggle => self.paused = !self.paused,
            Operation::StaleFence => return Err(Error::StaleFence),
            Operation::Read | Operation::Reopen | Operation::ReplayLast => return Ok(()),
        }
        self.refresh();
        self.sequence += 1;
        Ok(())
    }
}

fn phase(value: u8) -> WorkPhase {
    match value {
        0 => WorkPhase::WaitingDependencies,
        1 => WorkPhase::Queued,
        2 => WorkPhase::Reserved,
        3 => WorkPhase::Running,
        4 => WorkPhase::RetryPending,
        5 => WorkPhase::Cancelling,
        _ => WorkPhase::Terminal,
    }
}
fn terminal(value: &End) -> Result<WorkTerminal, String> {
    Ok(match value {
        End::Success(id) => WorkTerminal::Succeeded { result_digest: digest(*id) },
        End::Failed(id) => WorkTerminal::Failed { failure_digest: digest(*id) },
        End::Exhausted(id) => WorkTerminal::Exhausted { cause_digest: digest(*id) },
        End::Ambiguous(id) => WorkTerminal::Ambiguous { dispatch_id: dispatch(*id)? },
        End::Cancelled => WorkTerminal::Cancelled,
        End::Dependency(id) => WorkTerminal::DependencyFailed { dependency: work(*id)? },
        End::LostFailed(id) => WorkTerminal::Failed {
            failure_digest: peritus_codec::sha256(dispatch(*id)?.as_bytes()),
        },
        End::LostExhausted(id) => WorkTerminal::Exhausted {
            cause_digest: peritus_codec::sha256(dispatch(*id)?.as_bytes()),
        },
    })
}
fn vector_matches(actual: &ResourceVector, expected: [u8; 2]) -> bool {
    actual.entries().iter().map(|entry| (entry.kind().tag(), entry.quantity().get())).eq([
        (1, u64::from(expected[0])),
        (2, u64::from(expected[1])),
    ]
    .into_iter()
    .filter(|(_, quantity)| *quantity > 0))
}
fn compare(
    case: &Case,
    model: &Model,
    state: &SchedulerState,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    if state.sequence().get() != model.sequence
        || state.enqueue_ordinal() != model.enqueue
        || state.dispatch_ordinal() != model.dispatches
        || state.phase()
            != if model.paused { SchedulerPhase::Paused } else { SchedulerPhase::Active }
        || state.work().len() != model.jobs.len()
        || state.reservations().len() != model.reservations.len()
    {
        return Err("scheduler head/count/phase differs from independent history".into());
    }
    for (id, entry) in &model.jobs {
        let actual = state.work_item(work(*id)?).ok_or("retained work missing")?;
        let definition = actual.spec();
        if definition.id() != work(*id)?
            || definition.owner() != actor(entry.job.owner)?
            || definition.class() != class(entry.job.class)
            || definition.priority() != entry.job.priority
            || !vector_matches(definition.request(), entry.job.request)
            || definition.dependencies()
                != entry
                    .job
                    .dependencies
                    .iter()
                    .copied()
                    .map(work)
                    .collect::<Result<Vec<_>, _>>()?
            || definition.parent() != entry.job.parent.map(work).transpose()?
            || definition.maximum_attempts().get() != 3
            || definition.recovery() != recovery(entry.job.recovery)
            || definition.revision() != revision()?
            || definition.budget_reservation().is_some()
            || definition.payload_digest() != digest(*id)
            || actual.phase() != phase(entry.phase)
            || actual.enqueue_ordinal() != entry.ordinal
            || actual.bypasses() != entry.bypass
            || actual.attempts_started() != entry.attempts
            || actual.terminal() != entry.end.as_ref().map(terminal).transpose()?.as_ref()
            || actual.retry_cause() != entry.retry.map(digest)
        {
            return Err(format!("work {id} diverged: {actual:?}"));
        }
    }
    for (id, reservation) in &model.reservations {
        let actual = state.reservation(dispatch(*id)?).ok_or("reservation missing")?;
        let job = &model.jobs[&reservation.work].job;
        if actual.work_id() != work(reservation.work)?
            || actual.worker_id() != worker(reservation.worker)?
            || actual.owner() != actor(job.owner)?
            || actual.started() != reservation.started
            || actual.attempt().get() != reservation.attempt
            || !vector_matches(actual.resources(), job.request)
            || actual.revision() != revision()?
            || actual.dispatch_token() != digest(*id)
        {
            return Err("reservation ownership/resources/binding diverged".into());
        }
    }
    let used = state.used_resources().map_err(debug)?;
    let expected = model.usage(None);
    for (index, kind) in [ResourceKind::CPU, ResourceKind::MEMORY_BYTES].into_iter().enumerate() {
        if used.as_ref().map_or(0, |used| used.quantity(kind)) != expected[index] {
            return Err("global resource total differs from full reservation recomputation".into());
        }
    }
    if state.used_dispatches()
        != model.used.iter().copied().map(dispatch).collect::<Result<Vec<_>, _>>()?
    {
        return Err("historical dispatch identity was lost or reused".into());
    }
    for id in 0_u8..3 {
        let actual = state.worker(worker(id)?).ok_or("worker missing")?;
        let descriptor = actual.descriptor();
        let expected_classes =
            if id == 0 { vec![class(0), class(1)] } else { (0..5).map(class).collect() };
        if descriptor.id() != worker(id)?
            || descriptor.owner() != actor(if id == 1 { 1 } else { 0 })?
            || descriptor.classes() != expected_classes
            || descriptor.concurrency() != u16::from(case.concurrency)
            || !vector_matches(descriptor.capacity(), Model::worker_capacity(case, id))
        {
            return Err("immutable worker descriptor diverged from selected fixture".into());
        }
        let count = model.reservations.values().filter(|r| r.worker == id).count();
        let expected = if model.lost[usize::from(id)] {
            WorkerPhase::Lost
        } else if model.draining[usize::from(id)] {
            WorkerPhase::Draining
        } else if count >= usize::from(case.concurrency) {
            WorkerPhase::Busy
        } else {
            WorkerPhase::Available
        };
        if actual.phase() != expected {
            return Err("worker availability differs from retained ownership".into());
        }
        for index in 0..2 {
            if model.usage(Some(id))[index] > u64::from(Model::worker_capacity(case, id)[index]) {
                return Err("worker reservation sum exceeds capacity".into());
            }
        }
    }
    let expected = model
        .selected(case)
        .map(|(id, owner)| Ok::<_, String>((work(id)?, worker(owner)?)))
        .transpose()?;
    if select_next(state).map(|selected| (selected.work_id(), selected.worker_id())) != expected {
        return Err("selected pair differs from exhaustive feasible-pair minimum".into());
    }
    if model.feasible(case).len() > 1 {
        bump(metrics, "multiple_feasible_pairs");
    }
    if !model.reservations.is_empty()
        && model
            .usage(None)
            .iter()
            .zip(case.capacity)
            .any(|(used, capacity)| *used == u64::from(capacity))
    {
        bump(metrics, "saturated_resource_observations");
    }
    let expected_directives = model
        .reservations
        .iter()
        .filter(|(_, r)| {
            let phase = model.jobs[&r.work].phase;
            (phase == 2 && !r.started) || phase == 5
        })
        .map(|(id, _)| dispatch(*id))
        .collect::<Result<Vec<_>, _>>()?;
    let directives = pending_directives(state)
        .into_iter()
        .map(|directive| match directive {
            peritus_scheduler::SchedulerDirective::Dispatch(reservation) => {
                reservation.dispatch_id()
            }
            peritus_scheduler::SchedulerDirective::Cancel { dispatch_id, .. } => dispatch_id,
        })
        .collect::<Vec<_>>();
    if directives != expected_directives {
        return Err("pending durable directives differ from retained reservation phases".into());
    }
    bump(metrics, "independent_state_comparisons");
    bump(metrics, "exhaustive_pair_queries");
    Ok(())
}
fn actual_kind(case: &Case, model: &Model, operation: &Operation) -> Result<Command, String> {
    Ok(match operation {
        Operation::Admit(job) => Command::AdmitWork { spec: spec(job, case)? },
        Operation::Dispatch(id) => {
            Command::DispatchNext { dispatch_id: dispatch(*id)?, dispatch_token: digest(*id) }
        }
        Operation::Start(id) => Command::AcknowledgeStart { dispatch_id: dispatch(*id)? },
        Operation::Finish { dispatch: id, outcome: 0 } => {
            Command::CompleteWork { dispatch_id: dispatch(*id)?, result_digest: digest(*id) }
        }
        Operation::Finish { dispatch: id, outcome } => Command::FailWork {
            dispatch_id: dispatch(*id)?,
            failure_digest: digest(*id),
            disposition: match outcome {
                1 => FailureDisposition::Retryable,
                2 => FailureDisposition::Failed,
                _ => FailureDisposition::Ambiguous,
            },
        },
        Operation::Retry(id) => Command::RetryWork { work_id: work(*id)? },
        Operation::Cancel { work: id, tree: true } => {
            Command::CancelWorkTree { work_id: work(*id)? }
        }
        Operation::Cancel { work: id, tree: false } => Command::CancelWork { work_id: work(*id)? },
        Operation::AcknowledgeCancellation(id) => {
            Command::AcknowledgeCancellation { dispatch_id: dispatch(*id)? }
        }
        Operation::Worker { worker: id, available: true } => {
            Command::SetWorkerAvailable { worker_id: worker(*id)? }
        }
        Operation::Worker { worker: id, available: false } => {
            Command::DrainWorker { worker_id: worker(*id)? }
        }
        Operation::LoseWorker(id) => Command::LoseWorker { worker_id: worker(*id)? },
        Operation::Pause(false) => Command::ResumeScheduler,
        Operation::Pause(true) | Operation::StaleFence => Command::PauseScheduler,
        Operation::ExternalToggle => {
            if model.paused {
                Command::ResumeScheduler
            } else {
                Command::PauseScheduler
            }
        }
        Operation::ReplayLast | Operation::Reopen | Operation::Read => {
            return Err("non-command operation dispatched".into());
        }
    })
}
fn command(
    state: Option<&SchedulerState>,
    serial: u64,
    kind: Command,
    stale: bool,
) -> Result<SchedulerCommand, String> {
    SchedulerCommand::new(
        CommandId::new(bytes(serial)).map_err(debug)?,
        EventId::new(bytes(serial)).map_err(debug)?,
        RunId::new(bytes(0)).map_err(debug)?,
        state.map_or(0, |state| state.sequence().get()) + u64::from(stale),
        state.map(SchedulerState::last_event_id),
        state.map_or(Sha256Digest::new([0; 32]), SchedulerState::state_digest),
        revision()?,
        kind,
    )
    .map_err(debug)
}
fn open(path: &std::path::Path) -> Result<SqliteJournal, String> {
    SqliteJournal::open(
        path,
        StoreId::new(bytes(0)).map_err(debug)?,
        SqliteJournalOptions::default(),
    )
    .map_err(debug)
}
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.capacity.contains(&0)
        || case.active_limit == 0
        || case.active_limit > 4
        || case.concurrency == 0
        || case.concurrency > case.active_limit
        || case.bypass_limit == 0
        || case.operations.len() > 256
    {
        return Err("case outside scheduler pilot bounds".into());
    }
    let directory = tempfile::tempdir().map_err(debug)?;
    let path = directory.path().join("scheduler.sqlite");
    let mut journal = open(&path)?;
    let mut session = SchedulerSession::default();
    let run = RunId::new(bytes(0)).map_err(debug)?;
    if session.state(&journal, run).map_err(debug)?.is_some() {
        return Err("fresh scheduler session fabricated state".into());
    }
    let binding = SchedulerBinding::new(
        run,
        SchedulerId::new(bytes(0)).map_err(debug)?,
        revision()?,
        limits(case)?,
        vector(case.capacity)?,
    )
    .map_err(debug)?;
    let request = command(None, 0, Command::StartScheduler { binding }, false)?;
    let transition = start(&request).map_err(debug)?;
    let mut state = transition.state().clone();
    session.commit(&mut journal, &request, transition).map_err(debug)?;
    for id in 0_u8..3 {
        let descriptor = WorkerDescriptor::new(
            worker(id)?,
            actor(if id == 1 { 1 } else { 0 })?,
            if id == 0 { vec![class(0), class(1)] } else { (0..5).map(class).collect() },
            vector(Model::worker_capacity(case, id))?,
            u16::from(case.concurrency),
            limits(case)?,
        )
        .map_err(debug)?;
        let request = command(
            Some(&state),
            u64::from(id) + 1,
            Command::RegisterWorker { descriptor },
            false,
        )?;
        let _ = session.state(&journal, run).map_err(debug)?;
        let transition = decide(&state, &request).map_err(debug)?;
        state = transition.state().clone();
        session.commit(&mut journal, &request, transition).map_err(debug)?;
    }
    let mut model = Model::new();
    model.sequence = 4;
    let mut metrics = crate::Metrics::new();
    let mut last = None;
    let mut rejected = false;
    compare(case, &model, &state, &mut metrics)?;
    for (step, operation) in case.operations.iter().enumerate() {
        match operation {
            Operation::Reopen => {
                drop(journal);
                journal = open(&path)?;
                let restored = session
                    .state(&journal, run)
                    .map_err(debug)?
                    .ok_or("reopened scheduler absent")?;
                if restored != &state {
                    return Err("session reopened a different scheduler state".into());
                }
                bump(&mut metrics, "sqlite_reopens");
            }
            Operation::ReplayLast => {
                if let Some((request, transition)) = &last {
                    let before = state.clone();
                    commit_scheduler_transition(&mut journal, request, transition)
                        .map_err(debug)?;
                    let restored = load_scheduler_replay(&journal, run)
                        .map_err(debug)?
                        .rebuild()
                        .map_err(debug)?
                        .ok_or("idempotent commit lost state")?;
                    if restored != before {
                        return Err("exact durable replay changed accepted state".into());
                    }
                    bump(&mut metrics, "exact_commit_replays");
                }
            }
            Operation::Read => bump(&mut metrics, "explicit_reads"),
            _ => {
                let mut next = model.clone();
                let selected = model.selected(case);
                let aged = selected
                    .is_some_and(|(id, _)| model.jobs[&id].bypass >= u16::from(case.bypass_limit));
                let expected = next.apply(case, operation);
                let loss_before = if let Operation::LoseWorker(id) = operation {
                    model
                        .reservations
                        .iter()
                        .filter(|(_, r)| r.worker == *id)
                        .map(|(dispatch, r)| {
                            let row = &model.jobs[&r.work];
                            let tag = if row.phase == 5 {
                                4
                            } else {
                                match row.job.recovery {
                                    0 if row.attempts < 3 => 0,
                                    0 => 1,
                                    1 => 2,
                                    _ => 3,
                                }
                            };
                            Ok::<_, String>((tag, self::dispatch(*dispatch)?, work(r.work)?))
                        })
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    Vec::new()
                };
                let before = state.clone();
                let _ = session.state(&journal, run).map_err(debug)?;
                let kind = actual_kind(case, &model, operation);
                let request = kind.and_then(|kind| {
                    command(
                        Some(&state),
                        u64::try_from(step).map_err(debug)? + 4,
                        kind,
                        matches!(operation, Operation::StaleFence),
                    )
                });
                let actual = request.as_ref().map_err(Clone::clone).and_then(|request| {
                    decide(&state, request).map_err(|error| format!("typed:{:?}", error.kind()))
                });
                match (expected, actual) {
                    (Ok(()), Ok(transition)) => {
                        let request = request?;
                        if let Operation::LoseWorker(id) = operation {
                            let peritus_scheduler::SchedulerEventKind::WorkerLost {
                                worker_id,
                                outcomes,
                            } = transition.event().kind()
                            else {
                                return Err("worker loss emitted a different fact".into());
                            };
                            let actual = outcomes
                                .iter()
                                .map(|outcome| match outcome {
                                    peritus_scheduler::LossOutcome::Requeued {
                                        dispatch_id,
                                        work_id,
                                    } => (0, *dispatch_id, *work_id),
                                    peritus_scheduler::LossOutcome::Exhausted {
                                        dispatch_id,
                                        work_id,
                                    } => (1, *dispatch_id, *work_id),
                                    peritus_scheduler::LossOutcome::Ambiguous {
                                        dispatch_id,
                                        work_id,
                                    } => (2, *dispatch_id, *work_id),
                                    peritus_scheduler::LossOutcome::Failed {
                                        dispatch_id,
                                        work_id,
                                    } => (3, *dispatch_id, *work_id),
                                    peritus_scheduler::LossOutcome::Cancelled {
                                        dispatch_id,
                                        work_id,
                                    } => (4, *dispatch_id, *work_id),
                                })
                                .collect::<Vec<_>>();
                            if *worker_id != worker(*id)? || actual != loss_before {
                                return Err("worker loss outcome list differs from independent pre-state ownership".into());
                            }
                            for (tag, _, _) in &actual {
                                bump(
                                    &mut metrics,
                                    match tag {
                                        0 => "lost_work_requeued",
                                        1 => "lost_work_exhausted",
                                        2 => "lost_work_ambiguous",
                                        3 => "lost_work_failed",
                                        _ => "lost_work_cancelled",
                                    },
                                );
                            }
                            bump(&mut metrics, "worker_losses");
                        }
                        state = transition.state().clone();
                        if matches!(operation, Operation::ExternalToggle) {
                            commit_scheduler_transition(&mut journal, &request, &transition)
                                .map_err(debug)?;
                            bump(&mut metrics, "external_session_invalidations");
                        } else {
                            session
                                .commit(&mut journal, &request, transition.clone())
                                .map_err(debug)?;
                        }
                        last = Some((request, transition));
                        model = next;
                        bump(&mut metrics, "accepted_commands");
                        if rejected {
                            bump(&mut metrics, "accepted_after_rejection");
                            rejected = false;
                        }
                        if matches!(operation, Operation::Dispatch(_)) {
                            bump(&mut metrics, "reserved_dispatches");
                            if aged {
                                bump(&mut metrics, "aged_dispatches");
                            }
                            if selected.is_some_and(|(id, _)| model.jobs[&id].attempts > 1) {
                                bump(&mut metrics, "retried_dispatches");
                            }
                        }
                        if matches!(operation, Operation::Finish { .. }) {
                            bump(&mut metrics, "released_reservations");
                        }
                        match operation {
                            Operation::Start(_) => bump(&mut metrics, "acknowledged_starts"),
                            Operation::Retry(_) => bump(&mut metrics, "retry_queues"),
                            Operation::Cancel { .. } => bump(&mut metrics, "cancellation_requests"),
                            Operation::AcknowledgeCancellation(_) => {
                                bump(&mut metrics, "cancellation_acknowledgements")
                            }
                            Operation::Worker { .. } => bump(&mut metrics, "worker_controls"),
                            Operation::Pause(_) => bump(&mut metrics, "pause_controls"),
                            _ => {}
                        }
                    }
                    (Err(expected), Err(actual)) => {
                        let constructed = matches!(operation,Operation::Admit(job)if job.dependencies.contains(&job.id)||job.parent==Some(job.id));
                        if !constructed && actual != format!("typed:{expected:?}") {
                            return Err(format!(
                                "step {step} {operation:?}: expected {expected:?}, got {actual}"
                            ));
                        }
                        if state != before {
                            return Err("rejected scheduler command mutated state".into());
                        }
                        rejected = true;
                        bump(&mut metrics, "rejected_commands");
                        let id = match operation {
                            Operation::Start(id)
                            | Operation::AcknowledgeCancellation(id)
                            | Operation::Finish { dispatch: id, .. } => Some(*id),
                            _ => None,
                        };
                        if expected == Error::UnknownIdentity
                            && id.is_some_and(|id| model.lost_dispatches.contains(&id))
                        {
                            bump(&mut metrics, "late_lost_dispatch_rejections");
                        }
                    }
                    (expected, actual) => {
                        return Err(format!(
                            "step {step} {operation:?}: scheduler/model disagreement: expected {expected:?}, actual {}",
                            actual.map(|_| "accepted".to_owned()).unwrap_or_else(|error| error)
                        ));
                    }
                }
            }
        }
        let observed =
            session.state(&journal, run).map_err(debug)?.ok_or("durable scheduler absent")?;
        if observed != &state {
            return Err("cached session differs from committed scheduler".into());
        }
        compare(case, &model, &state, &mut metrics)?;
        bump(&mut metrics, "operations");
    }
    let replay = load_scheduler_replay(&journal, run).map_err(debug)?;
    if replay.events().len() != usize::try_from(model.sequence).map_err(debug)?
        || replay.rebuild().map_err(debug)?.as_ref() != Some(&state)
    {
        return Err("durable scheduler replay differs from independent accepted history".into());
    }
    bump(&mut metrics, "cases");
    Ok(metrics)
}
fn job(id: u8, owner: u8) -> Job {
    Job {
        id,
        owner,
        class: 0,
        priority: id,
        request: [1, 0],
        dependencies: Vec::new(),
        parent: None,
        recovery: 1,
    }
}
fn aging_case(bypass: u8, low: u8, high: u8) -> Case {
    let mut case = calibration();
    case.concurrency = 1;
    case.bypass_limit = bypass;
    let mut first = job(0, 0);
    first.priority = low;
    case.operations = vec![Operation::Admit(first)];
    for turn in 0..=bypass {
        let mut current = job(turn + 1, 0);
        current.priority = high;
        case.operations.extend([Operation::Admit(current), Operation::Dispatch(turn)]);
        if turn < bypass {
            case.operations
                .extend([Operation::Start(turn), Operation::Finish { dispatch: turn, outcome: 0 }]);
        }
    }
    case.operations.extend([Operation::Reopen, Operation::Read]);
    case
}
fn exhaustion_case() -> Case {
    let mut case = calibration();
    let mut current = job(0, 1);
    current.recovery = 0;
    case.operations = vec![Operation::Admit(current)];
    for attempt in 0..3 {
        case.operations.push(Operation::Dispatch(attempt));
        if attempt != 1 {
            case.operations.push(Operation::Start(attempt));
        }
        case.operations.extend([
            Operation::LoseWorker(1),
            Operation::Reopen,
            Operation::Start(attempt),
            Operation::Worker { worker: 1, available: true },
        ]);
    }
    case.operations.extend([
        Operation::Dispatch(3),
        Operation::Admit(job(1, 1)),
        Operation::Dispatch(4),
        Operation::Start(4),
        Operation::Finish { dispatch: 4, outcome: 0 },
        Operation::ReplayLast,
    ]);
    case
}
fn loss_case(recovery: u8, cancel: bool, started: bool) -> Case {
    if recovery == 0 && started {
        return exhaustion_case();
    }
    let mut case = calibration();
    case.capacity = [6, 4];
    let mut first = job(0, 0);
    first.recovery = recovery;
    let mut second = job(1, 0);
    second.recovery = recovery;
    case.operations = vec![
        Operation::Admit(first),
        Operation::Admit(second),
        Operation::Admit(job(2, 1)),
        Operation::Dispatch(0),
        Operation::Dispatch(1),
        Operation::Dispatch(2),
        Operation::Start(0),
        Operation::Start(1),
    ];
    if started {
        case.operations.push(Operation::Start(2));
    }
    if cancel {
        case.operations.push(Operation::Cancel { work: 1, tree: false });
    }
    case.operations.extend([
        Operation::LoseWorker(0),
        Operation::Reopen,
        Operation::Start(1),
        Operation::Finish { dispatch: 1, outcome: 0 },
        Operation::AcknowledgeCancellation(1),
        Operation::Dispatch(1),
        Operation::Worker { worker: 0, available: true },
        Operation::Dispatch(3),
        Operation::Admit(job(3, 0)),
        Operation::Dispatch(4),
        Operation::Start(4),
        Operation::Finish { dispatch: 4, outcome: 0 },
        Operation::Finish { dispatch: 0, outcome: 0 },
        Operation::Reopen,
        Operation::ReplayLast,
    ]);
    case
}
pub fn calibration() -> Case {
    Case {
        capacity: [4, 4],
        concurrency: 2,
        active_limit: 4,
        bypass_limit: 2,
        operations: vec![
            Operation::Admit(job(0, 1)),
            Operation::Dispatch(0),
            Operation::Start(0),
            Operation::Finish { dispatch: 0, outcome: 0 },
            Operation::Reopen,
            Operation::ReplayLast,
        ],
    }
}
pub fn examples() -> Vec<Case> {
    let mut deep = calibration();
    deep.operations.extend([
        Operation::Admit(job(1, 0)),
        Operation::Dispatch(1),
        Operation::Finish { dispatch: 1, outcome: 0 },
        Operation::Start(1),
        Operation::Finish { dispatch: 1, outcome: 1 },
        Operation::Retry(1),
        Operation::Dispatch(2),
        Operation::Start(2),
        Operation::Cancel { work: 1, tree: false },
        Operation::AcknowledgeCancellation(2),
        Operation::ExternalToggle,
        Operation::Dispatch(3),
        Operation::ExternalToggle,
        Operation::Reopen,
        Operation::ReplayLast,
    ]);
    let mut dependencies = calibration();
    let mut dependent = job(2, 0);
    dependent.dependencies = vec![1];
    dependent.parent = Some(1);
    dependencies.operations = vec![
        Operation::Admit(job(1, 0)),
        Operation::Admit(dependent),
        Operation::Admit(job(3, 0)),
        Operation::Dispatch(0),
        Operation::Start(0),
        Operation::Finish { dispatch: 0, outcome: 2 },
        Operation::Cancel { work: 3, tree: true },
        Operation::Read,
        Operation::Reopen,
    ];
    let mut cases = vec![
        calibration(),
        deep,
        dependencies,
        aging_case(2, 0, 255),
        Case {
            operations: vec![
                Operation::StaleFence,
                Operation::Admit(job(0, 0)),
                Operation::Reopen,
                Operation::Dispatch(0),
            ],
            ..calibration()
        },
    ];
    for recovery in 0..3 {
        for cancel in [false, true] {
            for started in [false, true] {
                cases.push(loss_case(recovery, cancel, started));
            }
        }
    }
    cases
}
