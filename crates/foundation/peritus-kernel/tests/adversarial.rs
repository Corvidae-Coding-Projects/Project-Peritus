//! Adversarial authority, causality, freshness, and replay checks.

mod support;

use peritus_kernel::{
    AcceptanceOutcome, AcceptancePhase, CommandEnvelope, KernelAggregate, KernelCommand,
    KernelErrorKind, KernelEventKind, KernelOutcome, ReducerInputs, RunPhase,
};
use peritus_types::{CommandId, EventId, ReviewCycleId};
use support::lifecycle::{evaluating_acceptance, proposed_action};
use support::{Fixture, applied, bytes, digest, execute};

#[test]
fn revision_forks_and_replayed_identities_preserve_the_aggregate() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let genesis = fixture.genesis(&contract);

    let stale = CommandEnvelope::new(
        CommandId::new(bytes(62)).expect("command"),
        EventId::new(bytes(142)).expect("event"),
        Some(genesis.head_event_id()),
        fixture.alternate_revision(),
    );
    assert_rejected(
        &genesis,
        genesis.clone().reduce(stale, KernelCommand::PauseSession, ReducerInputs::new(&contract)),
        KernelErrorKind::RevisionMismatch,
    );

    let fork = CommandEnvelope::new(
        CommandId::new(bytes(62)).expect("command"),
        EventId::new(bytes(142)).expect("event"),
        Some(EventId::new(bytes(99)).expect("wrong head")),
        fixture.revision,
    );
    assert_rejected(
        &genesis,
        genesis.clone().reduce(fork, KernelCommand::PauseSession, ReducerInputs::new(&contract)),
        KernelErrorKind::CausalHeadMismatch,
    );

    let paused =
        applied(execute(genesis, 62, KernelCommand::PauseSession, ReducerInputs::new(&contract)))
            .into_parts()
            .0;
    assert_replayed_identities_are_rejected(&paused, &contract);
}

#[test]
fn run_and_attempt_admission_fail_closed_on_budget_inputs() {
    let fixture = Fixture::new();
    let contract = fixture.contract();

    let state = fixture.genesis(&contract);
    assert_rejected(
        &state,
        execute(
            state.clone(),
            62,
            KernelCommand::StartRun { run_id: fixture.run_id },
            ReducerInputs::new(&contract),
        ),
        KernelErrorKind::MissingAuthorityInput,
    );

    let stale_budget = fixture.root_budget_snapshot(fixture.alternate_revision(), false);
    let state = fixture.genesis(&contract);
    assert_rejected(
        &state,
        execute(
            state.clone(),
            62,
            KernelCommand::StartRun { run_id: fixture.run_id },
            ReducerInputs::new(&contract).with_run_budget(stale_budget),
        ),
        KernelErrorKind::AuthorityMismatch,
    );

    let closed_budget = fixture.root_budget_snapshot(fixture.revision, true);
    let state = fixture.genesis(&contract);
    assert_rejected(
        &state,
        execute(
            state.clone(),
            62,
            KernelCommand::StartRun { run_id: fixture.run_id },
            ReducerInputs::new(&contract).with_run_budget(closed_budget),
        ),
        KernelErrorKind::BudgetUnavailable,
    );
}

#[test]
fn action_authorization_requires_the_exact_capability_use() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let proposed = proposed_action(&fixture, &contract);

    assert_rejected(
        &proposed,
        execute(
            proposed.clone(),
            66,
            KernelCommand::AuthorizeAction { action_id: fixture.action_id },
            ReducerInputs::new(&contract),
        ),
        KernelErrorKind::MissingAuthorityInput,
    );

    let wrong_use = fixture.capability_use(fixture.action_id, digest(71));
    assert_rejected(
        &proposed,
        execute(
            proposed.clone(),
            66,
            KernelCommand::AuthorizeAction { action_id: fixture.action_id },
            ReducerInputs::new(&contract).with_capability_use(&wrong_use),
        ),
        KernelErrorKind::AuthorityMismatch,
    );
}

