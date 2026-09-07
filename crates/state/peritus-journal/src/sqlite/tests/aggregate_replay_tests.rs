use crate::{AggregateKey, AggregateKind, HeadExpectation, JournalErrorKind, SqliteJournal};
use peritus_types::Sha256Digest;
use rusqlite::params;
use tempfile::TempDir;

use super::{command, draft, event, key, open, plan};

fn append_event(journal: &mut SqliteJournal, aggregate: AggregateKey, identity: u8) {
    let head = journal.head(aggregate).expect("observe head");
    let sequence = head.map_or(1, |head| head.sequence().get() + 1);
    journal
        .append(plan(
            command(identity),
            Sha256Digest::new([identity; 32]),
            head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present),
            vec![draft(
                aggregate,
                sequence,
                event(identity),
                head.map(crate::AggregateHead::event_id),
                identity,
            )],
        ))
        .expect("append exact event");
}

#[test]
fn aggregate_replay_retains_exact_interleaved_records_across_restart() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    let selected = key(AggregateKind::Scheduler, 1);
    let other = key(AggregateKind::Scheduler, 2);
    assert!(journal.records_for_aggregate(selected).expect("absent aggregate").is_empty());
    for identity in 1..=64 {
        append_event(&mut journal, selected, identity);
        append_event(&mut journal, other, identity + 64);
    }
    let expected = journal
        .integrity_export()
        .expect("independent full integrity scan")
        .records()
        .iter()
        .filter(|record| record.aggregate() == selected)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(expected.len(), 64);
    assert_eq!(journal.records_for_aggregate(selected).expect("selected replay"), expected);
    drop(journal);
    assert_eq!(open(&temp).records_for_aggregate(selected).expect("reopened replay"), expected);
}

#[test]
fn aggregate_replay_rejects_orphaned_events() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    let aggregate = key(AggregateKind::Scheduler, 3);
    append_event(&mut journal, aggregate, 1);
    journal.connection.execute("DELETE FROM aggregate_heads", []).expect("remove head");
    assert_eq!(
        journal
            .records_for_aggregate(aggregate)
            .expect_err("orphaned events are corruption")
            .kind(),
        JournalErrorKind::CorruptJournal
    );
}

#[test]
fn aggregate_replay_checks_complete_head_not_only_hash() {
    for field in ["sequence", "event_id"] {
        let temp = TempDir::new().expect("temporary journal");
        let mut journal = open(&temp);
        let aggregate = key(AggregateKind::Scheduler, 4);
        append_event(&mut journal, aggregate, 1);
        if field == "sequence" {
            journal.connection.execute("UPDATE aggregate_heads SET sequence = 2", [])
        } else {
            journal.connection.execute(
                "UPDATE aggregate_heads SET event_id = ?1",
                [event(2).as_bytes().as_slice()],
            )
        }
        .expect("change head metadata without changing hash");
        assert_eq!(
            journal
                .records_for_aggregate(aggregate)
                .expect_err("head differs from final event")
                .kind(),
            JournalErrorKind::CorruptJournal,
            "changed {field}"
        );
    }
}

#[test]
fn aggregate_replay_rejects_self_consistent_hash_with_broken_predecessor() {
    for genesis in [true, false] {
        let temp = TempDir::new().expect("temporary journal");
        let mut journal = open(&temp);
        let aggregate = key(AggregateKind::Scheduler, 5);
        append_event(&mut journal, aggregate, 1);
        if !genesis {
            append_event(&mut journal, aggregate, 2);
        }
        let identity = if genesis { 1 } else { 2 };
        let sequence = u64::from(identity);
        let previous = (!genesis).then(|| event(1));
        let broken_hash = Sha256Digest::new([99; 32]);
        let changed = draft(aggregate, sequence, event(identity), previous, identity);
        let self_consistent_hash =
            crate::hash_chain::event_hash(&changed, broken_hash, command(identity));
        journal
            .connection
            .execute(
                "UPDATE events SET previous_event_hash = ?1, event_hash = ?2 WHERE sequence = ?3",
                params![
                    broken_hash.as_bytes().as_slice(),
                    self_consistent_hash.as_bytes().as_slice(),
                    i64::from(identity)
                ],
            )
            .expect("change predecessor and recompute the row hash");
        journal
            .connection
            .execute(
                "UPDATE aggregate_heads SET event_hash = ?1",
                [self_consistent_hash.as_bytes().as_slice()],
            )
            .expect("retain matching final hash");
        assert_eq!(
            journal
                .records_for_aggregate(aggregate)
                .expect_err("broken predecessor is corruption")
                .kind(),
            JournalErrorKind::CorruptJournal
        );
    }
}

#[test]
fn aggregate_replay_rechecks_old_frame_bytes_and_sequence_gaps() {
    for remove in [true, false] {
        let temp = TempDir::new().expect("temporary journal");
        let mut journal = open(&temp);
        let aggregate = key(AggregateKind::Scheduler, 6);
        for identity in 1..=3 {
            append_event(&mut journal, aggregate, identity);
        }
        if remove {
            journal.connection.execute("DELETE FROM events WHERE sequence = 2", [])
        } else {
            journal
                .connection
                .execute("UPDATE events SET frame = zeroblob(16) WHERE sequence = 1", [])
        }
        .expect("damage historical event");
        assert_eq!(
            journal
                .records_for_aggregate(aggregate)
                .expect_err("historical damage is corruption")
                .kind(),
            JournalErrorKind::CorruptJournal
        );
    }
}
