use std::sync::{Arc, Mutex};

use peritus_product_runner::{ProductRunPhase, ProductRunner, ProductRunnerErrorKind};
use peritus_run_settlement::{CandidateStage, RunDisposition};
use serde::Deserialize;

use super::{
    fixture::{Expected, FixtureSet},
    product_fixture::{clean_review, complete_writer, input, repository, roles, scripted},
};

const COMPLETION: &str =
    include_str!("../../tests/fixtures/general-capability/completion/cases.json");
const RESUME: &str = include_str!("../../tests/fixtures/general-capability/resume/cases.json");

#[derive(Deserialize)]
struct Case {
    name: String,
    expected: Expected,
}

#[test]
fn candidate_handoff_and_phase_selective_resume_are_public_truth() {
    let completion: FixtureSet<Case> = serde_json::from_str(COMPLETION).expect("completion cases");
    let resume_cases: FixtureSet<Case> = serde_json::from_str(RESUME).expect("resume cases");
    assert_case_shapes(&completion, &resume_cases);

    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    runtime.block_on(async {
        let active_repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x11, "writer", complete_writer());
        let unavailable_reviewer = scripted(0x12, "unavailable-reviewer", Vec::new());
        let first = ProductRunner::run(
            input(
                &active_repository,
                &state,
                0x13,
                0x14,
                roles(writer.clone(), unavailable_reviewer, writer.clone()),
                None,
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect("reviewer failure settles");

        assert_eq!(first.settlement().disposition(), RunDisposition::CandidateAvailable);
        assert_eq!(
            first.settlement().checkpoint().expect("checkpoint").stage(),
            CandidateStage::ReviewPending
        );
        assert!(first.candidate().expect("candidate").diff.contains("answer"));
        let resume = first.resume().expect("resume").clone();
        assert_eq!(resume.next_phase(), ProductRunPhase::Reviewing);

        let foreign = ProductRunner::run(
            input(
                &active_repository,
                &state,
                0x13,
                0x15,
                roles(writer.clone(), writer.clone(), writer.clone()),
                Some(resume.clone()),
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect_err("foreign workspace resume");
        assert_eq!(foreign.kind(), ProductRunnerErrorKind::InvalidPrecondition);

        let writer_starts = writer.starts();
        let phases = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&phases);
        let reviewer = scripted(0x16, "reviewer", clean_review());
        let accepted = ProductRunner::run(
            input(
                &active_repository,
                &state,
                0x13,
                0x14,
                roles(writer.clone(), reviewer, writer.clone()),
                Some(resume),
            ),
            Arc::new(move |update| observed.lock().expect("phase log").push(update.phase)),
        )
        .await
        .expect("resume succeeds");
        assert!(accepted.settlement().is_accepted());
        assert_eq!(writer.starts(), writer_starts, "writer reran after completed write and gates");
        let phases = phases.lock().expect("phase log").clone();
        assert!(phases.contains(&ProductRunPhase::Reviewing));
        assert!(phases.contains(&ProductRunPhase::Finalizing));
        assert!(
            !phases.iter().any(|phase| matches!(
                phase,
                ProductRunPhase::Designing | ProductRunPhase::Writing | ProductRunPhase::Checking
            )),
            "unexpected resumed phases: {phases:?}"
        );

        let empty_repository = repository();
        let empty_state = tempfile::tempdir().expect("empty state");
        let unavailable = scripted(0x21, "unavailable-writer", Vec::new());
        let failed = ProductRunner::run(
            input(
                &empty_repository,
                &empty_state,
                0x22,
                0x23,
                roles(unavailable.clone(), unavailable.clone(), unavailable),
                None,
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect("pre-artifact provider failure settles");
        assert_eq!(failed.settlement().disposition(), RunDisposition::FailedNoCandidate);
        assert!(failed.candidate().is_none());
    });
}

fn assert_case_shapes(completion: &FixtureSet<Case>, resume: &FixtureSet<Case>) {
    let completion_outcomes = completion.cases.iter().map(|case| case.expected).collect::<Vec<_>>();
    let resume_outcomes = resume.cases.iter().map(|case| case.expected).collect::<Vec<_>>();
    assert_eq!(completion_outcomes, [Expected::Success, Expected::Partial, Expected::Failure]);
    assert_eq!(resume_outcomes, [Expected::Success, Expected::Partial, Expected::Failure]);
    assert!(completion.cases.iter().all(|case| !case.name.is_empty()));
    assert!(resume.cases.iter().all(|case| !case.name.is_empty()));
}
