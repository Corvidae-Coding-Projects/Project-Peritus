//! Durable D0 Agent aggregate projection behavior.

use peritus_codec::{CodecLimits, encode_message};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_projection::{AgentProjection, ProjectionErrorKind, replay_from_genesis};
use peritus_protocol::{
    AgentCountersDto, AgentEventDto, AgentEventKindDto, AgentPhaseDto, AgentResumablePhaseDto,
};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, EventSequence, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, TurnId, WorkspaceId,
};

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([1; 16]).expect("acceptance"),
        HarnessId::new([2; 16]).expect("harness"),
        WorkspaceId::new([3; 16]).expect("workspace"),
        Generation::new(1).expect("generation"),
        RevisionNumber::new(1).expect("revision"),
        PolicyId::new([4; 16]).expect("policy"),
        ProviderProfileId::new([5; 16]).expect("provider"),
    )
}

fn append_agent_event(
    journal: &mut SqliteJournal,
    aggregate: AggregateKey,
    turn_id: TurnId,
    sequence: u64,
    id: u8,
    prior: Option<EventId>,
) {
    let event_id = EventId::new([id; 16]).expect("event");
    let command_id = CommandId::new([id + 20; 16]).expect("command");
    let event = AgentEventDto::new(
        event_id,
        command_id,
        EventSequence::new(sequence).expect("sequence"),
        prior,
        turn_id,
        revision(),
        AgentPhaseDto::Active(AgentResumablePhaseDto::StreamingResponse),
        AgentEventKindDto::ProviderEventObserved,
        Sha256Digest::new([id + 1; 32]),
        AgentCountersDto::new(0, sequence, 1, 10, 0, 0, sequence),
        vec![id],
        CodecLimits::PRODUCTION,
    )
    .expect("agent event");
    let frame = ExactFrame::new(
        encode_message(&event, CodecLimits::PRODUCTION).expect("encode agent event"),
    )
    .expect("exact frame");
    let head = journal.head(aggregate).expect("head");
    let draft = EventDraft::new(
        aggregate,
        EventSequence::new(sequence).expect("sequence"),
        event_id,
        prior,
        frame,
        Sha256Digest::new([90; 32]),
        Vec::new(),
    )
    .expect("draft");
    let expectation = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let plan = AppendRequest::new(
        StoreId::new([9; 16]).expect("store"),
        command_id,
        Sha256Digest::new([id + 2; 32]),
        vec![expectation],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("append plan");
    journal.append(plan).expect("append agent event");
}

#[test]
fn projection_tracks_latest_phase_digest_and_counters() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut journal = SqliteJournal::open(
        temp.path().join("journal.sqlite3"),
        StoreId::new([9; 16]).expect("store"),
        SqliteJournalOptions::default(),
    )
    .expect("journal");
    let turn_id = TurnId::new([10; 16]).expect("turn");
    let aggregate = AggregateKey::new(
        AggregateKind::Agent,
        AggregateId::new(*turn_id.as_bytes()).expect("derived aggregate"),
    );
    let first = EventId::new([30; 16]).expect("event");
    append_agent_event(&mut journal, aggregate, turn_id, 1, 30, None);
    append_agent_event(&mut journal, aggregate, turn_id, 2, 31, Some(first));

    let export = journal.integrity_export().expect("export");
    let projection = AgentProjection::new().expect("projection");
    let replay = replay_from_genesis(&projection, &export).expect("agent replay");
    let entry = replay.state().get(aggregate).expect("entry");
    assert_eq!(entry.last_position(), 2);
    assert_eq!(entry.sequence(), 2);
    assert_eq!(entry.event_kind(), 5);
    assert_eq!(entry.counters().provider_events(), 2);
    assert_eq!(entry.successor_state_digest(), Sha256Digest::new([32; 32]));
    assert_eq!(replay.state().len(), 1);
}

#[test]
fn projection_rejects_turn_identity_not_derived_from_agent_aggregate() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut journal = SqliteJournal::open(
        temp.path().join("journal.sqlite3"),
        StoreId::new([9; 16]).expect("store"),
        SqliteJournalOptions::default(),
    )
    .expect("journal");
    let aggregate =
        AggregateKey::new(AggregateKind::Agent, AggregateId::new([40; 16]).expect("aggregate"));
    append_agent_event(
        &mut journal,
        aggregate,
        TurnId::new([41; 16]).expect("different turn"),
        1,
        42,
        None,
    );
    let export = journal.integrity_export().expect("export");
    let error = replay_from_genesis(&AgentProjection::new().expect("projection"), &export)
        .expect_err("identity mismatch");
    assert_eq!(error.kind(), ProjectionErrorKind::FoldInvariant);
}
