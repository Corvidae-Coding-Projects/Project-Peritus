use peritus_types::{CommandId, EventId, Sha256Digest};
use rusqlite::params;
use tempfile::TempDir;

use crate::sqlite::tests::{draft, key, open};
use crate::{
    AggregateKind, AppendPlan, AppendRequest, HeadExpectation, JournalErrorKind, StateInstall,
};

fn plan(journal: &crate::SqliteJournal, sequence: u64, bytes: Vec<u8>) -> AppendPlan {
    let mut identity = [0; 16];
    identity[..8].copy_from_slice(&sequence.to_be_bytes());
    identity[15] = 1;
    let aggregate = key(AggregateKind::Scheduler, 7);
    let head = journal.head(aggregate).expect("head");
    AppendRequest::new(
        journal.store_id(),
        CommandId::new(identity).expect("command"),
        Sha256Digest::new([7; 32]),
        vec![head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present)],
        vec![draft(
            aggregate,
            sequence,
            EventId::new(identity).expect("event"),
            head.map(crate::AggregateHead::event_id),
            7,
        )],
        vec![
            StateInstall::new(
                7,
                b"history".to_vec(),
                head.map(|head| head.sequence().get()),
                sequence,
                bytes,
            )
            .expect("state"),
        ],
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("plan")
}

#[test]
fn shared_history_preserves_empty_leaf_branch_and_maximum_values_after_restart() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    let lengths = [0, 1, 512, 513, 8192, 8193, 16 * 1024 * 1024];
    for (index, length) in lengths.iter().copied().enumerate() {
        let bytes = vec![u8::try_from(index).expect("index"); length];
        journal.append(plan(&journal, index as u64 + 1, bytes)).expect("commit");
    }
    drop(journal);
    let mut restarted = open(&temp);
    for (index, length) in lengths.iter().copied().enumerate().rev() {
        let state = restarted
            .state_record_revision(7, b"history", index as u64 + 1)
            .expect("history read")
            .expect("history present");
        assert_eq!(state.bytes(), vec![u8::try_from(index).expect("index"); length]);
        assert_eq!(state.digest(), peritus_codec::sha256(state.bytes()));
        assert_eq!(state.producing_position(), index as u64 + 1);
    }
    assert_eq!(restarted.integrity_scan().expect("full integrity").event_count(), 7);
}

#[test]
fn shared_history_rejects_missing_corrupt_and_wrong_root_nodes() {
    for damage in [
        "DELETE FROM state_history_nodes WHERE level = 0",
        "UPDATE state_history_nodes SET payload = zeroblob(length(payload)) WHERE level = 0",
        "UPDATE state_history_nodes SET byte_length = 0 WHERE level = 1",
        "UPDATE state_record_history SET root_digest = zeroblob(32)",
        "UPDATE state_record_history SET value_digest = zeroblob(32)",
        "PRAGMA foreign_keys = OFF; UPDATE state_record_history SET producing_position = 99; PRAGMA foreign_keys = ON;",
    ] {
        let temp = TempDir::new().expect("temporary journal");
        let mut journal = open(&temp);
        journal.append(plan(&journal, 1, vec![7; 8193])).expect("commit");
        journal.connection.execute_batch(damage).expect("damage disposable fixture");
        assert_eq!(
            journal.state_record_revision(7, b"history", 1).expect_err("history corruption").kind(),
            JournalErrorKind::CorruptJournal
        );
        assert_eq!(
            journal.integrity_scan().expect_err("full scan corruption").kind(),
            JournalErrorKind::CorruptJournal
        );
    }
}

#[test]
fn shared_history_node_bounds_reject_malformed_encodings() {
    use super::node::{Node, root_level};
    for (level, length, payload) in [
        (5, 1, vec![0; 32]),
        (0, 513, vec![0; 513]),
        (0, 1, vec![]),
        (1, 0, vec![]),
        (1, 8193, vec![0; 512]),
        (1, 513, vec![0; 33]),
        (4, crate::record::MAX_STATE_BYTES + 1, vec![0; 288]),
    ] {
        assert_eq!(
            Node::new(level, length, payload).expect_err("invalid node").kind(),
            JournalErrorKind::CorruptJournal
        );
    }
    assert_eq!(
        root_level(crate::record::MAX_STATE_BYTES + 1).expect_err("oversized value").kind(),
        JournalErrorKind::CorruptJournal
    );
}

