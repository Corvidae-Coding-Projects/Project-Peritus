//! Durable scheduler schema ownership over imported historical C0 records.

#[path = "versioned_durability/historical_import.rs"]
mod historical_import;

use peritus_codec::{CodecLimits, decode_frame, sha256};
use peritus_journal::{CommandResolution, SqliteJournal, SqliteJournalOptions};
use peritus_scheduler::{
    SCHEDULER_STATE_NAMESPACE, SchedulerCommand, SchedulerCommandKind, SchedulerErrorKind,
    SchedulerRecoveryAction, SchedulerSemantics, commit_scheduler_transition, decide,
    decode_scheduler_command, decode_scheduler_event, decode_scheduler_state,
    encode_scheduler_event, encode_scheduler_state, load_scheduler_replay, replay,
    scheduler_aggregate_key, scheduler_state_key, start,
};

use historical_import::{
    History, append_raw, command_id, event_id, import_historical, journal_path, open,
    relabel_schema, store_id,
};

const LIMITS: CodecLimits = CodecLimits::PRODUCTION;
const HISTORY_FILES: [&str; 7] = ["01", "03", "04", "05", "06", "07", "08"];
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn fresh_legacy_genesis_is_purely_replayable_but_not_durably_creatable() -> TestResult {
    let history = History::load("benign")?;
    let command = decode_scheduler_command(&history.commands[0], LIMITS)?;
    let transition = start(&command)?;
    assert_eq!(command.semantics(), SchedulerSemantics::LegacyQueueV1);
    assert_eq!(transition.event().semantics(), SchedulerSemantics::LegacyQueueV1);
    assert_eq!(encode_scheduler_event(transition.event(), LIMITS)?, history.events[0]);

    let directory = tempfile::tempdir()?;
    let mut journal = open(&directory)?;
    let aggregate = scheduler_aggregate_key(command.run_id())?;
    let state_key = scheduler_state_key(command.run_id());
    let error = commit_scheduler_transition(&mut journal, &command, &transition)
        .err()
        .ok_or_else(|| std::io::Error::other("fresh schema-1 genesis unexpectedly committed"))?;
    assert_eq!(error.kind(), SchedulerErrorKind::BindingMismatch);
    assert_eq!(error.recovery(), SchedulerRecoveryAction::CorrectInput);
    assert!(journal.head(aggregate)?.is_none());
    assert!(journal.state_record(SCHEDULER_STATE_NAMESPACE, &state_key)?.is_none());
    assert!(matches!(
        journal.resolve_command(command.command_id(), sha256(&history.commands[0]))?,
        CommandResolution::DefinitelyAbsent
    ));

    Ok(())
}

#[test]
fn authentic_legacy_histories_import_reopen_and_rebuild_exactly() -> TestResult {
    for scenario in ["benign", "retry", "loss"] {
        let history = History::load(scenario)?;
        let directory = tempfile::tempdir()?;
        let path = journal_path(&directory);
        let mut journal = open(&directory)?;
        let mut events = Vec::with_capacity(HISTORY_FILES.len());
        let mut final_state = import_historical(&mut journal, &history, 0, &mut events)?.state;
        for index in 1..HISTORY_FILES.len() {
            final_state = import_historical(&mut journal, &history, index, &mut events)?.state;
        }
        assert_eq!(encode_scheduler_state(&final_state, LIMITS)?, history.checkpoint);

        let aggregate = scheduler_aggregate_key(final_state.run_id())?;
        let records = journal.records_for_aggregate(aggregate)?;
        assert_eq!(records.len(), history.events.len());
        for (record, expected) in records.iter().zip(&history.events) {
            assert_eq!(record.frame_bytes(), expected);
        }
        drop(journal);

        let restarted = SqliteJournal::open(&path, store_id(), SqliteJournalOptions::default())?;
        let rebuilt = load_scheduler_replay(&restarted, final_state.run_id())?
            .rebuild()?
            .ok_or_else(|| std::io::Error::other("imported history rebuilt as absent"))?;
        assert_eq!(rebuilt, final_state);
        let checkpoint = restarted
            .state_record(SCHEDULER_STATE_NAMESPACE, &scheduler_state_key(final_state.run_id()))?
            .ok_or_else(|| std::io::Error::other("imported checkpoint is absent"))?;
        assert_eq!(checkpoint.bytes(), history.checkpoint);
    }

    Ok(())
}

