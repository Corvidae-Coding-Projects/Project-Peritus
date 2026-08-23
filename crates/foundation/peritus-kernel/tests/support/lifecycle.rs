//! Reusable legal lifecycle prefixes for integration tests.

use super::{Fixture, applied, digest, execute};
use peritus_kernel::{KernelAggregate, KernelCommand, ReducerInputs};
use peritus_policy::ActorRole;
use peritus_spec::AcceptanceContract;

pub fn started_run(fixture: &Fixture, contract: &AcceptanceContract) -> KernelAggregate {
    let (root_budget, _) = fixture.budget_snapshots();
    next(
        fixture.genesis(contract),
        62,
        KernelCommand::StartRun { run_id: fixture.run_id },
        ReducerInputs::new(contract).with_run_budget(root_budget),
    )
}

pub fn running_attempt(fixture: &Fixture, contract: &AcceptanceContract) -> KernelAggregate {
    let (root_budget, child_budget) = fixture.budget_snapshots();
    let state = next(
        fixture.genesis(contract),
        62,
        KernelCommand::StartRun { run_id: fixture.run_id },
        ReducerInputs::new(contract).with_run_budget(root_budget),
    );
    next(
        state,
        63,
        KernelCommand::StartAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id },
        ReducerInputs::new(contract)
            .with_attempt_budget(child_budget)
            .with_parent_budget(root_budget),
    )
}

pub fn active_turn(fixture: &Fixture, contract: &AcceptanceContract) -> KernelAggregate {
    next(
        running_attempt(fixture, contract),
        64,
        KernelCommand::StartTurn { attempt_id: fixture.attempt_id, turn_id: fixture.turn_id },
        ReducerInputs::new(contract),
    )
}

pub fn proposed_action(fixture: &Fixture, contract: &AcceptanceContract) -> KernelAggregate {
    next(
        active_turn(fixture, contract),
        65,
        KernelCommand::ProposeAction {
            turn_id: fixture.turn_id,
            action_id: fixture.action_id,
            digest: digest(70),
            actor_id: fixture.actor_id,
            role: ActorRole::Writer,
            environment_id: fixture.environment_id,
        },
        ReducerInputs::new(contract),
    )
}

pub fn authorized_action(fixture: &Fixture, contract: &AcceptanceContract) -> KernelAggregate {
    let state = proposed_action(fixture, contract);
    let capability_use = fixture.capability_use(fixture.action_id, digest(70));
    next(
        state,
        66,
        KernelCommand::AuthorizeAction { action_id: fixture.action_id },
        ReducerInputs::new(contract).with_capability_use(&capability_use),
    )
}

pub fn submitted_candidate(fixture: &Fixture, contract: &AcceptanceContract) -> KernelAggregate {
    let state = next(
        authorized_action(fixture, contract),
        67,
        KernelCommand::DispatchAction { action_id: fixture.action_id },
        ReducerInputs::new(contract),
    );
    let state = next(
        state,
        68,
        KernelCommand::CompleteAction { action_id: fixture.action_id },
        ReducerInputs::new(contract),
    );
    let state = next(
        state,
        69,
        KernelCommand::CompleteTurn { attempt_id: fixture.attempt_id, turn_id: fixture.turn_id },
        ReducerInputs::new(contract),
    );
    next(
        state,
        70,
        KernelCommand::SubmitAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id },
        ReducerInputs::new(contract),
    )
}

pub fn submitted_review(fixture: &Fixture, contract: &AcceptanceContract) -> KernelAggregate {
    let state = next(
        submitted_candidate(fixture, contract),
        71,
        KernelCommand::RequestReview {
            run_id: fixture.run_id,
            attempt_id: fixture.attempt_id,
            review_id: fixture.review_id,
        },
        ReducerInputs::new(contract),
    );
    let state = next(
        state,
        72,
        KernelCommand::BeginReview { review_id: fixture.review_id },
        ReducerInputs::new(contract),
    );
    next(
        state,
        73,
        KernelCommand::SubmitReview { review_id: fixture.review_id },
        ReducerInputs::new(contract),
    )
}

pub fn evaluating_acceptance(fixture: &Fixture, contract: &AcceptanceContract) -> KernelAggregate {
    next(
        submitted_review(fixture, contract),
        74,
        KernelCommand::BeginAcceptance { run_id: fixture.run_id },
        ReducerInputs::new(contract),
    )
}

pub fn next(
    state: KernelAggregate,
    value: u8,
    command: KernelCommand,
    inputs: ReducerInputs<'_>,
) -> KernelAggregate {
    applied(execute(state, value, command, inputs)).into_parts().0
}