#[test]
fn shared_history_rejects_hash_consistent_noncanonical_tree_shapes() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    journal.append(plan(&journal, 1, vec![7; 512])).expect("commit");
    let root: Vec<u8> = journal
        .connection
        .query_row("SELECT root_digest FROM state_record_history", [], |row| row.get(0))
        .expect("root");
    let taller =
        super::node::Node::new(1, 512, root).expect("valid internal node, not minimal root");
    journal
        .connection
        .execute(
            "INSERT INTO state_history_nodes VALUES (?1, 1, 512, ?2)",
            params![taller.digest().as_bytes().as_slice(), taller.payload()],
        )
        .expect("insert hash-consistent node");
    journal
        .connection
        .execute(
            "UPDATE state_record_history SET root_digest = ?1",
            [taller.digest().as_bytes().as_slice()],
        )
        .expect("select malformed root");
    assert_eq!(
        journal.state_record_revision(7, b"history", 1).expect_err("nonminimal root").kind(),
        JournalErrorKind::CorruptJournal
    );

    let wrong_child = super::node::Node::new(0, 1, vec![9]).expect("leaf");
    let wrong_root = super::node::Node::new(1, 513, wrong_child.digest().as_bytes().repeat(2))
        .expect("canonical parent bytes alone");
    for node in [&wrong_child, &wrong_root] {
        journal
            .connection
            .execute(
                "INSERT INTO state_history_nodes VALUES (?1, ?2, ?3, ?4)",
                params![
                    node.digest().as_bytes().as_slice(),
                    i64::from(node.level()),
                    i64::try_from(node.byte_length()).expect("bounded node length"),
                    node.payload()
                ],
            )
            .expect("insert hash-consistent shape mismatch");
    }
    journal
        .connection
        .execute(
            "UPDATE state_record_history SET root_digest = ?1",
            [wrong_root.digest().as_bytes().as_slice()],
        )
        .expect("select child mismatch");
    assert_eq!(
        journal.state_record_revision(7, b"history", 1).expect_err("wrong child span").kind(),
        JournalErrorKind::CorruptJournal
    );
}

#[test]
fn shared_history_failed_append_rolls_back_nodes_event_history_and_current_state() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    journal.append(plan(&journal, 1, vec![1; 1024])).expect("first commit");
    let count = || {
        journal
            .connection
            .query_row("SELECT COUNT(*) FROM state_history_nodes", [], |row| row.get::<_, i64>(0))
            .expect("node count")
    };
    let before = count();
    journal.connection.execute_batch("CREATE TEMP TRIGGER fail_history BEFORE INSERT ON state_record_history BEGIN SELECT RAISE(ABORT, 'injected history failure'); END;").expect("failpoint");
    assert!(journal.append(plan(&journal, 2, vec![2; 16384])).is_err());
    let after: i64 = journal
        .connection
        .query_row("SELECT COUNT(*) FROM state_history_nodes", [], |row| row.get(0))
        .expect("node count");
    assert_eq!(before, after);
    assert!(journal.state_record_revision(7, b"history", 2).expect("history read").is_none());
    assert_eq!(
        journal.state_record(7, b"history").expect("current").expect("state").bytes(),
        &[1; 1024]
    );
    assert_eq!(journal.integrity_scan().expect("unchanged valid journal").event_count(), 1);
}