#[test]
fn legacy_exact_retry_resolution_and_continuation_remain_version_sticky() -> TestResult {
    let history = History::load("benign")?;
    let directory = tempfile::tempdir()?;
    let path = journal_path(&directory);
    let mut journal = open(&directory)?;
    let mut events = Vec::with_capacity(HISTORY_FILES.len());

    let genesis = import_historical(&mut journal, &history, 0, &mut events)?;
    let regenerated = start(&genesis.command)?;
    assert_eq!(regenerated.event(), &genesis.event);
    assert_eq!(regenerated.state(), &genesis.state);
    let resolved = commit_scheduler_transition(&mut journal, &genesis.command, &regenerated)?;
    assert_eq!(resolved.batch_hash(), genesis.batch.batch_hash());
    assert_eq!(resolved.request_digest(), genesis.batch.request_digest());
    assert_eq!(
        journal.records_for_aggregate(scheduler_aggregate_key(genesis.state.run_id())?)?.len(),
        1
    );

    let strict_command =
        decode_scheduler_command(&relabel_schema(&history.commands[0], 2), LIMITS)?;
    let strict_transition = start(&strict_command)?;
    let conflict = commit_scheduler_transition(&mut journal, &strict_command, &strict_transition)
        .err()
        .ok_or_else(|| {
            std::io::Error::other("cross-schema command identity unexpectedly aliased")
        })?;
    // A same-identity, different-frame digest reaches the established C0 command-conflict path.
    assert_eq!(conflict.kind(), SchedulerErrorKind::Journal);
    assert_eq!(conflict.recovery(), SchedulerRecoveryAction::Quarantine);

    let mut final_state = genesis.state;
    for index in 1..HISTORY_FILES.len() {
        final_state = import_historical(&mut journal, &history, index, &mut events)?.state;
    }
    assert_eq!(encode_scheduler_state(&final_state, LIMITS)?, history.checkpoint);
    drop(journal);

    let mut restarted = SqliteJournal::open(&path, store_id(), SqliteJournalOptions::default())?;
    let rebuilt = load_scheduler_replay(&restarted, final_state.run_id())?
        .rebuild()?
        .ok_or_else(|| std::io::Error::other("advanced legacy history rebuilt as absent"))?;
    assert_eq!(rebuilt, final_state);

    let advanced_retry =
        commit_scheduler_transition(&mut restarted, &genesis.command, &regenerated)
            .err()
            .ok_or_else(|| std::io::Error::other("advanced genesis retry unexpectedly resolved"))?;
    assert_eq!(advanced_retry.kind(), SchedulerErrorKind::Journal);
    assert_eq!(advanced_retry.recovery(), SchedulerRecoveryAction::ReplayAggregate);

    let continuation = SchedulerCommand::from_state(
        &rebuilt,
        command_id(90),
        event_id(91),
        SchedulerCommandKind::PauseScheduler,
    )?;
    assert_eq!(continuation.semantics(), SchedulerSemantics::LegacyQueueV1);
    let successor = decide(&rebuilt, &continuation)?;
    let committed = commit_scheduler_transition(&mut restarted, &continuation, &successor)?;
    assert_eq!(committed.records().len(), 1);
    assert_eq!(
        decode_frame(committed.records()[0].frame_bytes(), LIMITS)?.header().schema_version(),
        1
    );
    let current = restarted
        .state_record(SCHEDULER_STATE_NAMESPACE, &scheduler_state_key(rebuilt.run_id()))?
        .ok_or_else(|| std::io::Error::other("legacy continuation checkpoint is absent"))?;
    assert_eq!(decode_frame(current.bytes(), LIMITS)?.header().schema_version(), 1);
    assert_eq!(
        load_scheduler_replay(&restarted, rebuilt.run_id())?.rebuild()?,
        Some(successor.state().clone())
    );

    Ok(())
}

