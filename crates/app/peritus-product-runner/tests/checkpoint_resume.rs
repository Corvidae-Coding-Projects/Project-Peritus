//! Failure-boundary settlement and phase-selective resume regressions.

#[allow(dead_code, reason = "shared integration support exposes helpers used by sibling tests")]
#[path = "production_composition/support.rs"]
mod support;

use std::{
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Duration,
};

use peritus_product_runner::{ProductRunPhase, ProductRunner, ProductRunnerErrorKind, RunObserver};
use peritus_run_settlement::{CandidateStage, RunDisposition, SettlementCause};

#[path = "checkpoint_resume/fixtures.rs"]
mod fixtures;

use fixtures::*;

#[test]
fn provider_failure_after_mutation_returns_the_exact_candidate() {
    run_async(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x11, "partial-writer", partial_writer(CORRECT));
        let outcome = ProductRunner::run(
            input(
                &repository,
                &state,
                0x12,
                0x13,
                roles(writer.clone(), writer.clone(), writer),
                Arc::new(AtomicBool::new(false)),
                Duration::from_mins(1),
                None,
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect("ordinary provider settlement");

        assert_eq!(outcome.settlement().disposition(), RunDisposition::CandidateAvailable);
        assert_eq!(outcome.settlement().cause(), SettlementCause::Provider);
        assert_eq!(outcome.resume().expect("resume").next_phase(), ProductRunPhase::Writing);
        assert!(outcome.candidate().expect("candidate").diff.contains("answer"));
        assert!(outcome.remaining_work().iter().any(|item| item.contains("gates")));
    });
}

#[test]
fn reviewer_failure_resumes_review_without_design_writer_or_gates() {
    run_async(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x21, "writer", complete_writer(CORRECT));
        let unavailable_reviewer = scripted(0x22, "offline-reviewer", Vec::new());
        let first = ProductRunner::run(
            input(
                &repository,
                &state,
                0x23,
                0x24,
                roles(writer.clone(), unavailable_reviewer, writer.clone()),
                Arc::new(AtomicBool::new(false)),
                Duration::from_mins(1),
                None,
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect("review failure settles");
        assert_eq!(first.settlement().disposition(), RunDisposition::CandidateAvailable);
        assert_eq!(
            first.settlement().checkpoint().expect("checkpoint").stage(),
            CandidateStage::ReviewPending,
            "{}",
            first.candidate().expect("candidate").gates,
        );
        let resume = first.resume().expect("resume").clone();

        let wrong_workspace = ProductRunner::run(
            input(
                &repository,
                &state,
                0x23,
                0x25,
                roles(writer.clone(), writer.clone(), writer.clone()),
                Arc::new(AtomicBool::new(false)),
                Duration::from_mins(1),
                Some(resume.clone()),
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect_err("foreign workspace must be rejected");
        assert_eq!(wrong_workspace.kind(), ProductRunnerErrorKind::InvalidPrecondition);

        let reviewer = scripted(0x26, "recovered-reviewer", clean_review());
        let phases = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&phases);
        let second = ProductRunner::run(
            input(
                &repository,
                &state,
                0x23,
                0x24,
                roles(writer.clone(), reviewer, writer),
                Arc::new(AtomicBool::new(false)),
                Duration::from_mins(1),
                Some(resume),
            ),
            Arc::new(move |update| observed.lock().expect("phases").push(update.phase)),
        )
        .await
        .expect("review resumes");

        assert!(second.settlement().is_accepted());
        let phases = phases.lock().expect("phases").clone();
        assert!(phases.contains(&ProductRunPhase::Reviewing));
        assert!(phases.contains(&ProductRunPhase::Finalizing));
        assert!(!phases.iter().any(|phase| matches!(
            phase,
            ProductRunPhase::Designing | ProductRunPhase::Writing | ProductRunPhase::Checking
        )));
    });
}

#[test]
fn cancellation_during_review_preserves_a_pending_review_candidate() {
    run_async(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x31, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0x32, "reviewer", clean_review());
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_on_review = Arc::clone(&cancelled);
        let observer: RunObserver = Arc::new(move |update| {
            if update.phase == ProductRunPhase::Reviewing {
                cancel_on_review.store(true, std::sync::atomic::Ordering::Release);
            }
        });
        let outcome = ProductRunner::run(
            input(
                &repository,
                &state,
                0x33,
                0x34,
                roles(writer.clone(), reviewer, writer),
                cancelled,
                Duration::from_mins(1),
                None,
            ),
            observer,
        )
        .await
        .expect("cancellation settles");

        assert_eq!(outcome.settlement().disposition(), RunDisposition::Cancelled);
        assert_eq!(outcome.settlement().cause(), SettlementCause::Cancellation);
        assert_eq!(
            outcome.settlement().checkpoint().expect("checkpoint").stage(),
            CandidateStage::ReviewPending,
            "{}",
            outcome.candidate().expect("candidate").gates,
        );
        assert!(outcome.candidate().is_some());
    });
}