#[test]
fn shared_history_never_accepts_or_overwrites_a_corrupt_reused_node() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    journal.append(plan(&journal, 1, vec![7; 512])).expect("first commit");
    journal
        .connection
        .execute("UPDATE state_history_nodes SET payload = zeroblob(512)", [])
        .expect("corrupt reused leaf");
    assert_eq!(
        journal.append(plan(&journal, 2, vec![7; 1024])).expect_err("corrupt node reuse").kind(),
        JournalErrorKind::CorruptJournal
    );
    assert_eq!(
        journal
            .head(key(AggregateKind::Scheduler, 7))
            .expect("head")
            .expect("present")
            .sequence()
            .get(),
        1
    );
    assert!(journal.state_record_revision(7, b"history", 2).expect("history read").is_none());
}

#[test]
fn shared_history_repeated_node_is_reread_and_failed_statements_release_before_retry() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    journal
        .connection
        .execute_batch(
            "CREATE TEMP TRIGGER corrupt_inserted_leaf AFTER INSERT ON state_history_nodes
             WHEN NEW.level = 0 BEGIN
               UPDATE state_history_nodes SET payload = zeroblob(length(payload))
               WHERE node_digest = NEW.node_digest;
             END;",
        )
        .expect("damage only the disposable fixture during insertion");
    // The second identical leaf must be re-read, even though this operation just inserted it.
    // Reusing the SQL program must not turn the first visit into cached validation authority.
    assert_eq!(
        journal.append(plan(&journal, 1, vec![7; 1024])).expect_err("reread damaged leaf").kind(),
        JournalErrorKind::CorruptJournal
    );
    assert!(journal.head(key(AggregateKind::Scheduler, 7)).expect("head").is_none());
    assert_eq!(
        journal
            .connection
            .query_row("SELECT COUNT(*) FROM state_history_nodes", [], |row| row.get::<_, i64>(0))
            .expect("rolled back nodes"),
        0
    );
    journal
        .connection
        .execute_batch("DROP TRIGGER corrupt_inserted_leaf")
        .expect("remove disposable failpoint");
    journal.append(plan(&journal, 1, vec![7; 1024])).expect("retry exact rejected command");
    for bytes in [vec![8; 8193], vec![9; 513]] {
        let next = journal
            .head(key(AggregateKind::Scheduler, 7))
            .expect("head")
            .expect("present")
            .sequence()
            .get()
            + 1;
        journal.append(plan(&journal, next, bytes)).expect("new statement bindings");
    }
    drop(journal);
    let mut restarted = open(&temp);
    for (revision, expected) in [(3, vec![9; 513]), (1, vec![7; 1024]), (2, vec![8; 8193])] {
        assert_eq!(
            restarted
                .state_record_revision(7, b"history", revision)
                .expect("checked history")
                .expect("present")
                .bytes(),
            expected
        );
    }
    assert_eq!(restarted.integrity_scan().expect("complete exact history").event_count(), 3);
}

#[test]
fn shared_history_sqlite_retention_is_bounded_below_full_growing_values() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    let mut value = vec![0; 300];
    let mut logical_bytes = 0_u64;
    for revision in 1_u64..=1024 {
        value[..8].copy_from_slice(&revision.to_be_bytes());
        value.extend_from_slice(&revision.to_be_bytes());
        value.extend_from_slice(&(!revision).to_be_bytes());
        logical_bytes += value.len() as u64;
        journal.append(plan(&journal, revision, value.clone())).expect("growing history commit");
    }
    journal.connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").expect("checkpoint");
    let pages = journal.storage_pages().expect("physical allocation");
    let allocated = pages.page_count() * pages.page_size();
    assert!(
        allocated < logical_bytes / 2,
        "physical SQLite bytes {allocated} must be below half of logical history {logical_bytes}"
    );
    for revision in [1_u64, 512, 1024] {
        let record = journal
            .state_record_revision(7, b"history", revision)
            .expect("history read")
            .expect("present");
        assert_eq!(
            record.bytes().len(),
            300 + 16 * usize::try_from(revision).expect("bounded revision")
        );
        assert_eq!(&record.bytes()[..8], &revision.to_be_bytes());
    }
    journal.integrity_scan().expect("all exact historical values remain checked");
}