#[test]
fn checkpoint_schema_guard_rejects_both_mixed_version_directions() -> TestResult {
    let history = History::load("benign")?;
    let legacy_command = decode_scheduler_command(&history.commands[0], LIMITS)?;
    let legacy_genesis = start(&legacy_command)?;
    let strict_command =
        decode_scheduler_command(&relabel_schema(&history.commands[0], 2), LIMITS)?;
    let strict_genesis = start(&strict_command)?;

    let legacy_directory = tempfile::tempdir()?;
    let mut legacy_journal = open(&legacy_directory)?;
    let mut events = Vec::new();
    let imported = import_historical(&mut legacy_journal, &history, 0, &mut events)?;
    let strict_continuation = SchedulerCommand::from_state(
        strict_genesis.state(),
        command_id(80),
        event_id(81),
        SchedulerCommandKind::PauseScheduler,
    )?;
    let strict_successor = decide(strict_genesis.state(), &strict_continuation)?;
    let legacy_head = legacy_journal.head(scheduler_aggregate_key(imported.state.run_id())?)?;
    let legacy_checkpoint = legacy_journal
        .state_record(SCHEDULER_STATE_NAMESPACE, &scheduler_state_key(imported.state.run_id()))?
        .ok_or_else(|| std::io::Error::other("legacy genesis checkpoint is absent"))?;
    let error =
        commit_scheduler_transition(&mut legacy_journal, &strict_continuation, &strict_successor)
            .err()
            .ok_or_else(|| {
                std::io::Error::other("schema-2 continuation crossed schema-1 checkpoint")
            })?;
    assert_eq!(error.kind(), SchedulerErrorKind::BindingMismatch);
    assert_eq!(error.recovery(), SchedulerRecoveryAction::CorrectInput);
    assert_eq!(
        legacy_journal.head(scheduler_aggregate_key(imported.state.run_id())?)?,
        legacy_head
    );
    assert_eq!(
        legacy_journal
            .state_record(
                SCHEDULER_STATE_NAMESPACE,
                &scheduler_state_key(imported.state.run_id()),
            )?
            .ok_or_else(|| std::io::Error::other("legacy checkpoint vanished"))?
            .bytes(),
        legacy_checkpoint.bytes()
    );

    let strict_directory = tempfile::tempdir()?;
    let mut strict_journal = open(&strict_directory)?;
    commit_scheduler_transition(&mut strict_journal, &strict_command, &strict_genesis)?;
    let legacy_continuation = SchedulerCommand::from_state(
        legacy_genesis.state(),
        command_id(82),
        event_id(83),
        SchedulerCommandKind::PauseScheduler,
    )?;
    let legacy_successor = decide(legacy_genesis.state(), &legacy_continuation)?;
    let error =
        commit_scheduler_transition(&mut strict_journal, &legacy_continuation, &legacy_successor)
            .err()
            .ok_or_else(|| {
                std::io::Error::other("schema-1 continuation crossed schema-2 checkpoint")
            })?;
    assert_eq!(error.kind(), SchedulerErrorKind::BindingMismatch);
    assert_eq!(error.recovery(), SchedulerRecoveryAction::CorrectInput);
    assert_eq!(
        strict_journal
            .head(scheduler_aggregate_key(strict_genesis.state().run_id())?)?
            .ok_or_else(|| std::io::Error::other("strict head vanished"))?
            .sequence()
            .get(),
        1
    );

    Ok(())
}

