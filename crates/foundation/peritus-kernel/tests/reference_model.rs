//! Deterministic generated traces checked against an independent lifecycle model.

#[path = "reference_model/model.rs"]
mod model;
mod support;

use peritus_kernel::{
    AcceptancePhase, ActionPhase, AttemptPhase, KernelAggregate, KernelCommand, KernelOutcome,
    ReducerInputs, ReviewPhase, RunPhase, SessionPhase, TurnPhase,
};
use peritus_policy::ActorRole;
use peritus_spec::AcceptanceContract;
use support::{Fixture, digest, execute};

#[derive(Clone, Copy)]
enum Op {
    PauseSession,
    ResumeSession,
    StartRun,
    CancelRun,
    FailRun,
    ExhaustRun,
    RejectRun,
    StartAttempt,
    FailAttempt,
    ExhaustAttempt,
    StartTurn,
    CancelTurn,
    ProposeAction,
    AuthorizeAction,
    DispatchAction,
    CompleteAction,
    CancelAction,
    CompleteTurn,
    SubmitAttempt,
    RequestReview,
    BeginReview,
    SubmitReview,
    BeginAcceptance,
    EvaluateAcceptance { acceptable: bool },
}

#[derive(Clone, Copy)]
struct ReferenceModel {
    session: SessionPhase,
    run: Option<RunPhase>,
    attempt: Option<AttemptPhase>,
    turn: Option<TurnPhase>,
    action: Option<ActionPhase>,
    review: Option<ReviewPhase>,
    acceptance: Option<AcceptancePhase>,
    sequence: u64,
}

#[test]
fn generated_traces_match_reference_after_each_step_and_first_rejection() {
    for seed in 0..64 {
        run_generated_trace(seed);
    }
}

fn run_generated_trace(seed: u8) {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let mut aggregate = fixture.genesis(&contract);
    let mut model = ReferenceModel::genesis();
    let mut saw_rejection = false;

    for (index, op) in generated_trace(seed).into_iter().enumerate() {
        let before = aggregate.clone();
        let previous_head = before.head_event_id();
        let command_offset = u8::try_from(index).expect("generated trace length");
        let command_value = 80u8.checked_add(command_offset).expect("command identity range");
        let outcome = apply_kernel(before.clone(), op, command_value, &fixture, &contract);
        if model.step(op).is_ok() {
            let transition = outcome.into_result().expect("reference-accepted command");
            assert_eq!(transition.event().previous_event_id(), Some(previous_head));
            assert_eq!(transition.event().sequence().get(), model.sequence);
            aggregate = transition.into_parts().0;
            assert_projection(&aggregate, model, &fixture);
        } else {
            let (returned, _) = outcome.into_result().expect_err("reference-rejected command");
            assert_eq!(returned, before);
            saw_rejection = true;
            break;
        }
    }
    assert!(saw_rejection, "seed {seed} must reach an intentional first rejection");
}

fn generated_trace(seed: u8) -> Vec<Op> {
    let mut trace = Vec::new();
    if seed & 1 == 1 {
        trace.extend([Op::PauseSession, Op::ResumeSession]);
    }
    trace.push(Op::StartRun);
    match seed % 5 {
        0 => trace.extend([Op::CancelRun, Op::CancelRun]),
        1 => trace.extend([Op::FailRun, Op::FailRun]),
        2 => trace.extend([Op::ExhaustRun, Op::ExhaustRun]),
        3 => trace.extend([Op::RejectRun, Op::RejectRun]),
        _ => append_attempt_trace(&mut trace, seed),
    }
    trace
}

fn append_attempt_trace(trace: &mut Vec<Op>, seed: u8) {
    trace.push(Op::StartAttempt);
    match (seed / 5) % 3 {
        0 => trace.extend([Op::FailAttempt, Op::FailAttempt]),
        1 => trace.extend([Op::ExhaustAttempt, Op::ExhaustAttempt]),
        _ => {
            trace.push(Op::StartTurn);
            if seed & 2 == 0 {
                trace.extend([Op::CancelTurn, Op::CancelTurn]);
            } else {
                append_review_trace(trace, seed);
            }
        }
    }
}

fn append_review_trace(trace: &mut Vec<Op>, seed: u8) {
    trace.push(Op::ProposeAction);
    if seed & 4 == 0 {
        trace.push(Op::CancelAction);
    } else {
        trace.extend([Op::AuthorizeAction, Op::DispatchAction, Op::CompleteAction]);
    }
    trace.extend([
        Op::CompleteTurn,
        Op::SubmitAttempt,
        Op::RequestReview,
        Op::BeginReview,
        Op::SubmitReview,
        Op::BeginAcceptance,
    ]);
    let acceptable = seed & 8 != 0;
    trace.extend([Op::EvaluateAcceptance { acceptable }, Op::EvaluateAcceptance { acceptable }]);
}

