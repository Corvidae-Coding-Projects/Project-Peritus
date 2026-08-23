use crate::{
    AggregateKind, AppendRequest, AuthorityEpoch, CommandResolution, EventDraft,
    ExpectedAuthorityEpoch, HeadExpectation, JournalErrorKind, StateInstall,
};
use peritus_types::{EventSequence, Sha256Digest};
use tempfile::TempDir;

use super::{command, draft, event, frame, key, open, plan, store_id};

#[test]
fn restart_preserves_exact_frames_and_command_resolution() {
    let temp = TempDir::new().expect("temporary directory");
    let aggregate = key(AggregateKind::Kernel, 16);
    let exact = frame(60);
    {
        let mut journal = open(&temp);
        let event = EventDraft::new(
            aggregate,
            EventSequence::first(),
            event(60),
            None,
            exact.clone(),
            Sha256Digest::new([60; 32]),
            Vec::new(),
        )
        .expect("event");
        journal
            .append(plan(
                command(60),
                Sha256Digest::new([61; 32]),
                HeadExpectation::Absent(aggregate),
                vec![event],
            ))
            .expect("commit before restart");
    }
    let mut reopened = open(&temp);
    let resolved = reopened
        .resolve_command(command(60), Sha256Digest::new([61; 32]))
        .expect("resolve after restart");
    let CommandResolution::Committed(batch) = resolved else {
        panic!("command must remain committed");
    };
    assert_eq!(batch.records()[0].frame_bytes(), exact.bytes());
    let export = reopened.integrity_export().expect("checked exact export");
    assert_eq!(export.records()[0].frame_bytes(), exact.bytes());
}

#[test]
fn corruption_in_payload_or_head_is_detected() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let aggregate = key(AggregateKind::Kernel, 17);
    journal
        .append(plan(
            command(70),
            Sha256Digest::new([70; 32]),
            HeadExpectation::Absent(aggregate),
            vec![draft(aggregate, 1, event(70), None, 7)],
        ))
        .expect("commit");
    journal
        .connection
        .execute("UPDATE events SET frame = ?1 WHERE global_position = 1", [[0_u8; 16].as_slice()])
        .expect("inject frame corruption");
    assert_eq!(
        journal.integrity_scan().expect_err("corruption detected").kind(),
        JournalErrorKind::CorruptJournal
    );

    let second_temp = TempDir::new().expect("temporary directory");
    let mut second = open(&second_temp);
    second
        .append(plan(
            command(71),
            Sha256Digest::new([71; 32]),
            HeadExpectation::Absent(aggregate),
            vec![draft(aggregate, 1, event(71), None, 8)],
        ))
        .expect("commit");
    second
        .connection
        .execute("UPDATE aggregate_heads SET event_hash = ?1", [[99_u8; 32].as_slice()])
        .expect("inject head corruption");
    assert_eq!(
        second.integrity_scan().expect_err("head corruption detected").kind(),
        JournalErrorKind::CorruptJournal
    );
}

#[test]
fn authority_epoch_cas_is_monotonic_across_restart_and_overflow_is_typed() {
    let temp = TempDir::new().expect("temporary directory");
    let first = {
        let mut journal = open(&temp);
        journal.allocate_authority_epoch(ExpectedAuthorityEpoch::Absent).expect("first epoch")
    };
    assert_eq!(first.get(), 1);
    let mut reopened = open(&temp);
    assert_eq!(
        reopened
            .allocate_authority_epoch(ExpectedAuthorityEpoch::Absent)
            .expect_err("stale absence")
            .kind(),
        JournalErrorKind::StaleAuthorityEpoch
    );
    let second = reopened
        .allocate_authority_epoch(ExpectedAuthorityEpoch::Current(first.epoch()))
        .expect("second epoch");
    assert_eq!(second.get(), 2);
    assert_eq!(
        AuthorityEpoch::new(u64::MAX)
            .expect("maximum epoch")
            .checked_next()
            .expect_err("typed overflow")
            .kind(),
        JournalErrorKind::SequenceOverflow
    );
}

#[test]
fn integrity_rejects_state_history_gaps_and_current_row_rewinds() {
    let gap_temp = TempDir::new().expect("temporary directory");
    let mut gap = open(&gap_temp);
    append_two_state_revisions(&mut gap, 80);
    gap.connection
        .execute("DELETE FROM state_record_history WHERE revision = 1", [])
        .expect("remove first history revision");
    assert_eq!(
        gap.integrity_scan().expect_err("history gap is corruption").kind(),
        JournalErrorKind::CorruptJournal
    );

    let rewind_temp = TempDir::new().expect("temporary directory");
    let mut rewind = open(&rewind_temp);
    append_two_state_revisions(&mut rewind, 90);
    rewind
        .connection
        .execute_batch(
            "UPDATE state_records
                SET revision = 1,
                    value_digest = (SELECT value_digest FROM state_record_history WHERE revision = 1),
                    value = (SELECT value FROM state_record_history WHERE revision = 1),
                    producing_position = (SELECT producing_position FROM state_record_history WHERE revision = 1);",
        )
        .expect("rewind current row to valid older history");
    assert_eq!(
        rewind.integrity_scan().expect_err("current rewind is corruption").kind(),
        JournalErrorKind::CorruptJournal
    );
}

fn append_two_state_revisions(journal: &mut super::SqliteJournal, identity: u8) {
    let aggregate = key(AggregateKind::Kernel, identity);
    let state_key = b"durable-state".to_vec();
    let first = AppendRequest::new(
        store_id(),
        command(identity),
        Sha256Digest::new([identity; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![draft(aggregate, 1, event(identity), None, identity)],
        vec![
            StateInstall::new(7, state_key.clone(), None, 1, vec![identity])
                .expect("first state revision"),
        ],
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("first state append plan");
    journal.append(first).expect("first state append");

    let next_identity = identity.checked_add(1).expect("test identity successor");
    let head = journal.head(aggregate).expect("head read").expect("head exists");
    let second = AppendRequest::new(
        store_id(),
        command(next_identity),
        Sha256Digest::new([next_identity; 32]),
        vec![HeadExpectation::Present(head)],
        vec![draft(aggregate, 2, event(next_identity), Some(event(identity)), next_identity)],
        vec![
            StateInstall::new(7, state_key, Some(1), 2, vec![next_identity])
                .expect("second state revision"),
        ],
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("second state append plan");
    journal.append(second).expect("second state append");
}
