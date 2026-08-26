//! Aggregate lifecycle, reducer, cancellation, and recovery coverage.

use peritus_debugger::{
    AnalysisCounts, DebuggerCommand, DebuggerCommandKind, DebuggerErrorKind, DebuggerJobId,
    DebuggerPhase, DebuggerRecoveryDecision, ModelAnalysisId, ModelAttemptFailure,
    ModelAttemptFailureCode, ModelBudget, ModelRetryPolicy, ModelWorkState, PublicationRecord,
    ReportId, ReportRecord, SelectionManifestId, SelectionRecord, decide, decide_recovery,
};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, EvidenceId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(1)).expect("acceptance identity"),
        HarnessId::new(bytes(2)).expect("harness identity"),
        WorkspaceId::new(bytes(3)).expect("workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(4)).expect("policy identity"),
        ProviderProfileId::new(bytes(5)).expect("provider identity"),
    )
}

fn next(
    state: Option<&peritus_debugger::DebuggerState>,
    seed: u8,
    kind: DebuggerCommandKind,
) -> DebuggerCommand {
    let (job_id, sequence, previous, prior_digest, query_digest) = state.map_or_else(
        || (DebuggerJobId::new(bytes(10)).expect("job identity"), 0, None, digest(0), digest(11)),
        |state| {
            (
                state.job_id(),
                state.sequence(),
                Some(state.last_event_id()),
                state.state_digest(),
                state.query_digest(),
            )
        },
    );
    DebuggerCommand::new(
        CommandId::new(bytes(seed)).expect("command identity"),
        EventId::new(bytes(seed.wrapping_add(1))).expect("event identity"),
        job_id,
        sequence,
        previous,
        prior_digest,
        query_digest,
        kind,
    )
    .expect("valid fenced command")
}

fn deterministic_state(model_plan: Option<Sha256Digest>) -> peritus_debugger::DebuggerState {
    let create = next(
        None,
        20,
        DebuggerCommandKind::CreateJob {
            revision: revision(),
            query_digest: digest(11),
            limits_digest: digest(12),
            model_plan_digest: model_plan,
        },
    );
    let created = decide(None, &create).expect("create job");
    let selection = SelectionRecord::new(
        SelectionManifestId::new(bytes(30)).expect("manifest identity"),
        digest(31),
        1,
        2,
    )
    .expect("selection");
    let select =
        next(Some(created.state()), 22, DebuggerCommandKind::RecordSelection { selection });
    let selected = decide(Some(created.state()), &select).expect("select evidence");
    let analyze = next(
        Some(selected.state()),
        24,
        DebuggerCommandKind::RecordDeterministicAnalysis {
            analysis_digest: digest(32),
            counts: AnalysisCounts::new(2, 1, 1),
        },
    );
    decide(Some(selected.state()), &analyze).expect("deterministic analysis").into_parts().1
}

#[test]
fn retry_is_a_separate_bounded_transition_and_recovery_action() {
    let plan_digest = digest(40);
    let deterministic = deterministic_state(Some(plan_digest));
    let model_id = ModelAnalysisId::new(bytes(41)).expect("model identity");
    let request = next(
        Some(&deterministic),
        42,
        DebuggerCommandKind::RequestModelAnalysis {
            model_id,
            plan_digest,
            request_digest: digest(43),
            budget: ModelBudget::new(16, 4096, 1000, 1000, 1500).expect("model budget"),
            retry_policy: ModelRetryPolicy::new(2, 50).expect("retry policy"),
        },
    );
    let pending = decide(Some(&deterministic), &request).expect("request model");
    assert_eq!(
        decide_recovery(pending.state(), false, false, true),
        DebuggerRecoveryDecision::ClaimModelAttempt { attempt: 1 },
    );
    let start = next(
        Some(pending.state()),
        44,
        DebuggerCommandKind::MarkModelAttemptStarted { model_id, attempt: 1, started_at_tick: 100 },
    );
    let running = decide(Some(pending.state()), &start).expect("start claimed attempt");
    let failure = ModelAttemptFailure::new(
        model_id,
        1,
        ModelAttemptFailureCode::ProviderStream,
        true,
        digest(45),
        3,
        120,
    )
    .expect("retryable failure");
    let settle =
        next(Some(running.state()), 46, DebuggerCommandKind::RecordModelFailure { failure });
    let awaiting = decide(Some(running.state()), &settle).expect("settle failure");
    assert_eq!(awaiting.state().phase(), DebuggerPhase::ModelPending);
    assert_eq!(
        decide_recovery(awaiting.state(), false, false, false),
        DebuggerRecoveryDecision::ScheduleModelRetry { completed_attempt: 1 },
    );
    let schedule = next(
        Some(awaiting.state()),
        48,
        DebuggerCommandKind::ScheduleModelRetry { model_id, next_attempt: 2, not_before_tick: 125 },
    );
    let retry = decide(Some(awaiting.state()), &schedule).expect("schedule retry");
    assert_eq!(
        retry.state().model().expect("model state").state(),
        ModelWorkState::Pending { attempt: 2, not_before_tick: 125 },
    );
}

