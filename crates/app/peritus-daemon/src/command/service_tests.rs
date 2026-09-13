//! Application-ledger retry coverage for committed historical scheduler commands.

use std::path::Path;

use peritus_app_protocol::{
    AppErrorCode, AppProtocolLimits, CommandBinding, CommandDisposition, CommandSubmissionFrames,
    CorrelationId, IdempotencyKey, RequestId,
};
use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_codec::{CodecLimits, encode_message, sha256};
use peritus_evidence::revision_digest;
use peritus_journal::{
    AppendRequest, ApplicationCommandAdmission, ApplicationCommandSettlement,
    ApplicationCommandState, ApplicationPrincipalKind, ApplicationRequestId, EventDraft,
    ExactFrame, HeadExpectation, NewApplicationCommand, NewApplicationPrincipal,
    NewApplicationSession, SqliteJournal, SqliteJournalOptions, StateInstall, StoreId,
};
use peritus_kernel::CommandEnvelope;
use peritus_protocol::CommandEnvelopeDto;
use peritus_scheduler::{
    SCHEDULER_STATE_NAMESPACE, SchedulerBinding, SchedulerCommand, SchedulerCommandKind,
    decode_scheduler_command, decode_scheduler_event, decode_scheduler_state,
    encode_scheduler_command, scheduler_aggregate_key, scheduler_state_key, start,
};
use peritus_types::{ActorId, SessionId, Sha256Digest};

use super::{committed_result_digest, submit};
use crate::{AuthorityHandle, AuthorityOwner, DaemonLifecycle, StartupPhase};

#[derive(Clone, Copy)]
enum SeedSettlement {
    Pending,
    Indeterminate,
    Committed,
}

#[tokio::test]
async fn settled_v1_genesis_retry_returns_its_original_committed_result() {
    let directory = tempfile::tempdir().expect("temporary application retry directory");
    let (journal, binding, expected_result_digest) =
        seeded_journal(directory.path(), SeedSettlement::Committed);
    let (authority, task) = spawn_authority(directory.path(), journal);

    let result = submit(&authority, actor(), &binding).await.expect("replay settled command");
    assert_eq!(result.disposition(), CommandDisposition::Replayed);
    assert_eq!(result.original_request_id(), binding.request_id());
    let range = result.committed_events().expect("retained committed range");
    assert_eq!((range.first().get(), range.last().get()), (1, 1));
    assert_eq!(authority.global_events_after(0, 2).await.expect("read C0 tail").records().len(), 1);

    stop_authority(authority, task).await;
    let journal = open_journal(directory.path());
    let record = journal
        .application_command(legacy_command().command_id())
        .expect("read settled application command")
        .expect("settled application command exists");
    assert_eq!(record.state(), ApplicationCommandState::Committed);
    assert_eq!(record.domain_command_digest(), sha256(&legacy_command_bytes()));
    assert_eq!(record.result_digest(), Some(expected_result_digest));
}

#[tokio::test]
async fn unsettled_v1_genesis_retries_reconcile_the_exact_c0_digest() {
    for settlement in [SeedSettlement::Pending, SeedSettlement::Indeterminate] {
        let directory = tempfile::tempdir().expect("temporary reconciliation directory");
        let (journal, binding, expected_result_digest) =
            seeded_journal(directory.path(), settlement);
        let (authority, task) = spawn_authority(directory.path(), journal);

        let result =
            submit(&authority, actor(), &binding).await.expect("reconcile historical retry");
        assert_eq!(result.disposition(), CommandDisposition::Replayed);
        assert_eq!(result.original_request_id(), binding.request_id());
        let range = result.committed_events().expect("reconciled committed range");
        assert_eq!((range.first().get(), range.last().get()), (1, 1));
        assert_eq!(
            authority
                .global_events_after(0, 2)
                .await
                .expect("read reconciled C0 tail")
                .records()
                .len(),
            1
        );

        stop_authority(authority, task).await;
        let journal = open_journal(directory.path());
        let record = journal
            .application_command(legacy_command().command_id())
            .expect("read reconciled application command")
            .expect("reconciled application command exists");
        assert_eq!(record.state(), ApplicationCommandState::Committed);
        assert_eq!(record.domain_command_digest(), sha256(&legacy_command_bytes()));
        assert_eq!(record.result_digest(), Some(expected_result_digest));
    }
}

