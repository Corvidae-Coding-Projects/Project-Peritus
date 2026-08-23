//! Durable-state history and late idempotent-retry integration coverage.

mod support;

use peritus_journal::{AppendRequest, CommandResolution, HeadExpectation, StateInstall};
use peritus_types::{CommandId, EventId, Sha256Digest};
use tempfile::TempDir;

use support::{aggregate, command, digest, event, frame, open, store_id};

const STATE_NAMESPACE: u16 = 900;
const STATE_KEY: &[u8] = b"durable-lineage";

fn state_plan(
    head: HeadExpectation,
    command_id: CommandId,
    request_digest: Sha256Digest,
    event_id: EventId,
    previous_event_id: Option<EventId>,
    revision: u64,
    value: &[u8],
) -> peritus_journal::AppendPlan {
    let expected_revision = revision.checked_sub(1).filter(|value| *value != 0);
    let marker = u8::try_from(revision).expect("small fixture revision");
    let draft = peritus_journal::EventDraft::new(
        head.key(),
        peritus_types::EventSequence::new(revision).expect("positive fixture revision"),
        event_id,
        previous_event_id,
        frame(marker),
        digest(marker + 10),
        Vec::new(),
    )
    .expect("state event");
    AppendRequest::new(
        store_id(),
        command_id,
        request_digest,
        vec![head],
        vec![draft],
        vec![
            StateInstall::new(
                STATE_NAMESPACE,
                STATE_KEY.to_vec(),
                expected_revision,
                revision,
                value.to_vec(),
            )
            .expect("state install"),
        ],
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("state append plan")
}

#[test]
fn older_command_replay_returns_its_original_batch_without_rolling_back_current_state() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let aggregate = aggregate(peritus_journal::AggregateKind::Budget, 1);

    let first_command = command(1);
    let first_request_digest = digest(1);
    let first_event = event(1);
    let first_batch = journal
        .append(state_plan(
            HeadExpectation::Absent(aggregate),
            first_command,
            first_request_digest,
            first_event,
            None,
            1,
            b"revision-one",
        ))
        .expect("first commit");

    let first_head = journal.head(aggregate).expect("head read").expect("first head");
    journal
        .append(state_plan(
            HeadExpectation::Present(first_head),
            command(2),
            digest(2),
            event(2),
            Some(first_event),
            2,
            b"revision-two",
        ))
        .expect("second commit");

    let replayed = journal
        .append(state_plan(
            HeadExpectation::Absent(aggregate),
            first_command,
            first_request_digest,
            first_event,
            None,
            1,
            b"revision-one",
        ))
        .expect("older exact command replay");
    assert_eq!(replayed.batch_hash(), first_batch.batch_hash());
    assert_eq!(replayed.first_position(), 1);
    assert_eq!(replayed.last_position(), 1);

    let current =
        journal.state_record(STATE_NAMESPACE, STATE_KEY).expect("current state").expect("present");
    assert_eq!(current.revision(), 2);
    assert_eq!(current.bytes(), b"revision-two");
    let historical = journal
        .state_record_revision(STATE_NAMESPACE, STATE_KEY, 1)
        .expect("historical state")
        .expect("revision one retained");
    assert_eq!(historical.bytes(), b"revision-one");
    assert_eq!(historical.producing_position(), 1);
    assert_eq!(
        journal
            .state_record_revision(STATE_NAMESPACE, STATE_KEY, 2)
            .expect("second history")
            .expect("revision two retained")
            .producing_position(),
        2
    );
    assert_eq!(journal.records_for_aggregate(aggregate).expect("history").len(), 2);
    assert!(matches!(
        journal
            .resolve_command(first_command, first_request_digest)
            .expect("command resolution"),
        CommandResolution::Committed(batch) if batch.last_position() == 1
    ));
}