#[test]
fn cancellation_is_terminal_and_wins_before_effect_execution() {
    let deterministic = deterministic_state(None);
    let cancel = next(
        Some(&deterministic),
        60,
        DebuggerCommandKind::CancelJob { reason_digest: digest(61) },
    );
    let cancelled = decide(Some(&deterministic), &cancel).expect("cancel job");
    assert_eq!(cancelled.state().phase(), DebuggerPhase::Cancelled);
    assert_eq!(
        decide_recovery(cancelled.state(), false, false, false),
        DebuggerRecoveryDecision::Complete,
    );

    let second = next(
        Some(cancelled.state()),
        62,
        DebuggerCommandKind::CancelJob { reason_digest: digest(63) },
    );
    let error = decide(Some(cancelled.state()), &second).expect_err("terminal state is immutable");
    assert_eq!(error.kind(), DebuggerErrorKind::IllegalTransition);
}

#[test]
fn report_recovery_distinguishes_staging_commit_admission_and_settlement() {
    let deterministic = deterministic_state(None);
    assert_eq!(
        decide_recovery(&deterministic, true, false, false),
        DebuggerRecoveryDecision::PrepareReport,
        "an orphaned staged artifact is reused by deterministic report preparation",
    );

    let report =
        ReportRecord::new(ReportId::new(bytes(70)).expect("report identity"), digest(71), 128)
            .expect("report record");
    let complete = next(Some(&deterministic), 72, DebuggerCommandKind::CompleteReport { report });
    let report_ready = decide(Some(&deterministic), &complete).expect("commit staged report");
    assert_eq!(report_ready.state().phase(), DebuggerPhase::ReportReady);
    assert_eq!(
        decide_recovery(report_ready.state(), true, false, true),
        DebuggerRecoveryDecision::ClaimPublication,
    );
    assert_eq!(
        decide_recovery(report_ready.state(), true, true, true),
        DebuggerRecoveryDecision::ReconcilePublication,
    );

    let publication = PublicationRecord::new(
        report.id(),
        report.digest(),
        report.size(),
        EvidenceId::new(bytes(74)).expect("evidence identity"),
        9,
    )
    .expect("publication record");
    let publish = next(
        Some(report_ready.state()),
        75,
        DebuggerCommandKind::RecordPublication { publication },
    );
    let published = decide(Some(report_ready.state()), &publish).expect("record publication");
    assert_eq!(published.state().phase(), DebuggerPhase::Published);
    assert_eq!(
        decide_recovery(published.state(), true, true, false),
        DebuggerRecoveryDecision::Complete,
    );

    let wrong_publication = PublicationRecord::new(
        report.id(),
        digest(77),
        report.size(),
        EvidenceId::new(bytes(78)).expect("evidence identity"),
        9,
    )
    .expect("well-formed mismatched publication");
    let wrong = next(
        Some(report_ready.state()),
        79,
        DebuggerCommandKind::RecordPublication { publication: wrong_publication },
    );
    assert_eq!(
        decide(Some(report_ready.state()), &wrong)
            .expect_err("publication must bind the committed artifact")
            .kind(),
        DebuggerErrorKind::IdempotencyConflict,
    );
}