#[tokio::test]
async fn same_id_schema_v2_relabel_conflicts_without_aliasing_the_v1_result() {
    let directory = tempfile::tempdir().expect("temporary cross-schema retry directory");
    let (journal, legacy_binding, _) = seeded_journal(directory.path(), SeedSettlement::Committed);
    let current_binding = current_binding();
    assert_eq!(
        legacy_binding.frames().envelope().as_domain().command_id(),
        current_binding.frames().envelope().as_domain().command_id()
    );
    assert_ne!(
        legacy_binding.frames().command_frame().digest(),
        current_binding.frames().command_frame().digest()
    );
    assert_ne!(legacy_binding.request_digest(), current_binding.request_digest());
    let (authority, task) = spawn_authority(directory.path(), journal);

    let conflict =
        submit(&authority, actor(), &current_binding).await.expect("bounded application conflict");
    assert_eq!(conflict.disposition(), CommandDisposition::Rejected);
    assert_eq!(
        conflict.error().map(peritus_app_protocol::AppProtocolError::code),
        Some(AppErrorCode::IdempotencyConflict)
    );
    assert_eq!(authority.global_events_after(0, 2).await.expect("read C0 tail").records().len(), 1);

    let exact = submit(&authority, actor(), &legacy_binding)
        .await
        .expect("original schema remains replayable");
    assert_eq!(exact.disposition(), CommandDisposition::Replayed);
    assert_eq!(exact.committed_events().expect("original range").count(), 1);
    stop_authority(authority, task).await;
}

fn seeded_journal(
    root: &Path,
    settlement: SeedSettlement,
) -> (SqliteJournal, CommandBinding, Sha256Digest) {
    let mut journal = open_journal(root);
    seed_application_session(&mut journal);
    let batch = append_legacy_genesis(&mut journal);
    let binding = legacy_binding();
    let command = new_application_command(&binding);
    let ApplicationCommandAdmission::Inserted(record) =
        journal.admit_application_command(command).expect("admit historical application command")
    else {
        panic!("historical application command must be newly admitted");
    };
    let result_digest = committed_result_digest(&batch);
    match settlement {
        SeedSettlement::Pending => {}
        SeedSettlement::Indeterminate => {
            journal
                .settle_application_command(
                    record.command_id(),
                    record.request_digest(),
                    ApplicationCommandSettlement::indeterminate(),
                )
                .expect("mark historical application command indeterminate");
        }
        SeedSettlement::Committed => {
            journal
                .settle_application_command(
                    record.command_id(),
                    record.request_digest(),
                    ApplicationCommandSettlement::committed(&batch, result_digest),
                )
                .expect("settle historical application command");
        }
    }
    (journal, binding, result_digest)
}