#[test]
fn acceptance_rejects_missing_stale_and_untracked_evidence() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let evaluating = evaluating_acceptance(&fixture, &contract);

    assert_rejected(
        &evaluating,
        execute(
            evaluating.clone(),
            75,
            KernelCommand::EvaluateAcceptance { run_id: fixture.run_id },
            ReducerInputs::new(&contract),
        ),
        KernelErrorKind::MissingAuthorityInput,
    );

    let stale = fixture.evidence(&contract, fixture.alternate_revision(), fixture.review_id);
    assert_rejected(
        &evaluating,
        execute(
            evaluating.clone(),
            75,
            KernelCommand::EvaluateAcceptance { run_id: fixture.run_id },
            ReducerInputs::new(&contract).with_acceptance_evidence(&stale),
        ),
        KernelErrorKind::AuthorityMismatch,
    );

    let unknown_review = ReviewCycleId::new(bytes(90)).expect("unknown review");
    let untracked = fixture.evidence(&contract, fixture.revision, unknown_review);
    assert_rejected(
        &evaluating,
        execute(
            evaluating.clone(),
            75,
            KernelCommand::EvaluateAcceptance { run_id: fixture.run_id },
            ReducerInputs::new(&contract).with_acceptance_evidence(&untracked),
        ),
        KernelErrorKind::AuthorityMismatch,
    );
}

#[test]
fn incomplete_current_evidence_enters_fixer_state_without_implicit_success() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let evaluating = evaluating_acceptance(&fixture, &contract);
    let incomplete = fixture.incomplete_evidence(&contract, fixture.revision, fixture.review_id);

    let transition = applied(execute(
        evaluating,
        75,
        KernelCommand::EvaluateAcceptance { run_id: fixture.run_id },
        ReducerInputs::new(&contract).with_acceptance_evidence(&incomplete),
    ));
    assert_eq!(transition.event().kind(), KernelEventKind::AcceptanceNeedsChanges);
    assert!(matches!(
        transition.acceptance_outcome(),
        Some(AcceptanceOutcome::NeedsChanges { unmet_conditions }) if unmet_conditions > 0
    ));
    let state = transition.into_parts().0;
    let run = state.run(fixture.run_id).expect("run");
    assert_eq!(run.phase(), RunPhase::Fixing);
    assert_eq!(run.acceptance(), AcceptancePhase::NeedsChanges);
}

fn assert_replayed_identities_are_rejected(
    state: &KernelAggregate,
    contract: &peritus_spec::AcceptanceContract,
) {
    let duplicate_command = CommandEnvelope::new(
        CommandId::new(bytes(62)).expect("duplicate command"),
        EventId::new(bytes(143)).expect("new event"),
        Some(state.head_event_id()),
        state.revision(),
    );
    assert_rejected(
        state,
        state.clone().reduce(
            duplicate_command,
            KernelCommand::ResumeSession,
            ReducerInputs::new(contract),
        ),
        KernelErrorKind::DuplicateCommand,
    );

    let duplicate_event = CommandEnvelope::new(
        CommandId::new(bytes(63)).expect("new command"),
        EventId::new(bytes(142)).expect("duplicate event"),
        Some(state.head_event_id()),
        state.revision(),
    );
    assert_rejected(
        state,
        state.clone().reduce(
            duplicate_event,
            KernelCommand::ResumeSession,
            ReducerInputs::new(contract),
        ),
        KernelErrorKind::DuplicateEvent,
    );
}

fn assert_rejected(expected: &KernelAggregate, outcome: KernelOutcome, kind: KernelErrorKind) {
    let (returned, error) = outcome.into_result().expect_err("command must reject");
    assert_eq!(error.kind(), kind);
    assert_eq!(&returned, expected);
    assert_eq!(returned.last_sequence(), expected.last_sequence());
    assert_eq!(returned.head_event_id(), expected.head_event_id());
}
