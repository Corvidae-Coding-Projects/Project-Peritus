//! Focused real-C0 durability, idempotency, and restart-recovery tests.

mod support;

use std::time::Duration;

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_trace::{
    DiagnosticCode, JournalTraceStore, ObservationKind, SpanKind, recover_all, recover_trace,
};
use peritus_types::CommandId;
use tempfile::TempDir;

use support::{binding, event, observation, span, trace};

#[test]
fn c0_record_replay_and_restart_rebuild_are_exact() {
    let temporary = TempDir::new().expect("temporary directory");
    let path = temporary.path().join("trace.sqlite3");
    let store_id = StoreId::new([71; 16]).expect("store id");
    let trace_id = trace(72);
    let span_id = span(73);
    let binding = binding(7);
    let start = observation(
        74,
        trace_id,
        span_id,
        1,
        None,
        Vec::new(),
        binding,
        1,
        ObservationKind::SpanStarted(SpanKind::Recovery),
    );
    let diagnostic = observation(
        75,
        trace_id,
        span_id,
        2,
        None,
        vec![event(74)],
        binding,
        2,
        ObservationKind::Diagnostic(DiagnosticCode::RecoveryCompleted),
    );

    let mut journal = open(&path, store_id);
    {
        let mut store = JournalTraceStore::new(&mut journal);
        let first = store
            .record(CommandId::new([76; 16]).expect("command"), start.clone())
            .expect("record start");
        let replay = store
            .record(CommandId::new([76; 16]).expect("command"), start)
            .expect("resolve exact command");
        assert!(replay.exact_replay());
        assert_eq!(replay.global_position(), first.global_position());
        store
            .record(CommandId::new([77; 16]).expect("command"), diagnostic)
            .expect("record diagnostic");
    }

    let recovered = recover_trace(&journal, trace_id).expect("recover aggregate");
    assert_eq!(recovered.trace().expect("trace").observations().len(), 2);
    assert!(recovered.head().is_some());
    let all = recover_all(&mut journal).expect("rebuild all trace projections");
    assert_eq!(all.observation_count(), 2);
    drop(journal);

    let reopened = open(&path, store_id);
    let restarted = recover_trace(&reopened, trace_id).expect("restart recovery");
    assert_eq!(restarted.trace(), recovered.trace());
    assert_eq!(restarted.head(), recovered.head());
}

#[test]
fn changed_command_reuse_is_explicit_and_does_not_append() {
    let temporary = TempDir::new().expect("temporary directory");
    let mut journal =
        open(&temporary.path().join("conflict.sqlite3"), StoreId::new([81; 16]).expect("store id"));
    let trace_id = trace(82);
    let binding = binding(8);
    let first = observation(
        83,
        trace_id,
        span(84),
        1,
        None,
        Vec::new(),
        binding,
        1,
        ObservationKind::SpanStarted(SpanKind::Internal),
    );
    let changed = observation(
        85,
        trace_id,
        span(86),
        1,
        None,
        Vec::new(),
        binding,
        1,
        ObservationKind::SpanStarted(SpanKind::Internal),
    );
    let command = CommandId::new([87; 16]).expect("command");
    let error = {
        let mut store = JournalTraceStore::new(&mut journal);
        store.record(command, first).expect("initial command");
        store.record(command, changed).expect_err("changed command reuse")
    };
    assert_eq!(error.kind(), peritus_trace::TraceErrorKind::DuplicateConflict);
    assert_eq!(
        recover_trace(&journal, trace_id)
            .expect("recover")
            .trace()
            .expect("trace")
            .observations()
            .len(),
        1,
    );
}

fn open(path: &std::path::Path, store_id: StoreId) -> SqliteJournal {
    SqliteJournal::open(
        path,
        store_id,
        SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
    )
    .expect("open C0 journal")
}
