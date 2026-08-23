//! Positive and negative lifecycle transition matrix.

mod support;

use peritus_kernel::{
    AcceptanceOutcome, AcceptancePhase, ActionPhase, AttemptPhase, KernelCommand, KernelErrorKind,
    KernelEventKind, ReducerInputs, ReviewPhase, RunPhase, SessionPhase, TurnPhase, WaiverPhase,
};
use peritus_types::FindingId;
use support::lifecycle::{
    active_turn, authorized_action, evaluating_acceptance, next, running_attempt, started_run,
    submitted_review,
};
use support::{Fixture, applied, bytes, envelope, execute};

#[test]
fn session_and_run_control_matrix_accepts_only_declared_edges() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let (root_budget, _) = fixture.budget_snapshots();
    let state = fixture.genesis(&contract);

    let paused =
        applied(execute(state, 62, KernelCommand::PauseSession, ReducerInputs::new(&contract)))
            .into_parts()
            .0;
    assert_eq!(paused.session().phase(), SessionPhase::Paused);

    let rejected = paused
        .clone()
        .reduce(envelope(&paused, 63), KernelCommand::PauseSession, ReducerInputs::new(&contract))
        .into_result()
        .expect_err("double pause");
    assert_eq!(rejected.1.kind(), KernelErrorKind::IllegalPhase);
    assert_eq!(rejected.0, paused);

    let open =
        applied(execute(paused, 64, KernelCommand::ResumeSession, ReducerInputs::new(&contract)))
            .into_parts()
            .0;
    let running = applied(execute(
        open,
        65,
        KernelCommand::StartRun { run_id: fixture.run_id },
        ReducerInputs::new(&contract).with_run_budget(root_budget),
    ))
    .into_parts()
    .0;
    assert_eq!(running.run(fixture.run_id).expect("run").phase(), RunPhase::Pending);

    let error = running
        .clone()
        .reduce(envelope(&running, 66), KernelCommand::CloseSession, ReducerInputs::new(&contract))
        .into_result()
        .expect_err("live run blocks close");
    assert_eq!(error.1.kind(), KernelErrorKind::LiveChild);
    assert_eq!(error.0, running);

    let terminal = applied(execute(
        running,
        67,
        KernelCommand::CancelRun { run_id: fixture.run_id },
        ReducerInputs::new(&contract),
    ))
    .into_parts()
    .0;
    assert_eq!(terminal.run(fixture.run_id).expect("run").phase(), RunPhase::Cancelled);
    let closed =
        applied(execute(terminal, 68, KernelCommand::CloseSession, ReducerInputs::new(&contract)))
            .into_parts()
            .0;
    assert_eq!(closed.session().phase(), SessionPhase::Closed);
}

#[test]
fn missing_parents_and_illegal_child_edges_fail_without_events() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let state = fixture.genesis(&contract);
    let before_sequence = state.last_sequence();

    let rejected = execute(
        state,
        62,
        KernelCommand::StartAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id },
        ReducerInputs::new(&contract),
    )
    .into_result()
    .expect_err("missing run");
    assert_eq!(rejected.1.kind(), KernelErrorKind::MissingEntity);
    assert_eq!(rejected.0.last_sequence(), before_sequence);
    assert!(rejected.0.attempts().is_empty());
}

