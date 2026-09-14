//! Scheduler schema policy at the authoritative daemon domain ingress.

use peritus_app_protocol::AppErrorCode;
use peritus_codec::CodecLimits;
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_scheduler::{
    SCHEDULER_STATE_NAMESPACE, SchedulerBinding, SchedulerCommand, SchedulerCommandKind,
    SchedulerSession, decode_scheduler_command, encode_scheduler_command, scheduler_aggregate_key,
    scheduler_state_key,
};

use super::{DomainOutcome, DomainSubmission, dispatch};

#[test]
fn fresh_v1_genesis_is_rejected_without_state_while_equivalent_v2_genesis_commits() {
    let directory = tempfile::tempdir().expect("temporary scheduler ingress directory");
    let path = directory.path().join("scheduler-schema-ingress.sqlite3");
    let store_id = StoreId::new([0x71; 16]).expect("nonzero store identity");
    let mut journal = SqliteJournal::open(&path, store_id, SqliteJournalOptions::default())
        .expect("open journal");
    let mut session = SchedulerSession::default();
    let legacy_frame = legacy_genesis_bytes();
    let legacy = decode_scheduler_command(&legacy_frame, CodecLimits::PRODUCTION)
        .expect("fixed legacy scheduler genesis");
    let aggregate = scheduler_aggregate_key(legacy.run_id()).expect("scheduler aggregate key");
    let state_key = scheduler_state_key(legacy.run_id());

    let outcome = dispatch(&mut journal, &mut session, submission(&legacy, legacy_frame))
        .expect("legacy schema policy is a bounded rejection");
    assert!(matches!(outcome, DomainOutcome::Rejected(AppErrorCode::UnsupportedSchema)));
    assert!(journal.head(aggregate).expect("read rejected aggregate head").is_none());
    assert!(
        journal
            .state_record(SCHEDULER_STATE_NAMESPACE, &state_key)
            .expect("read rejected aggregate checkpoint")
            .is_none()
    );

    let current = current_genesis(&legacy);
    let current_frame = encode_scheduler_command(&current, CodecLimits::PRODUCTION)
        .expect("encode current scheduler genesis");
    let outcome = dispatch(&mut journal, &mut session, submission(&current, current_frame))
        .expect("current scheduler genesis dispatch");
    let DomainOutcome::Committed(batch) = outcome else {
        panic!("current scheduler genesis must commit");
    };
    assert_eq!(batch.records().len(), 1);
    assert!(journal.head(aggregate).expect("read committed aggregate head").is_some());
    assert!(
        journal
            .state_record(SCHEDULER_STATE_NAMESPACE, &state_key)
            .expect("read committed aggregate checkpoint")
            .is_some()
    );
}

fn legacy_genesis_bytes() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/protocol/scheduler-v1-recovery/benign/command-01.bin"),
    )
    .expect("read fixed legacy scheduler genesis")
}

fn current_genesis(legacy: &SchedulerCommand) -> SchedulerCommand {
    let SchedulerCommandKind::StartScheduler { binding: legacy_binding } = legacy.kind() else {
        panic!("fixed legacy command must be scheduler genesis");
    };
    let binding = SchedulerBinding::new(
        legacy_binding.run_id(),
        legacy_binding.scheduler_id(),
        legacy_binding.revision(),
        legacy_binding.limits(),
        legacy_binding.capacity().clone(),
    )
    .expect("equivalent strict scheduler binding");
    SchedulerCommand::new(
        legacy.command_id(),
        legacy.event_id(),
        binding.run_id(),
        legacy.expected_sequence(),
        legacy.expected_previous_event(),
        legacy.prior_state_digest(),
        binding.revision(),
        SchedulerCommandKind::StartScheduler { binding },
    )
    .expect("equivalent strict scheduler genesis")
}

fn submission(command: &SchedulerCommand, frame: Vec<u8>) -> DomainSubmission {
    DomainSubmission::new(
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
        command.revision(),
        70,
        frame,
    )
}