fn append_legacy_genesis(journal: &mut SqliteJournal) -> peritus_journal::CommittedBatch {
    let command = legacy_command();
    let event_bytes = legacy_event_bytes();
    let state_bytes = legacy_state_bytes();
    let command_bytes = legacy_command_bytes();
    let event = decode_scheduler_event(&event_bytes, CodecLimits::PRODUCTION)
        .expect("fixed historical scheduler event");
    let state = decode_scheduler_state(&state_bytes, CodecLimits::PRODUCTION)
        .expect("fixed historical scheduler state");
    let transition = start(&command).expect("historical scheduler genesis reduces");
    assert_eq!(transition.event(), &event);
    assert_eq!(transition.state(), &state);
    let aggregate = scheduler_aggregate_key(command.run_id()).expect("scheduler aggregate key");
    let event = EventDraft::new(
        aggregate,
        event.sequence(),
        event.id(),
        event.previous_event(),
        ExactFrame::new(event_bytes).expect("exact historical event frame"),
        revision_digest(&event.revision()),
        Vec::new(),
    )
    .expect("historical event draft");
    let state = StateInstall::new(
        SCHEDULER_STATE_NAMESPACE,
        scheduler_state_key(command.run_id()),
        None,
        state.sequence().get(),
        state_bytes,
    )
    .expect("historical state install");
    let plan = AppendRequest::new(
        journal.store_id(),
        command.command_id(),
        sha256(&command_bytes),
        vec![HeadExpectation::Absent(aggregate)],
        vec![event],
        vec![state],
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("historical C0 append plan");
    journal.append(plan).expect("append historical scheduler genesis")
}

fn legacy_binding() -> CommandBinding {
    let command = legacy_command();
    binding(&command, legacy_command_bytes())
}

fn current_binding() -> CommandBinding {
    let legacy = legacy_command();
    let SchedulerCommandKind::StartScheduler { binding: historical } = legacy.kind() else {
        panic!("fixed historical command must be scheduler genesis");
    };
    let current = SchedulerBinding::new(
        historical.run_id(),
        historical.scheduler_id(),
        historical.revision(),
        historical.limits(),
        historical.capacity().clone(),
    )
    .and_then(|scheduler| {
        SchedulerCommand::new(
            legacy.command_id(),
            legacy.event_id(),
            legacy.run_id(),
            legacy.expected_sequence(),
            legacy.expected_previous_event(),
            legacy.prior_state_digest(),
            legacy.revision(),
            SchedulerCommandKind::StartScheduler { binding: scheduler },
        )
    })
    .expect("equivalent current scheduler genesis");
    let bytes = encode_scheduler_command(&current, CodecLimits::PRODUCTION)
        .expect("encode current scheduler command");
    binding(&current, bytes)
}

fn binding(command: &SchedulerCommand, command_bytes: Vec<u8>) -> CommandBinding {
    let envelope = CommandEnvelope::new(
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
        command.revision(),
    );
    let envelope_bytes =
        encode_message(&CommandEnvelopeDto::from(envelope), CodecLimits::PRODUCTION)
            .expect("encode command envelope");
    let frames = CommandSubmissionFrames::parse(
        envelope_bytes,
        command_bytes,
        AppProtocolLimits::PRODUCTION,
    )
    .expect("parse exact scheduler submission frames");
    CommandBinding::new(
        actor(),
        session(),
        RequestId::new([0xa3; 16]).expect("request identity"),
        CorrelationId::new([0xa4; 16]).expect("correlation identity"),
        IdempotencyKey::new(b"historical-scheduler-genesis".to_vec()).expect("idempotency key"),
        Some(command.revision()),
        frames,
    )
    .expect("bind scheduler application command")
}

fn new_application_command(binding: &CommandBinding) -> NewApplicationCommand {
    NewApplicationCommand::new(
        binding.actor_id(),
        binding.session_id(),
        binding.idempotency_key().as_bytes().to_vec(),
        binding.request_digest().as_sha256(),
        binding.frames().command_frame().digest(),
        ApplicationRequestId::new(binding.request_id().into_bytes()).expect("request identity"),
        binding.frames().envelope().as_domain().command_id(),
    )
    .expect("application command facts")
}

fn seed_application_session(journal: &mut SqliteJournal) {
    journal
        .bind_application_principal(NewApplicationPrincipal::new(
            Sha256Digest::new([0xa5; 32]),
            ApplicationPrincipalKind::UnixPeer,
            actor(),
            Sha256Digest::new([0xa6; 32]),
        ))
        .expect("bind application principal");
    journal
        .open_application_session(
            NewApplicationSession::new(session(), actor(), 1, 1, [0xa7; 16], 1, 0)
                .expect("application session facts"),
        )
        .expect("open application session");
}

fn spawn_authority(
    root: &Path,
    journal: SqliteJournal,
) -> (AuthorityHandle, tokio::task::JoinHandle<Result<(), crate::DaemonError>>) {
    let database = root.join("application-retry.sqlite3");
    let artifacts = ArtifactStore::open(
        StoreConfig::new(root.join("artifacts"), 1_024, 8_192)
            .and_then(|config| config.with_database_path(database))
            .expect("artifact store configuration"),
    )
    .expect("artifact store");
    AuthorityOwner::spawn(journal, ready_lifecycle(), artifacts, 1_024, 4, 1, 16)
        .expect("authority owner")
}

async fn stop_authority(
    authority: AuthorityHandle,
    task: tokio::task::JoinHandle<Result<(), crate::DaemonError>>,
) {
    authority.stop().await.expect("stop authority");
    task.await.expect("authority task").expect("authority shutdown");
}

fn ready_lifecycle() -> DaemonLifecycle {
    let mut lifecycle = DaemonLifecycle::starting();
    for phase in [
        StartupPhase::Lock,
        StartupPhase::Migrate,
        StartupPhase::Journal,
        StartupPhase::Artifacts,
        StartupPhase::Evidence,
        StartupPhase::Projections,
        StartupPhase::AuthorityEpoch,
        StartupPhase::DomainRecovery,
        StartupPhase::EffectRecovery,
        StartupPhase::AppRecovery,
        StartupPhase::Outbox,
        StartupPhase::Ipc,
        StartupPhase::Ready,
    ] {
        lifecycle.advance(phase).expect("advance startup lifecycle");
    }
    lifecycle
}

fn open_journal(root: &Path) -> SqliteJournal {
    SqliteJournal::open(
        root.join("application-retry.sqlite3"),
        StoreId::new([0xa8; 16]).expect("store identity"),
        SqliteJournalOptions::default(),
    )
    .expect("open application retry journal")
}

fn legacy_command() -> SchedulerCommand {
    decode_scheduler_command(&legacy_command_bytes(), CodecLimits::PRODUCTION)
        .expect("fixed historical scheduler genesis")
}

fn legacy_command_bytes() -> Vec<u8> {
    read_scheduler_v1_fixture("scheduler-command.bin")
}

fn legacy_event_bytes() -> Vec<u8> {
    read_scheduler_v1_fixture("scheduler-event.bin")
}

fn legacy_state_bytes() -> Vec<u8> {
    read_scheduler_v1_fixture("scheduler-state.bin")
}

fn read_scheduler_v1_fixture(file_name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/protocol/scheduler-v1")
            .join(file_name),
    )
    .expect("read fixed historical scheduler fixture")
}

fn actor() -> ActorId {
    ActorId::new([0xa1; 16]).expect("actor identity")
}

fn session() -> SessionId {
    SessionId::new([0xa2; 16]).expect("session identity")
}
