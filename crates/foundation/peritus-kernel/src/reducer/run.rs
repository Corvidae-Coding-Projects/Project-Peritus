//! Run admission, control, and terminal transitions.

use super::AppliedCommand;
use crate::{
    AcceptancePhase, ActionPhase, AttemptPhase, AuthorityInputKind, KernelAggregate, KernelCommand,
    KernelError, KernelErrorKind, KernelEventKind, KernelSubject, LifecycleEntity, ReducerInputs,
    RunPhase, RunState, SessionPhase, TurnPhase,
};
use peritus_budget::BudgetAccountPhase;
use peritus_types::RunId;
use vstd::prelude::*;

verus! {

pub(super) fn apply(
    state: &mut KernelAggregate,
    command: &KernelCommand,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    match command {
        KernelCommand::StartRun { run_id } => start(state, *run_id, inputs),
        KernelCommand::PauseRun { run_id } => pause(state, *run_id),
        KernelCommand::ResumeRun { run_id } => resume(state, *run_id),
        KernelCommand::CancelRun { run_id } => terminate(
            state, *run_id, RunPhase::Cancelled, AttemptPhase::Cancelled,
            TurnPhase::Cancelled, ActionPhase::Cancelled, KernelEventKind::RunCancelled,
        ),
        KernelCommand::FailRun { run_id } => terminate(
            state, *run_id, RunPhase::Failed, AttemptPhase::Failed,
            TurnPhase::Failed, ActionPhase::Failed, KernelEventKind::RunFailed,
        ),
        KernelCommand::ExhaustRun { run_id } => terminate(
            state, *run_id, RunPhase::Exhausted, AttemptPhase::Exhausted,
            TurnPhase::Cancelled, ActionPhase::Cancelled, KernelEventKind::RunExhausted,
        ),
        KernelCommand::RejectRun { run_id } => terminate(
            state, *run_id, RunPhase::Rejected, AttemptPhase::Failed,
            TurnPhase::Cancelled, ActionPhase::Cancelled, KernelEventKind::RunRejected,
        ),
        _ => Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Run)),
    }
}

fn start(
    state: &mut KernelAggregate,
    run_id: RunId,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    if state.session.phase() != SessionPhase::Open {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Session));
    }
    if state.run(run_id).is_some() {
        return Err(KernelError::entity(KernelErrorKind::DuplicateEntity, LifecycleEntity::Run));
    }
    if state.has_live_run() {
        return Err(KernelError::entity(KernelErrorKind::LiveChild, LifecycleEntity::Run));
    }
    let Some(snapshot) = inputs.run_budget() else {
        return Err(KernelError::authority(
            KernelErrorKind::MissingAuthorityInput,
            AuthorityInputKind::RunBudget,
        ));
    };
    if snapshot.revision() != state.revision || snapshot.parent_id().is_some() {
        return Err(KernelError::authority(
            KernelErrorKind::AuthorityMismatch,
            AuthorityInputKind::RunBudget,
        ));
    }
    if snapshot.phase() != BudgetAccountPhase::Open {
        return Err(KernelError::authority(
            KernelErrorKind::BudgetUnavailable,
            AuthorityInputKind::RunBudget,
        ));
    }
    state.runs.push(RunState::pending(
        run_id,
        state.revision,
        snapshot.id(),
        snapshot.limits(),
    ));
    Ok(AppliedCommand::new(KernelEventKind::RunStarted, KernelSubject::Run(run_id)))
}

fn pause(state: &mut KernelAggregate, run_id: RunId) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.run_index(run_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Run));
    };
    if state.session.phase() != SessionPhase::Open
        || !matches!(state.runs[index].phase(), RunPhase::Running | RunPhase::Fixing)
    {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Run));
    }
    state.runs[index].set_phase(RunPhase::Paused);
    Ok(AppliedCommand::new(KernelEventKind::RunPaused, KernelSubject::Run(run_id)))
}

fn resume(state: &mut KernelAggregate, run_id: RunId) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.run_index(run_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Run));
    };
    if state.session.phase() != SessionPhase::Open || state.runs[index].phase() != RunPhase::Paused {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Run));
    }
    state.runs[index].set_phase(RunPhase::Running);
    Ok(AppliedCommand::new(KernelEventKind::RunResumed, KernelSubject::Run(run_id)))
}

#[allow(
    clippy::option_if_let_else,
    reason = "explicit parent traversal mirrors the Verus lifecycle relation"
)]
fn terminate(
    state: &mut KernelAggregate,
    run_id: RunId,
    run_phase: RunPhase,
    attempt_phase: AttemptPhase,
    turn_phase: TurnPhase,
    action_phase: ActionPhase,
    event_kind: KernelEventKind,
) -> Result<AppliedCommand, KernelError> {
    let Some(run_index) = state.run_index(run_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Run));
    };
    if state.runs[run_index].phase().is_terminal() {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Run));
    }
    let mut attempt_index = 0;
    while attempt_index < state.attempts.len()
        invariant
            attempt_index <= state.attempts.len(),
            (run_index as int) < state.runs@.len(),
        decreases state.attempts.len() - attempt_index,
    {
        if state.attempts[attempt_index].run_id() == run_id
            && !state.attempts[attempt_index].phase().is_terminal()
        {
            state.attempts[attempt_index].set_phase(attempt_phase);
        }
        attempt_index += 1;
    }
    let mut turn_index = 0;
    while turn_index < state.turns.len()
        invariant
            turn_index <= state.turns.len(),
            (run_index as int) < state.runs@.len(),
        decreases state.turns.len() - turn_index,
    {
        let attempt_id = state.turns[turn_index].attempt_id();
        let belongs = match state.attempt(attempt_id) {
            Some(attempt) => attempt.run_id() == run_id,
            None => false,
        };
        if belongs && !state.turns[turn_index].phase().is_terminal() {
            state.turns[turn_index].set_phase(turn_phase);
        }
        turn_index += 1;
    }
    let mut action_index = 0;
    while action_index < state.actions.len()
        invariant
            action_index <= state.actions.len(),
            (run_index as int) < state.runs@.len(),
        decreases state.actions.len() - action_index,
    {
        let turn_id = state.actions[action_index].turn_id();
        let belongs = match state.turn(turn_id) {
            Some(turn) => match state.attempt(turn.attempt_id()) {
                Some(attempt) => attempt.run_id() == run_id,
                None => false,
            },
            None => false,
        };
        if belongs && !state.actions[action_index].phase().is_terminal() {
            state.actions[action_index].set_phase(action_phase);
        }
        action_index += 1;
    }
    state.runs[run_index].set_phase(run_phase);
    state.runs[run_index].set_acceptance(AcceptancePhase::Terminated);
    state.runs[run_index].set_current_attempt(None);
    Ok(AppliedCommand::new(event_kind, KernelSubject::Run(run_id)))
}

} // verus!