#[test]
fn deadline_cutoff_still_runs_finalization() {
    run_async(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let phases = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&phases);
        let outcome = ProductRunner::run(
            input(
                &repository,
                &state,
                0x42,
                0x43,
                stalled_roles(0x41),
                Arc::new(AtomicBool::new(false)),
                Duration::from_secs(2),
                None,
            ),
            Arc::new(move |update| observed.lock().expect("phases").push(update.phase)),
        )
        .await
        .expect("deadline settles");

        assert_eq!(outcome.settlement().cause(), SettlementCause::Deadline);
        assert_eq!(outcome.settlement().disposition(), RunDisposition::FailedNoCandidate);
        let final_phase = phases.lock().expect("phases").last().copied();
        assert_eq!(final_phase, Some(ProductRunPhase::Finalizing));
    });
}

#[test]
fn failing_gates_remain_a_candidate_when_the_fixer_provider_fails() {
    run_async(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x51, "writer", complete_writer(UNFORMATTED));
        let reviewer = scripted(0x52, "reviewer", clean_review());
        let fixer = scripted(0x53, "offline-fixer", Vec::new());
        let outcome = ProductRunner::run(
            input(
                &repository,
                &state,
                0x54,
                0x55,
                roles(writer, reviewer, fixer),
                Arc::new(AtomicBool::new(false)),
                Duration::from_mins(1),
                None,
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect("fixer provider failure settles");

        assert_eq!(outcome.settlement().disposition(), RunDisposition::CandidateAvailable);
        assert_eq!(outcome.settlement().cause(), SettlementCause::Provider);
        assert!(matches!(
            outcome.settlement().checkpoint().expect("checkpoint").gates(),
            peritus_run_settlement::EvidenceStatus::Failed(_)
        ));
        assert!(outcome.remaining_work().iter().any(|item| item.contains("gates")));
    });
}

#[test]
fn failure_during_fix_resumes_the_fixer_then_rechecks_and_accepts() {
    run_async(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x61, "writer", complete_writer(INCORRECT));
        let reviewer = scripted(0x62, "reviewer", finding_then_clean_review());
        let interrupted_fixer = scripted(0x63, "interrupted-fixer", fixer_patch());
        let first = ProductRunner::run(
            input(
                &repository,
                &state,
                0x64,
                0x65,
                roles(writer.clone(), reviewer.clone(), interrupted_fixer),
                Arc::new(AtomicBool::new(false)),
                Duration::from_mins(1),
                None,
            ),
            Arc::new(|_| {}),
        )
        .await
        .expect("interrupted fixer settles");
        assert_eq!(first.settlement().disposition(), RunDisposition::CandidateAvailable);
        assert_eq!(first.resume().expect("resume").next_phase(), ProductRunPhase::Fixing);
        let resume = first.resume().expect("resume").clone();

        let recovered_fixer = scripted(0x66, "recovered-fixer", completed_fixer());
        let phases = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&phases);
        let second = ProductRunner::run(
            input(
                &repository,
                &state,
                0x64,
                0x65,
                roles(writer, reviewer, recovered_fixer),
                Arc::new(AtomicBool::new(false)),
                Duration::from_mins(1),
                Some(resume),
            ),
            Arc::new(move |update| observed.lock().expect("phases").push(update.phase)),
        )
        .await
        .expect("fixer resumes");

        assert!(second.settlement().is_accepted());
        let phases = phases.lock().expect("phases").clone();
        assert_eq!(phases.first(), Some(&ProductRunPhase::Fixing));
        assert!(phases.contains(&ProductRunPhase::Verifying));
        assert!(!phases.iter().any(|phase| matches!(
            phase,
            ProductRunPhase::Designing | ProductRunPhase::Writing | ProductRunPhase::Checking
        )));
    });
}
