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