#[test]
fn durable_load_rejects_a_mixed_event_schema_chain_before_rebuild() -> TestResult {
    let history = History::load("benign")?;
    let directory = tempfile::tempdir()?;
    let mut journal = open(&directory)?;
    let mut legacy_events = Vec::new();
    let genesis = import_historical(&mut journal, &history, 0, &mut legacy_events)?;

    let legacy_event = decode_scheduler_event(&history.events[1], LIMITS)?;
    legacy_events.push(legacy_event);
    let legacy_checkpoint = replay(&legacy_events)?;
    let legacy_checkpoint_bytes = encode_scheduler_state(&legacy_checkpoint, LIMITS)?;
    let mixed_event_bytes = relabel_schema(&history.events[1], 2);
    let mixed_event = decode_scheduler_event(&mixed_event_bytes, LIMITS)?;
    assert_eq!(mixed_event.semantics(), SchedulerSemantics::StrictRecoveryQueueV2);
    assert_eq!(mixed_event.successor_state_digest(), legacy_checkpoint.state_digest());

    append_raw(
        &mut journal,
        &history.commands[1],
        mixed_event_bytes.clone(),
        legacy_checkpoint_bytes,
    )?;
    let records =
        journal.records_for_aggregate(scheduler_aggregate_key(genesis.state.run_id())?)?;
    assert_eq!(records[0].frame_bytes(), history.events[0]);
    assert_eq!(records[1].frame_bytes(), mixed_event_bytes);

    let error = load_scheduler_replay(&journal, genesis.state.run_id())
        .err()
        .ok_or_else(|| std::io::Error::other("mixed event schemas unexpectedly loaded"))?;
    assert_eq!(error.kind(), SchedulerErrorKind::Journal);
    assert_eq!(error.recovery(), SchedulerRecoveryAction::Quarantine);

    Ok(())
}

#[test]
fn durable_load_rejects_checkpoint_event_schema_mismatch_before_rebuild() -> TestResult {
    let history = History::load("benign")?;
    let directory = tempfile::tempdir()?;
    let mut journal = open(&directory)?;
    let mut legacy_events = Vec::new();
    let genesis = import_historical(&mut journal, &history, 0, &mut legacy_events)?;

    let strict_genesis_command =
        decode_scheduler_command(&relabel_schema(&history.commands[0], 2), LIMITS)?;
    let strict_genesis = start(&strict_genesis_command)?;
    let legacy_successor_command = decode_scheduler_command(&history.commands[1], LIMITS)?;
    let strict_successor_command = SchedulerCommand::from_state(
        strict_genesis.state(),
        legacy_successor_command.command_id(),
        legacy_successor_command.event_id(),
        legacy_successor_command.kind().clone(),
    )?;
    let strict_successor = decide(strict_genesis.state(), &strict_successor_command)?;
    let strict_checkpoint_bytes = encode_scheduler_state(strict_successor.state(), LIMITS)?;
    assert_eq!(
        decode_scheduler_state(&strict_checkpoint_bytes, LIMITS)?.binding().semantics(),
        SchedulerSemantics::StrictRecoveryQueueV2
    );

    append_raw(
        &mut journal,
        &history.commands[1],
        history.events[1].clone(),
        strict_checkpoint_bytes.clone(),
    )?;
    let stored = journal
        .state_record(SCHEDULER_STATE_NAMESPACE, &scheduler_state_key(genesis.state.run_id()))?
        .ok_or_else(|| std::io::Error::other("mixed checkpoint is absent"))?;
    assert_eq!(stored.bytes(), strict_checkpoint_bytes);

    let error = load_scheduler_replay(&journal, genesis.state.run_id()).err().ok_or_else(|| {
        std::io::Error::other("checkpoint/event schema mismatch unexpectedly loaded")
    })?;
    assert_eq!(error.kind(), SchedulerErrorKind::Journal);
    assert_eq!(error.recovery(), SchedulerRecoveryAction::Quarantine);

    Ok(())
}
