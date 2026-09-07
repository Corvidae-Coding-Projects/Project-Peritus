use crate::{AggregateKind, HeadExpectation};
use peritus_types::Sha256Digest;
use tempfile::TempDir;

use crate::sqlite::tests::{command, draft, event, key, open, plan};

#[test]
fn aggregate_replay_snapshot_does_not_mix_concurrent_committed_heads_and_rows() {
    let temp = TempDir::new().expect("temporary journal");
    let mut reader = open(&temp);
    let aggregate = key(AggregateKind::Scheduler, 7);
    reader
        .append(plan(
            command(1),
            Sha256Digest::new([1; 32]),
            HeadExpectation::Absent(aggregate),
            vec![draft(aggregate, 1, event(1), None, 1)],
        ))
        .expect("genesis");
    let mut writer = open(&temp);
    let transaction = reader.connection.unchecked_transaction().expect("read transaction");
    let first =
        super::load_head(&transaction, aggregate).expect("pin first snapshot").expect("head");
    writer
        .append(plan(
            command(2),
            Sha256Digest::new([2; 32]),
            HeadExpectation::Present(first),
            vec![draft(aggregate, 2, event(2), Some(event(1)), 2)],
        ))
        .expect("real competing connection commits while read snapshot is held");
    assert_eq!(writer.head(aggregate).expect("new head").expect("head").sequence().get(), 2);
    let snapshot = super::load_snapshot(&transaction, aggregate).expect("coherent old snapshot");
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].event_id(), event(1));
    transaction.commit().expect("end old read snapshot");
    let current = reader.records_for_aggregate(aggregate).expect("fresh replay");
    assert_eq!(current.len(), 2);
    assert_eq!(current[1].event_id(), event(2));
}

fn append_checkpoint(journal: &mut crate::SqliteJournal, identity: u8) {
    let aggregate = key(AggregateKind::Scheduler, 7);
    let head = journal.head(aggregate).expect("head");
    let revision = u64::from(identity);
    let install = crate::StateInstall::new(
        1,
        b"scheduler".to_vec(),
        head.map(|value| value.sequence().get()),
        revision,
        vec![identity; 32],
    )
    .expect("checkpoint install");
    let request = crate::AppendRequest::new(
        journal.store_id(),
        command(identity),
        Sha256Digest::new([identity; 32]),
        vec![head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present)],
        vec![draft(
            aggregate,
            revision,
            event(identity),
            head.map(crate::AggregateHead::event_id),
            identity,
        )],
        vec![install],
        Vec::new(),
        None,
        None,
        Vec::new(),
    );
    journal.append(request.plan().expect("checkpoint plan")).expect("checkpoint commit");
}

#[test]
fn aggregate_checkpoint_snapshot_keeps_history_and_state_at_the_same_commit() {
    let temp = TempDir::new().expect("temporary journal");
    let mut reader = open(&temp);
    append_checkpoint(&mut reader, 1);
    let mut writer = open(&temp);
    let aggregate = key(AggregateKind::Scheduler, 7);
    let transaction = reader.connection.unchecked_transaction().expect("read transaction");
    super::load_head(&transaction, aggregate).expect("pin old snapshot");
    append_checkpoint(&mut writer, 2);
    let (records, checkpoint) =
        super::checkpoint::load_checkpoint_snapshot(&transaction, aggregate, 1, b"scheduler")
            .expect("snapshot after a real concurrent commit")
            .into_parts();
    assert_eq!(records.len(), 1);
    let checkpoint = checkpoint.expect("old checkpoint");
    assert_eq!(checkpoint.revision(), 1);
    assert_eq!(checkpoint.bytes(), &[1; 32]);
    assert_eq!(checkpoint.producing_position(), records[0].global_position());
    transaction.commit().expect("release old snapshot");
    let current =
        reader.aggregate_checkpoint_snapshot(aggregate, 1, b"scheduler").expect("current snapshot");
    drop(reader);
    let restarted = open(&temp);
    assert_eq!(
        current,
        restarted
            .aggregate_checkpoint_snapshot(aggregate, 1, b"scheduler")
            .expect("restart preserves exact observation")
    );
    let (records, checkpoint) = current.into_parts();
    assert_eq!(records.len(), 2);
    let checkpoint = checkpoint.expect("new checkpoint");
    assert_eq!(checkpoint.revision(), 2);
    assert_eq!(checkpoint.bytes(), &[2; 32]);
    assert_eq!(checkpoint.producing_position(), records[1].global_position());
}

#[test]
fn aggregate_checkpoint_snapshot_preserves_absence_and_input_rejection() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    let aggregate = key(AggregateKind::Scheduler, 7);
    assert_eq!(
        journal
            .aggregate_checkpoint_snapshot(aggregate, 1, b"scheduler")
            .expect("empty snapshot")
            .into_parts(),
        (Vec::new(), None)
    );
    for (namespace, state_key) in [(0, b"scheduler".as_slice()), (1, b"".as_slice())] {
        assert_eq!(
            journal
                .aggregate_checkpoint_snapshot(aggregate, namespace, state_key)
                .expect_err("invalid state identity")
                .kind(),
            crate::JournalErrorKind::InvalidInput
        );
    }
    append_checkpoint(&mut journal, 1);
    let (records, checkpoint) = journal
        .aggregate_checkpoint_snapshot(aggregate, 1, b"missing")
        .expect("independent missing checkpoint")
        .into_parts();
    assert_eq!(records.len(), 1);
    assert!(checkpoint.is_none());
    let (records, checkpoint) = journal
        .aggregate_checkpoint_snapshot(key(AggregateKind::Scheduler, 8), 1, b"scheduler")
        .expect("independent missing aggregate")
        .into_parts();
    assert!(records.is_empty());
    assert!(checkpoint.is_some());
}

#[test]
fn aggregate_checkpoint_snapshot_rejects_corruption_in_either_component() {
    for sql in [
        "UPDATE state_records SET value = zeroblob(length(value))",
        "UPDATE events SET frame = zeroblob(length(frame))",
    ] {
        let temp = TempDir::new().expect("temporary journal");
        let mut journal = open(&temp);
        append_checkpoint(&mut journal, 1);
        journal.connection.execute_batch(sql).expect("inject corruption");
        assert_eq!(
            journal
                .aggregate_checkpoint_snapshot(key(AggregateKind::Scheduler, 7), 1, b"scheduler")
                .expect_err("corruption must fail closed")
                .kind(),
            crate::JournalErrorKind::CorruptJournal
        );
    }
}
