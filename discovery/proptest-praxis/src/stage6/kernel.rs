//! Independent session/run/attempt/turn phase model over the public B0 reducer.
//! Mounted below `agent_driver` only to reuse its synthetic acceptance-contract fixture.

use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, ChildBudgetRequest,
};
use peritus_kernel::{
    CommandEnvelope, KernelAggregate, KernelCommand, KernelErrorKind, KernelEventKind,
    KernelOutcome, KernelSubject, ReducerInputs,
};
use peritus_types::{PolicyId, RevisionTuple, RunId};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

use super::{contract, identity::CommandIds};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Kind {
    StartRun,
    StartAttempt,
    StartTurn,
    CompleteTurn,
    FailTurn,
    CancelTurn,
    SubmitAttempt,
    FailAttempt,
    ExhaustAttempt,
    PauseRun,
    ResumeRun,
    CancelRun,
    FailRun,
    ExhaustRun,
    RejectRun,
    PauseSession,
    ResumeSession,
    CloseSession,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Fault {
    None,
    WrongRevision,
    WrongHead,
    DuplicateCommand,
    DuplicateEvent,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Step {
    pub kind: Kind,
    pub fault: Fault,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub steps: Vec<Step>,
}

fn step(kind: Kind) -> Step {
    Step { kind, fault: Fault::None }
}

pub fn calibration() -> Case {
    Case { steps: vec![step(Kind::StartRun), step(Kind::PauseSession)] }
}

pub fn examples() -> Vec<Case> {
    vec![
        calibration(),
        Case {
            steps: vec![
                step(Kind::StartRun),
                step(Kind::StartAttempt),
                step(Kind::StartTurn),
                step(Kind::CompleteTurn),
                step(Kind::SubmitAttempt),
                step(Kind::FailAttempt),
                step(Kind::PauseRun),
                step(Kind::PauseSession),
                step(Kind::ResumeSession),
                step(Kind::ResumeRun),
                step(Kind::CancelRun),
                step(Kind::CloseSession),
            ],
        },
        Case {
            steps: vec![
                step(Kind::StartRun),
                step(Kind::StartAttempt),
                step(Kind::StartTurn),
                step(Kind::ExhaustRun),
                step(Kind::CloseSession),
            ],
        },
        Case {
            steps: vec![
                step(Kind::StartRun),
                Step { kind: Kind::CancelRun, fault: Fault::WrongRevision },
                Step { kind: Kind::CancelRun, fault: Fault::WrongHead },
                Step { kind: Kind::CancelRun, fault: Fault::DuplicateCommand },
                Step { kind: Kind::CancelRun, fault: Fault::DuplicateEvent },
                step(Kind::CancelRun),
                step(Kind::CloseSession),
            ],
        },
        Case {
            steps: vec![
                step(Kind::PauseSession),
                step(Kind::StartRun),
                step(Kind::ResumeSession),
                step(Kind::StartRun),
                step(Kind::FailRun),
                step(Kind::CloseSession),
            ],
        },
        Case {
            steps: vec![
                step(Kind::StartRun),
                step(Kind::StartAttempt),
                step(Kind::StartTurn),
                step(Kind::FailTurn),
                step(Kind::ExhaustAttempt),
                step(Kind::RejectRun),
                step(Kind::CloseSession),
            ],
        },
        Case {
            steps: vec![
                step(Kind::StartRun),
                step(Kind::StartAttempt),
                step(Kind::StartTurn),
                step(Kind::CancelTurn),
                step(Kind::SubmitAttempt),
                step(Kind::ExhaustRun),
                step(Kind::CloseSession),
            ],
        },
    ]
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let kinds = prop::sample::select(vec![
        Kind::StartRun,
        Kind::StartAttempt,
        Kind::StartTurn,
        Kind::CompleteTurn,
        Kind::FailTurn,
        Kind::CancelTurn,
        Kind::SubmitAttempt,
        Kind::FailAttempt,
        Kind::ExhaustAttempt,
        Kind::PauseRun,
        Kind::ResumeRun,
        Kind::CancelRun,
        Kind::FailRun,
        Kind::ExhaustRun,
        Kind::RejectRun,
        Kind::PauseSession,
        Kind::ResumeSession,
        Kind::CloseSession,
    ]);
    let faults = prop_oneof![
        7 => Just(Fault::None),
        1 => Just(Fault::WrongRevision),
        1 => Just(Fault::WrongHead),
        1 => Just(Fault::DuplicateCommand),
        1 => Just(Fault::DuplicateEvent),
    ];
    let operation = (kinds, faults).prop_map(|(kind, fault)| Step { kind, fault });
    let general = prop::collection::vec(operation.clone(), 0..=max_steps.min(64))
        .prop_map(|steps| Case { steps });
    if max_steps < 2 {
        return general.boxed();
    }
    let blocked_pause =
        prop::collection::vec(operation.clone(), 0..=max_steps.saturating_sub(2).min(62)).prop_map(
            |mut suffix| {
                let mut steps = calibration().steps;
                steps.append(&mut suffix);
                Case { steps }
            },
        );
    if max_steps < 8 {
        return prop_oneof![3 => general, 2 => blocked_pause].boxed();
    }
    let completed_turn = prop::collection::vec(operation, 0..=max_steps.saturating_sub(8).min(56))
        .prop_map(|mut suffix| {
            let mut steps = vec![
                step(Kind::StartRun),
                step(Kind::StartAttempt),
                step(Kind::StartTurn),
                step(Kind::CompleteTurn),
                step(Kind::SubmitAttempt),
                step(Kind::FailAttempt),
                step(Kind::PauseRun),
                step(Kind::PauseSession),
            ];
            steps.append(&mut suffix);
            Case { steps }
        });
    prop_oneof![3 => general, 2 => blocked_pause, 2 => completed_turn].boxed()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Session {
    Open,
    Paused,
    Closed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Run {
    Pending,
    Running,
    Paused,
    Reviewing,
    Cancelled,
    Failed,
    Exhausted,
    Rejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attempt {
    Active,
    Submitted,
    Failed,
    Exhausted,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Turn {
    Active,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
struct Model {
    session: Session,
    run: Option<Run>,
    attempt: Option<Attempt>,
    turn: Option<Turn>,
    current_attempt: bool,
    accepted: u64,
}

impl Model {
    fn new() -> Self {
        Self {
            session: Session::Open,
            run: None,
            attempt: None,
            turn: None,
            current_attempt: false,
            accepted: 1,
        }
    }

    fn run_live(&self) -> bool {
        matches!(self.run, Some(Run::Pending | Run::Running | Run::Paused | Run::Reviewing))
    }

    fn apply(&mut self, kind: Kind) -> Option<KernelEventKind> {
        match kind {
            Kind::StartRun if self.session == Session::Open && self.run.is_none() => {
                self.run = Some(Run::Pending);
                Some(KernelEventKind::RunStarted)
            }
            Kind::StartAttempt
                if self.session == Session::Open
                    && matches!(self.run, Some(Run::Pending | Run::Running))
                    && !self.current_attempt
                    && self.attempt.is_none() =>
            {
                self.run = Some(Run::Running);
                self.attempt = Some(Attempt::Active);
                self.current_attempt = true;
                Some(KernelEventKind::AttemptStarted)
            }
            Kind::StartTurn if self.attempt == Some(Attempt::Active) && self.turn.is_none() => {
                self.turn = Some(Turn::Active);
                Some(KernelEventKind::TurnStarted)
            }
            Kind::CompleteTurn if self.turn == Some(Turn::Active) => {
                self.turn = Some(Turn::Completed);
                Some(KernelEventKind::TurnCompleted)
            }
            Kind::FailTurn if self.turn == Some(Turn::Active) => {
                self.turn = Some(Turn::Failed);
                Some(KernelEventKind::TurnFailed)
            }
            Kind::CancelTurn if self.turn == Some(Turn::Active) => {
                self.turn = Some(Turn::Cancelled);
                Some(KernelEventKind::TurnCancelled)
            }
            Kind::SubmitAttempt
                if self.run == Some(Run::Running)
                    && self.attempt == Some(Attempt::Active)
                    && self.turn != Some(Turn::Active) =>
            {
                self.run = Some(Run::Reviewing);
                self.attempt = Some(Attempt::Submitted);
                Some(KernelEventKind::AttemptSubmitted)
            }
            Kind::FailAttempt
                if self.run_live()
                    && matches!(self.attempt, Some(Attempt::Active | Attempt::Submitted)) =>
            {
                self.run = Some(Run::Running);
                self.attempt = Some(Attempt::Failed);
                self.current_attempt = false;
                if self.turn == Some(Turn::Active) {
                    self.turn = Some(Turn::Failed);
                }
                Some(KernelEventKind::AttemptFailed)
            }
            Kind::ExhaustAttempt
                if self.run_live()
                    && matches!(self.attempt, Some(Attempt::Active | Attempt::Submitted)) =>
            {
                self.run = Some(Run::Running);
                self.attempt = Some(Attempt::Exhausted);
                self.current_attempt = false;
                if self.turn == Some(Turn::Active) {
                    self.turn = Some(Turn::Cancelled);
                }
                Some(KernelEventKind::AttemptExhausted)
            }
            Kind::PauseRun if self.session == Session::Open && self.run == Some(Run::Running) => {
                self.run = Some(Run::Paused);
                Some(KernelEventKind::RunPaused)
            }
            Kind::ResumeRun if self.session == Session::Open && self.run == Some(Run::Paused) => {
                self.run = Some(Run::Running);
                Some(KernelEventKind::RunResumed)
            }
            Kind::CancelRun | Kind::FailRun | Kind::ExhaustRun | Kind::RejectRun
                if self.run_live() =>
            {
                let (run, attempt, turn, event) = match kind {
                    Kind::CancelRun => (
                        Run::Cancelled,
                        Attempt::Cancelled,
                        Turn::Cancelled,
                        KernelEventKind::RunCancelled,
                    ),
                    Kind::FailRun => {
                        (Run::Failed, Attempt::Failed, Turn::Failed, KernelEventKind::RunFailed)
                    }
                    Kind::ExhaustRun => (
                        Run::Exhausted,
                        Attempt::Exhausted,
                        Turn::Cancelled,
                        KernelEventKind::RunExhausted,
                    ),
                    _ => (
                        Run::Rejected,
                        Attempt::Failed,
                        Turn::Cancelled,
                        KernelEventKind::RunRejected,
                    ),
                };
                self.run = Some(run);
                if matches!(self.attempt, Some(Attempt::Active | Attempt::Submitted)) {
                    self.attempt = Some(attempt);
                }
                if self.turn == Some(Turn::Active) {
                    self.turn = Some(turn);
                }
                self.current_attempt = false;
                Some(event)
            }
            Kind::PauseSession
                if self.session == Session::Open
                    && (!self.run_live() || self.run == Some(Run::Paused)) =>
            {
                self.session = Session::Paused;
                Some(KernelEventKind::SessionPaused)
            }
            Kind::ResumeSession if self.session == Session::Paused => {
                self.session = Session::Open;
                Some(KernelEventKind::SessionResumed)
            }
            Kind::CloseSession if self.session != Session::Closed && !self.run_live() => {
                self.session = Session::Closed;
                Some(KernelEventKind::SessionClosed)
            }
            _ => None,
        }
    }
}

fn command(kind: Kind, ids: &CommandIds) -> KernelCommand {
    match kind {
        Kind::StartRun => KernelCommand::StartRun { run_id: ids.run },
        Kind::StartAttempt => {
            KernelCommand::StartAttempt { run_id: ids.run, attempt_id: ids.attempt }
        }
        Kind::StartTurn => KernelCommand::StartTurn { attempt_id: ids.attempt, turn_id: ids.turn },
        Kind::CompleteTurn => {
            KernelCommand::CompleteTurn { attempt_id: ids.attempt, turn_id: ids.turn }
        }
        Kind::FailTurn => KernelCommand::FailTurn { attempt_id: ids.attempt, turn_id: ids.turn },
        Kind::CancelTurn => {
            KernelCommand::CancelTurn { attempt_id: ids.attempt, turn_id: ids.turn }
        }
        Kind::SubmitAttempt => {
            KernelCommand::SubmitAttempt { run_id: ids.run, attempt_id: ids.attempt }
        }
        Kind::FailAttempt => {
            KernelCommand::FailAttempt { run_id: ids.run, attempt_id: ids.attempt }
        }
        Kind::ExhaustAttempt => {
            KernelCommand::ExhaustAttempt { run_id: ids.run, attempt_id: ids.attempt }
        }
        Kind::PauseRun => KernelCommand::PauseRun { run_id: ids.run },
        Kind::ResumeRun => KernelCommand::ResumeRun { run_id: ids.run },
        Kind::CancelRun => KernelCommand::CancelRun { run_id: ids.run },
        Kind::FailRun => KernelCommand::FailRun { run_id: ids.run },
        Kind::ExhaustRun => KernelCommand::ExhaustRun { run_id: ids.run },
        Kind::RejectRun => KernelCommand::RejectRun { run_id: ids.run },
        Kind::PauseSession => KernelCommand::PauseSession,
        Kind::ResumeSession => KernelCommand::ResumeSession,
        Kind::CloseSession => KernelCommand::CloseSession,
    }
}

fn snapshots(
    ids: &CommandIds,
) -> Result<(peritus_budget::BudgetSnapshot, peritus_budget::BudgetSnapshot), String> {
    let root = BudgetLedger::new_root(
        ids.kernel_root_budget,
        ids.revision,
        BudgetLimits::new(BudgetAmounts::from_units(100, 100, 1000, 100, 100)),
    );
    let root = root
        .transition(BudgetCommand::AllocateChild(ChildBudgetRequest::new(
            ids.kernel_child_budget,
            ids.kernel_root_budget,
            ids.revision,
            BudgetLimits::new(BudgetAmounts::from_units(20, 20, 200, 20, 20)),
        )))
        .map_err(|error| format!("kernel child budget: {error:?}"))?
        .into_ledger();
    let parent = root
        .account(ids.kernel_root_budget)
        .map_err(|error| format!("kernel root budget: {error:?}"))?;
    let child = root
        .account(ids.kernel_child_budget)
        .map_err(|error| format!("kernel child budget: {error:?}"))?;
    Ok((parent, child))
}

fn wrong_revision(revision: RevisionTuple) -> RevisionTuple {
    RevisionTuple::new(
        revision.acceptance_spec_id(),
        revision.harness_id(),
        revision.workspace_id(),
        revision.workspace_generation(),
        revision.workspace_revision(),
        PolicyId::new([254; 16]).expect("nonzero alternate policy"),
        revision.provider_profile_id(),
    )
}

fn event_subject(kind: Kind, ids: &CommandIds) -> KernelSubject {
    match kind {
        Kind::PauseSession | Kind::ResumeSession | Kind::CloseSession => {
            KernelSubject::Session(ids.session)
        }
        Kind::StartRun
        | Kind::PauseRun
        | Kind::ResumeRun
        | Kind::CancelRun
        | Kind::FailRun
        | Kind::ExhaustRun
        | Kind::RejectRun => KernelSubject::Run(ids.run),
        Kind::StartAttempt | Kind::SubmitAttempt | Kind::FailAttempt | Kind::ExhaustAttempt => {
            KernelSubject::Attempt(ids.attempt)
        }
        _ => KernelSubject::Turn(ids.turn),
    }
}

fn check_projection(
    state: &KernelAggregate,
    model: &Model,
    ids: &CommandIds,
) -> Result<(), String> {
    let session = match model.session {
        Session::Open => peritus_kernel::SessionPhase::Open,
        Session::Paused => peritus_kernel::SessionPhase::Paused,
        Session::Closed => peritus_kernel::SessionPhase::Closed,
    };
    if state.session().phase() != session
        || state.last_sequence().get() != model.accepted
        || !state.is_valid()
    {
        return Err("kernel session/sequence/aggregate validity diverged".into());
    }
    let run = state.runs();
    if run.len() != usize::from(model.run.is_some()) {
        return Err("kernel run multiplicity diverged".into());
    }
    if let Some(expected) = model.run {
        let phase = match expected {
            Run::Pending => peritus_kernel::RunPhase::Pending,
            Run::Running => peritus_kernel::RunPhase::Running,
            Run::Paused => peritus_kernel::RunPhase::Paused,
            Run::Reviewing => peritus_kernel::RunPhase::Reviewing,
            Run::Cancelled => peritus_kernel::RunPhase::Cancelled,
            Run::Failed => peritus_kernel::RunPhase::Failed,
            Run::Exhausted => peritus_kernel::RunPhase::Exhausted,
            Run::Rejected => peritus_kernel::RunPhase::Rejected,
        };
        if run[0].id() != ids.run
            || run[0].phase() != phase
            || run[0].current_attempt_id().is_some() != model.current_attempt
        {
            return Err(format!("kernel run phase/current attempt diverged: {:?}", run[0]));
        }
    }
    let attempt = state.attempts();
    if attempt.len() != usize::from(model.attempt.is_some()) {
        return Err("kernel attempt multiplicity diverged".into());
    }
    if let Some(expected) = model.attempt {
        let phase = match expected {
            Attempt::Active => peritus_kernel::AttemptPhase::Active,
            Attempt::Submitted => peritus_kernel::AttemptPhase::Submitted,
            Attempt::Failed => peritus_kernel::AttemptPhase::Failed,
            Attempt::Exhausted => peritus_kernel::AttemptPhase::Exhausted,
            Attempt::Cancelled => peritus_kernel::AttemptPhase::Cancelled,
        };
        if attempt[0].id() != ids.attempt
            || attempt[0].run_id() != ids.run
            || attempt[0].phase() != phase
        {
            return Err(format!("kernel attempt phase diverged: {:?}", attempt[0]));
        }
    }
    let turn = state.turns();
    if turn.len() != usize::from(model.turn.is_some()) {
        return Err("kernel turn multiplicity diverged".into());
    }
    if let Some(expected) = model.turn {
        let phase = match expected {
            Turn::Active => peritus_kernel::TurnPhase::Active,
            Turn::Completed => peritus_kernel::TurnPhase::Completed,
            Turn::Failed => peritus_kernel::TurnPhase::Failed,
            Turn::Cancelled => peritus_kernel::TurnPhase::Cancelled,
        };
        if turn[0].id() != ids.turn
            || turn[0].attempt_id() != ids.attempt
            || turn[0].phase() != phase
        {
            return Err(format!("kernel turn phase diverged: {:?}", turn[0]));
        }
    }
    Ok(())
}

fn bump(metrics: &mut crate::Metrics, key: &str) {
    *metrics.entry(key.to_owned()).or_default() += 1;
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.steps.len() > 64 {
        return Err("setup-incomplete: kernel history length bound".into());
    }
    let fixture_run = RunId::new([73; 16]).map_err(|error| format!("fixture run: {error:?}"))?;
    let acceptance = contract::command_contract(fixture_run, 0)?;
    let ids = CommandIds::new(fixture_run, 1, &acceptance)?;
    let (root_budget, child_budget) = snapshots(&ids)?;
    let genesis_command = ids.command("stage6-kernel-genesis-command")?;
    let genesis_event = ids.event("stage6-kernel-genesis-event")?;
    let genesis = KernelAggregate::open(
        ids.project,
        ids.session,
        &acceptance,
        ids.revision,
        CommandEnvelope::new(genesis_command, genesis_event, None, ids.revision),
    )
    .map_err(|error| format!("kernel genesis: {error:?}"))?;
    let mut state = genesis.into_parts().0;
    let mut model = Model::new();
    let mut metrics = crate::Metrics::new();
    check_projection(&state, &model, &ids)?;
    for (index, step) in case.steps.iter().copied().enumerate() {
        let command_id = ids.command(&format!("stage6-kernel-command-{index}"))?;
        let event_id = ids.event(&format!("stage6-kernel-event-{index}"))?;
        let previous = state.head_event_id();
        let envelope = CommandEnvelope::new(
            if matches!(step.fault, Fault::DuplicateCommand) {
                genesis_command
            } else {
                command_id
            },
            if matches!(step.fault, Fault::DuplicateEvent) { genesis_event } else { event_id },
            if matches!(step.fault, Fault::WrongHead) { None } else { Some(previous) },
            if matches!(step.fault, Fault::WrongRevision) {
                wrong_revision(ids.revision)
            } else {
                ids.revision
            },
        );
        let inputs = match step.kind {
            Kind::StartRun => ReducerInputs::new(&acceptance).with_run_budget(root_budget),
            Kind::StartAttempt => ReducerInputs::new(&acceptance)
                .with_attempt_budget(child_budget)
                .with_parent_budget(root_budget),
            _ => ReducerInputs::new(&acceptance),
        };
        let before = state.clone();
        let mut projected = model.clone();
        let wanted =
            if matches!(step.fault, Fault::None) { projected.apply(step.kind) } else { None };
        match state.reduce(envelope, command(step.kind, &ids), inputs) {
            KernelOutcome::Applied(transition) => {
                let Some(kind) = wanted else {
                    return Err(format!("kernel step {index} {step:?} unexpectedly applied"));
                };
                let event = transition.event();
                if event.kind() != kind
                    || event.subject() != event_subject(step.kind, &ids)
                    || event.id() != envelope.event_id()
                    || event.command_id() != envelope.command_id()
                    || event.previous_event_id() != Some(previous)
                    || event.sequence().get() != model.accepted + 1
                    || event.revision() != ids.revision
                {
                    return Err(format!(
                        "kernel event binding diverged at step {index}: {event:?}"
                    ));
                }
                state = transition.into_parts().0;
                projected.accepted += 1;
                model = projected;
                bump(&mut metrics, "kernel_applied");
                bump(&mut metrics, &format!("kernel_applied_{:?}", step.kind));
            }
            KernelOutcome::Rejected { aggregate, error } => {
                if wanted.is_some() {
                    return Err(format!("kernel step {index} {step:?} rejected: {error:?}"));
                }
                if aggregate != before {
                    return Err(format!("kernel rejection mutated aggregate at step {index}"));
                }
                if let Some(expected_error) = (match step.fault {
                    Fault::None => None,
                    Fault::WrongRevision => Some(KernelErrorKind::RevisionMismatch),
                    Fault::WrongHead => Some(KernelErrorKind::CausalHeadMismatch),
                    Fault::DuplicateCommand => Some(KernelErrorKind::DuplicateCommand),
                    Fault::DuplicateEvent => Some(KernelErrorKind::DuplicateEvent),
                }) && error.kind() != expected_error
                {
                    return Err(format!(
                        "kernel preflight error at step {index}: expected {expected_error:?}, got {error:?}"
                    ));
                }
                state = aggregate;
                bump(&mut metrics, "kernel_rejected");
            }
        }
        check_projection(&state, &model, &ids)?;
    }
    bump(&mut metrics, "kernel_histories");
    Ok(metrics)
}
