//! Journal, outbox, artifact, evidence, and restart integration coverage.

use std::time::Duration;

use peritus_debugger::{
    AnalysisCounts, DebuggerCommand, DebuggerCommandKind, DebuggerErrorKind, DebuggerJobId,
    DebuggerPhase, ModelAnalysisId, ModelAttemptFailure, ModelAttemptFailureCode, ModelBudget,
    ModelDirectiveClaim, ModelRetryPolicy, SelectionManifestId, SelectionRecord,
    commit_debugger_claimed_transition, commit_debugger_settlement, commit_debugger_transition,
    debugger_aggregate_key, load_debugger_replay,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
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

fn open() -> (tempfile::TempDir, SqliteJournal) {
    let temporary = tempfile::tempdir().expect("temporary journal directory");
    let journal = SqliteJournal::open(
        temporary.path().join("debugger.sqlite3"),
        StoreId::new(bytes(6)).expect("store identity"),
        SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
    )
    .expect("open debugger journal");
    (temporary, journal)
}

fn genesis(
    command_seed: u8,
    event_seed: u8,
    model_plan_digest: Option<Sha256Digest>,
) -> DebuggerCommand {
    DebuggerCommand::new(
        CommandId::new(bytes(command_seed)).expect("command identity"),
        EventId::new(bytes(event_seed)).expect("event identity"),
        DebuggerJobId::new(bytes(12)).expect("job identity"),
        0,
        None,
        digest(0),
        digest(13),
        DebuggerCommandKind::CreateJob {
            revision: revision(),
            query_digest: digest(13),
            limits_digest: digest(14),
            model_plan_digest,
        },
    )
    .expect("valid genesis")
}

fn next(
    state: &peritus_debugger::DebuggerState,
    command_seed: u8,
    event_seed: u8,
    kind: DebuggerCommandKind,
) -> DebuggerCommand {
    DebuggerCommand::new(
        CommandId::new(bytes(command_seed)).expect("command identity"),
        EventId::new(bytes(event_seed)).expect("event identity"),
        state.job_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.query_digest(),
        kind,
    )
    .expect("valid fenced command")
}

#[test]
fn commit_is_idempotent_and_replay_matches_the_atomic_checkpoint() {
    let (_temporary, mut journal) = open();
    let command = genesis(10, 11, None);
    let transition = peritus_debugger::decide(None, &command).expect("create transition");
    let first =
        commit_debugger_transition(&mut journal, &command, &transition).expect("commit transition");
    assert_eq!(first.first_position(), 1);
    assert_eq!(first.last_position(), 1);
    assert_eq!(first.records().len(), 1);

    let retry = commit_debugger_transition(&mut journal, &command, &transition)
        .expect("exact retry resolves original commit");
    assert_eq!(retry.command_id(), first.command_id());
    assert_eq!(retry.request_digest(), first.request_digest());
    assert_eq!(retry.batch_hash(), first.batch_hash());
    assert_eq!(retry.first_position(), first.first_position());

    let replay = load_debugger_replay(&journal, command.job_id()).expect("load debugger replay");
    let recovered = replay.rebuild().expect("rebuild exact checkpoint").expect("committed state");
    assert_eq!(recovered, transition.state().clone());
    assert_eq!(recovered.phase(), DebuggerPhase::Created);
    assert_eq!(replay.events(), &[transition.event().clone()]);
    assert_eq!(
        debugger_aggregate_key(command.job_id()).expect("aggregate").kind(),
        peritus_journal::AggregateKind::Debugger
    );
}

#[test]
fn reused_command_identity_with_different_content_is_rejected() {
    let (_temporary, mut journal) = open();
    let command = genesis(20, 21, None);
    let transition = peritus_debugger::decide(None, &command).expect("create transition");
    commit_debugger_transition(&mut journal, &command, &transition).expect("commit transition");

    let conflicting = genesis(20, 22, None);
    let conflicting_transition =
        peritus_debugger::decide(None, &conflicting).expect("locally valid conflict");
    let error = commit_debugger_transition(&mut journal, &conflicting, &conflicting_transition)
        .expect_err("command identity cannot be rebound");
    assert_eq!(error.kind(), DebuggerErrorKind::IdempotencyConflict);
}

#[test]
fn model_outbox_start_and_failure_settlement_share_one_claim_fence() {
    let (_temporary, mut journal) = open();
    let plan_digest = digest(40);
    let create = genesis(31, 32, Some(plan_digest));
    let created = peritus_debugger::decide(None, &create).expect("create transition");
    commit_debugger_transition(&mut journal, &create, &created).expect("commit create");

    let selection = SelectionRecord::new(
        SelectionManifestId::new(bytes(41)).expect("selection identity"),
        digest(42),
        1,
        2,
    )
    .expect("selection record");
    let select = next(created.state(), 33, 34, DebuggerCommandKind::RecordSelection { selection });
    let selected = peritus_debugger::decide(Some(created.state()), &select).expect("selection");
    commit_debugger_transition(&mut journal, &select, &selected).expect("commit selection");
    let analyze = next(
        selected.state(),
        35,
        36,
        DebuggerCommandKind::RecordDeterministicAnalysis {
            analysis_digest: digest(43),
            counts: AnalysisCounts::new(2, 1, 1),
        },
    );
    let analyzed = peritus_debugger::decide(Some(selected.state()), &analyze).expect("analysis");
    commit_debugger_transition(&mut journal, &analyze, &analyzed).expect("commit analysis");

    let model_id = ModelAnalysisId::new(bytes(44)).expect("model identity");
    let request = next(
        analyzed.state(),
        37,
        38,
        DebuggerCommandKind::RequestModelAnalysis {
            model_id,
            plan_digest,
            request_digest: digest(45),
            budget: ModelBudget::new(8, 2048, 500, 500, 750).expect("budget"),
            retry_policy: ModelRetryPolicy::new(2, 10).expect("retry policy"),
        },
    );
    let pending = peritus_debugger::decide(Some(analyzed.state()), &request).expect("request");
    commit_debugger_transition(&mut journal, &request, &pending).expect("commit request");
    let message = journal.claim_outbox(1, 20).expect("claim query").expect("model directive");
    let claim = ModelDirectiveClaim::from_message(&message).expect("checked model claim");
    assert_eq!(claim.directive().model_id(), model_id);
    assert_eq!(claim.directive().attempt(), 1);

    let start = next(
        pending.state(),
        39,
        40,
        DebuggerCommandKind::MarkModelAttemptStarted { model_id, attempt: 1, started_at_tick: 2 },
    );
    let running = peritus_debugger::decide(Some(pending.state()), &start).expect("start attempt");
    commit_debugger_claimed_transition(&mut journal, &start, &running, claim)
        .expect("commit intent before provider I/O");
    let failure = ModelAttemptFailure::new(
        model_id,
        1,
        ModelAttemptFailureCode::ProviderStream,
        true,
        digest(46),
        2,
        50,
    )
    .expect("failure observation");
    let settle = next(running.state(), 41, 42, DebuggerCommandKind::RecordModelFailure { failure });
    let settled = peritus_debugger::decide(Some(running.state()), &settle).expect("settlement");
    commit_debugger_settlement(&mut journal, &settle, &settled, claim)
        .expect("atomic failure and outbox acknowledgement");
    assert!(
        journal.claim_outbox(21, 30).expect("outbox query").is_none(),
        "acknowledged attempt cannot be delivered again",
    );
}

#[test]
fn absent_job_has_no_phantom_checkpoint_or_history() {
    let (_temporary, journal) = open();
    let job_id = DebuggerJobId::new(bytes(30)).expect("job identity");
    let replay = load_debugger_replay(&journal, job_id).expect("load absent aggregate");
    assert!(replay.events().is_empty());
    assert!(replay.rebuild().expect("consistent absence").is_none());
}
