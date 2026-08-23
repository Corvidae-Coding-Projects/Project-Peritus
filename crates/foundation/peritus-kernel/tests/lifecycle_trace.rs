//! End-to-end lifecycle traces through B1 authority and B2 acceptance.

mod support;

use peritus_kernel::{
    AcceptanceOutcome, ActionPhase, AttemptPhase, KernelCommand, ReducerInputs, ReviewPhase,
    RunPhase, TurnPhase,
};
use support::lifecycle::evaluating_acceptance;
use support::{Fixture, applied, execute};

#[test]
fn complete_writer_action_review_acceptance_trace_is_causal() {
    let fixture = Fixture::new();
    let contract = fixture.contract();
    let state = evaluating_acceptance(&fixture, &contract);
    let evidence = fixture.evidence(&contract, fixture.revision, fixture.review_id);
    let transition = applied(execute(
        state,
        75,
        KernelCommand::EvaluateAcceptance { run_id: fixture.run_id },
        ReducerInputs::new(&contract).with_acceptance_evidence(&evidence),
    ));
    assert_eq!(transition.acceptance_outcome(), Some(AcceptanceOutcome::Accepted));
    let state = transition.into_parts().0;

    assert!(state.is_valid());
    assert_eq!(state.last_sequence().get(), 15);
    assert_eq!(state.run(fixture.run_id).expect("run").phase(), RunPhase::Accepted);
    assert_eq!(state.attempt(fixture.attempt_id).expect("attempt").phase(), AttemptPhase::Accepted);
    assert_eq!(state.turn(fixture.turn_id).expect("turn").phase(), TurnPhase::Completed);
    assert_eq!(state.action(fixture.action_id).expect("action").phase(), ActionPhase::Succeeded);
    assert_eq!(state.review(fixture.review_id).expect("review").phase(), ReviewPhase::Submitted);
}