fn apply_kernel(
    state: KernelAggregate,
    op: Op,
    value: u8,
    fixture: &Fixture,
    contract: &AcceptanceContract,
) -> KernelOutcome {
    let command = kernel_command(op, fixture);
    match op {
        Op::StartRun => {
            let (root, _) = fixture.budget_snapshots();
            execute(state, value, command, ReducerInputs::new(contract).with_run_budget(root))
        }
        Op::StartAttempt => {
            let (root, child) = fixture.budget_snapshots();
            execute(
                state,
                value,
                command,
                ReducerInputs::new(contract).with_attempt_budget(child).with_parent_budget(root),
            )
        }
        Op::AuthorizeAction => {
            let capability = fixture.capability_use(fixture.action_id, digest(70));
            execute(
                state,
                value,
                command,
                ReducerInputs::new(contract).with_capability_use(&capability),
            )
        }
        Op::EvaluateAcceptance { acceptable } => {
            let evidence = if acceptable {
                fixture.evidence(contract, fixture.revision, fixture.review_id)
            } else {
                fixture.incomplete_evidence(contract, fixture.revision, fixture.review_id)
            };
            execute(
                state,
                value,
                command,
                ReducerInputs::new(contract).with_acceptance_evidence(&evidence),
            )
        }
        _ => execute(state, value, command, ReducerInputs::new(contract)),
    }
}

const fn kernel_command(op: Op, fixture: &Fixture) -> KernelCommand {
    match op {
        Op::PauseSession => KernelCommand::PauseSession,
        Op::ResumeSession => KernelCommand::ResumeSession,
        Op::StartRun => KernelCommand::StartRun { run_id: fixture.run_id },
        Op::CancelRun => KernelCommand::CancelRun { run_id: fixture.run_id },
        Op::FailRun => KernelCommand::FailRun { run_id: fixture.run_id },
        Op::ExhaustRun => KernelCommand::ExhaustRun { run_id: fixture.run_id },
        Op::RejectRun => KernelCommand::RejectRun { run_id: fixture.run_id },
        Op::StartAttempt => {
            KernelCommand::StartAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id }
        }
        Op::FailAttempt => {
            KernelCommand::FailAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id }
        }
        Op::ExhaustAttempt => {
            KernelCommand::ExhaustAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id }
        }
        Op::StartTurn => {
            KernelCommand::StartTurn { attempt_id: fixture.attempt_id, turn_id: fixture.turn_id }
        }
        Op::CancelTurn => {
            KernelCommand::CancelTurn { attempt_id: fixture.attempt_id, turn_id: fixture.turn_id }
        }
        Op::ProposeAction => KernelCommand::ProposeAction {
            turn_id: fixture.turn_id,
            action_id: fixture.action_id,
            digest: digest(70),
            actor_id: fixture.actor_id,
            role: ActorRole::Writer,
            environment_id: fixture.environment_id,
        },
        Op::AuthorizeAction => KernelCommand::AuthorizeAction { action_id: fixture.action_id },
        Op::DispatchAction => KernelCommand::DispatchAction { action_id: fixture.action_id },
        Op::CompleteAction => KernelCommand::CompleteAction { action_id: fixture.action_id },
        Op::CancelAction => KernelCommand::CancelAction { action_id: fixture.action_id },
        Op::CompleteTurn => {
            KernelCommand::CompleteTurn { attempt_id: fixture.attempt_id, turn_id: fixture.turn_id }
        }
        Op::SubmitAttempt => {
            KernelCommand::SubmitAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id }
        }
        Op::RequestReview => KernelCommand::RequestReview {
            run_id: fixture.run_id,
            attempt_id: fixture.attempt_id,
            review_id: fixture.review_id,
        },
        Op::BeginReview => KernelCommand::BeginReview { review_id: fixture.review_id },
        Op::SubmitReview => KernelCommand::SubmitReview { review_id: fixture.review_id },
        Op::BeginAcceptance => KernelCommand::BeginAcceptance { run_id: fixture.run_id },
        Op::EvaluateAcceptance { .. } => {
            KernelCommand::EvaluateAcceptance { run_id: fixture.run_id }
        }
    }
}

fn assert_projection(state: &KernelAggregate, model: ReferenceModel, fixture: &Fixture) {
    assert_eq!(state.session().phase(), model.session);
    assert_eq!(state.last_sequence().get(), model.sequence);
    assert_eq!(state.run(fixture.run_id).map(|value| value.phase()), model.run);
    assert_eq!(state.attempt(fixture.attempt_id).map(|value| value.phase()), model.attempt);
    assert_eq!(state.turn(fixture.turn_id).map(|value| value.phase()), model.turn);
    assert_eq!(
        state.action(fixture.action_id).map(peritus_kernel::ActionState::phase),
        model.action
    );
    assert_eq!(state.review(fixture.review_id).map(|value| value.phase()), model.review);
    assert_eq!(state.run(fixture.run_id).map(|value| value.acceptance()), model.acceptance);
    assert!(state.is_valid());
}