#[test]
fn run_pause_resume_and_all_non_acceptance_terminal_edges_are_explicit() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let running = running_attempt(&fixture, &contract);
    let paused = next(
        running,
        64,
        KernelCommand::PauseRun { run_id: fixture.run_id },
        ReducerInputs::new(&contract),
    );
    assert_eq!(paused.run(fixture.run_id).expect("run").phase(), RunPhase::Paused);
    let resumed = next(
        paused,
        65,
        KernelCommand::ResumeRun { run_id: fixture.run_id },
        ReducerInputs::new(&contract),
    );
    assert_eq!(resumed.run(fixture.run_id).expect("run").phase(), RunPhase::Running);

    let terminal_cases = [
        (
            KernelCommand::CancelRun { run_id: fixture.run_id },
            KernelEventKind::RunCancelled,
            RunPhase::Cancelled,
        ),
        (
            KernelCommand::FailRun { run_id: fixture.run_id },
            KernelEventKind::RunFailed,
            RunPhase::Failed,
        ),
        (
            KernelCommand::ExhaustRun { run_id: fixture.run_id },
            KernelEventKind::RunExhausted,
            RunPhase::Exhausted,
        ),
        (
            KernelCommand::RejectRun { run_id: fixture.run_id },
            KernelEventKind::RunRejected,
            RunPhase::Rejected,
        ),
    ];
    for (command, event_kind, phase) in terminal_cases {
        let transition = applied(execute(
            started_run(&fixture, &contract),
            63,
            command,
            ReducerInputs::new(&contract),
        ));
        assert_eq!(transition.event().kind(), event_kind);
        assert_eq!(transition.acceptance_outcome(), None);
        let state = transition.into_parts().0;
        let run = state.run(fixture.run_id).expect("terminal run");
        assert_eq!(run.phase(), phase);
        assert_eq!(run.acceptance(), AcceptancePhase::Terminated);
        assert_ne!(run.phase(), RunPhase::Accepted);
    }
}

#[test]
fn attempt_turn_and_action_families_cover_progress_failure_and_rejection() {
    let fixture = Fixture::new();
    let contract = fixture.contract();

    let turn = active_turn(&fixture, &contract);
    let live_action = next(
        turn,
        65,
        KernelCommand::ProposeAction {
            turn_id: fixture.turn_id,
            action_id: fixture.action_id,
            digest: support::digest(70),
            actor_id: fixture.actor_id,
            role: peritus_policy::ActorRole::Writer,
            environment_id: fixture.environment_id,
        },
        ReducerInputs::new(&contract),
    );
    let rejected = execute(
        live_action,
        66,
        KernelCommand::CompleteTurn { attempt_id: fixture.attempt_id, turn_id: fixture.turn_id },
        ReducerInputs::new(&contract),
    )
    .into_result()
    .expect_err("live action blocks turn completion");
    assert_eq!(rejected.1.kind(), KernelErrorKind::LiveChild);

    let authorized = authorized_action(&fixture, &contract);
    let dispatched = next(
        authorized,
        67,
        KernelCommand::DispatchAction { action_id: fixture.action_id },
        ReducerInputs::new(&contract),
    );
    let failed_action = next(
        dispatched,
        68,
        KernelCommand::FailAction { action_id: fixture.action_id },
        ReducerInputs::new(&contract),
    );
    assert_eq!(
        failed_action.action(fixture.action_id).expect("action").phase(),
        ActionPhase::Failed
    );
    let failed_turn = next(
        failed_action,
        69,
        KernelCommand::FailTurn { attempt_id: fixture.attempt_id, turn_id: fixture.turn_id },
        ReducerInputs::new(&contract),
    );
    assert_eq!(failed_turn.turn(fixture.turn_id).expect("turn").phase(), TurnPhase::Failed);
    let failed_attempt = next(
        failed_turn,
        70,
        KernelCommand::FailAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id },
        ReducerInputs::new(&contract),
    );
    assert_eq!(
        failed_attempt.attempt(fixture.attempt_id).expect("attempt").phase(),
        AttemptPhase::Failed
    );
    assert_eq!(failed_attempt.run(fixture.run_id).expect("run").phase(), RunPhase::Running);
}

#[test]
fn review_waiver_and_acceptance_families_cover_terminal_and_illegal_edges() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let submitted = submitted_review(&fixture, &contract);

    let rejected = execute(
        submitted.clone(),
        74,
        KernelCommand::SubmitReview { review_id: fixture.review_id },
        ReducerInputs::new(&contract),
    )
    .into_result()
    .expect_err("submitted review cannot submit twice");
    assert_eq!(rejected.1.kind(), KernelErrorKind::IllegalPhase);
    let invalidated = next(
        submitted.clone(),
        74,
        KernelCommand::InvalidateReview { review_id: fixture.review_id },
        ReducerInputs::new(&contract),
    );
    assert_eq!(
        invalidated.review(fixture.review_id).expect("review").phase(),
        ReviewPhase::Invalidated
    );

    let finding_id = FindingId::new(bytes(91)).expect("finding");
    let requested = next(
        submitted,
        74,
        KernelCommand::RequestWaiver {
            run_id: fixture.run_id,
            review_id: fixture.review_id,
            finding_id,
        },
        ReducerInputs::new(&contract),
    );
    let denied = next(
        requested,
        75,
        KernelCommand::DenyWaiver { finding_id },
        ReducerInputs::new(&contract),
    );
    assert_eq!(denied.waiver(finding_id).expect("waiver").phase(), WaiverPhase::Denied);
    let invalidated = next(
        denied,
        76,
        KernelCommand::InvalidateWaiver { finding_id },
        ReducerInputs::new(&contract),
    );
    assert_eq!(invalidated.waiver(finding_id).expect("waiver").phase(), WaiverPhase::Invalidated);

    let evaluating = evaluating_acceptance(&fixture, &contract);
    let rejected = execute(
        evaluating,
        75,
        KernelCommand::BeginAcceptance { run_id: fixture.run_id },
        ReducerInputs::new(&contract),
    )
    .into_result()
    .expect_err("acceptance cannot begin twice");
    assert_eq!(rejected.1.kind(), KernelErrorKind::IllegalPhase);
}

#[test]
fn approved_exact_revision_waiver_can_participate_in_acceptance() {
    let fixture = Fixture::new();
    let contract = fixture.waiver_contract();
    let finding_id = FindingId::new(bytes(91)).expect("finding");
    let state = next(
        submitted_review(&fixture, &contract),
        74,
        KernelCommand::RequestWaiver {
            run_id: fixture.run_id,
            review_id: fixture.review_id,
            finding_id,
        },
        ReducerInputs::new(&contract),
    );
    let evidence = fixture.waiver_evidence(&contract, finding_id);
    let state = next(
        state,
        75,
        KernelCommand::GrantWaiver { finding_id },
        ReducerInputs::new(&contract).with_acceptance_evidence(&evidence),
    );
    assert_eq!(state.waiver(finding_id).expect("waiver").phase(), WaiverPhase::Granted);
    let state = next(
        state,
        76,
        KernelCommand::BeginAcceptance { run_id: fixture.run_id },
        ReducerInputs::new(&contract),
    );
    let transition = applied(execute(
        state,
        77,
        KernelCommand::EvaluateAcceptance { run_id: fixture.run_id },
        ReducerInputs::new(&contract).with_acceptance_evidence(&evidence),
    ));
    assert_eq!(transition.acceptance_outcome(), Some(AcceptanceOutcome::Accepted));
    assert_eq!(
        transition.aggregate().run(fixture.run_id).expect("run").phase(),
        RunPhase::Accepted
    );
}

#[test]
fn needs_changes_attempt_resumes_only_through_the_explicit_fixer_edge() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let evidence = fixture.incomplete_evidence(&contract, fixture.revision, fixture.review_id);
    let fixing = next(
        evaluating_acceptance(&fixture, &contract),
        75,
        KernelCommand::EvaluateAcceptance { run_id: fixture.run_id },
        ReducerInputs::new(&contract).with_acceptance_evidence(&evidence),
    );
    let active = next(
        fixing,
        76,
        KernelCommand::ResumeAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id },
        ReducerInputs::new(&contract),
    );
    assert_eq!(active.run(fixture.run_id).expect("run").phase(), RunPhase::Running);
    assert_eq!(active.attempt(fixture.attempt_id).expect("attempt").phase(), AttemptPhase::Active);
    assert_eq!(active.run(fixture.run_id).expect("run").acceptance(), AcceptancePhase::Pending);
    let rejected = execute(
        active,
        77,
        KernelCommand::ResumeAttempt { run_id: fixture.run_id, attempt_id: fixture.attempt_id },
        ReducerInputs::new(&contract),
    )
    .into_result()
    .expect_err("active attempt cannot resume twice");
    assert_eq!(rejected.1.kind(), KernelErrorKind::IllegalPhase);
}
