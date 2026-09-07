use peritus_types::Sha256Digest;
use tempfile::TempDir;

use super::{command, draft, event, key, open, plan};
use crate::{AggregateKind, AppendPlan, CommandResolution, HeadExpectation, JournalErrorKind};

fn genesis(identity: u8) -> AppendPlan {
    let aggregate = key(AggregateKind::Scheduler, identity);
    plan(
        command(identity),
        Sha256Digest::new([identity; 32]),
        HeadExpectation::Absent(aggregate),
        vec![draft(aggregate, 1, event(identity), None, identity)],
    )
}

#[test]
fn guarded_receipt_extends_only_its_own_instance_and_append_generation() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    let before = journal.observe_replay().expect("initial observation");
    let (receipt, after) = journal.append_observed(genesis(10), &before).expect("guarded append");
    assert_eq!(receipt.last_position(), 1);
    assert!(!journal.replay_observation_is_current(&before).expect("old observation"));
    assert!(journal.replay_observation_is_current(&after).expect("new observation"));

    let mut other = open(&temp);
    assert!(!other.replay_observation_is_current(&after).expect("other connection"));
    assert_eq!(
        other.append_observed(genesis(11), &after).expect_err("foreign observation").kind(),
        JournalErrorKind::StaleHead
    );
    journal.append(genesis(12)).expect("ordinary same-owner append");
    assert!(!journal.replay_observation_is_current(&after).expect("intervening append"));
    drop(journal);
    let restarted = open(&temp);
    assert!(!restarted.replay_observation_is_current(&after).expect("reopened connection"));
}

#[test]
fn external_write_between_planning_check_and_append_is_rejected_without_new_rows() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    journal.append(genesis(10)).expect("initial event");
    let observation = journal.observe_replay().expect("verified replay observation");
    assert!(journal.replay_observation_is_current(&observation).expect("planning check"));

    // The head and command being appended are unchanged. Only an old event was corrupted,
    // after the planning check and before the actual write transaction acquires its lock.
    let external =
        rusqlite::Connection::open(temp.path().join("journal.sqlite3")).expect("external writer");
    external
        .execute("UPDATE events SET frame = zeroblob(length(frame))", [])
        .expect("external old-history corruption");
    assert_eq!(
        journal.append_observed(genesis(11), &observation).expect_err("stale replay").kind(),
        JournalErrorKind::StaleHead
    );
    assert!(matches!(
        journal.resolve_command(command(11), Sha256Digest::new([11; 32])).expect("no command"),
        CommandResolution::DefinitelyAbsent
    ));
    assert!(journal.head(key(AggregateKind::Scheduler, 11)).expect("no new head").is_none());
    assert_eq!(
        journal
            .records_for_aggregate(key(AggregateKind::Scheduler, 10))
            .expect_err("cold replay still rejects corruption")
            .kind(),
        JournalErrorKind::CorruptJournal
    );
}

#[test]
fn failed_and_lost_acknowledgement_appends_invalidate_observations() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    journal.append(genesis(10)).expect("initial event");
    let observation = journal.observe_replay().expect("before conflicting event");
    let aggregate = key(AggregateKind::Scheduler, 11);
    let duplicate = plan(
        command(11),
        Sha256Digest::new([11; 32]),
        HeadExpectation::Absent(aggregate),
        vec![draft(aggregate, 1, event(10), None, 11)],
    );
    assert!(journal.append_observed(duplicate, &observation).is_err());
    assert!(!journal.replay_observation_is_current(&observation).expect("failure invalidation"));
    let observation = journal.observe_replay().expect("before lost acknowledgement");
    assert_eq!(
        journal
            .append_losing_acknowledgement(genesis(12))
            .expect_err("lost acknowledgement")
            .kind(),
        JournalErrorKind::IndeterminateCommit
    );
    assert!(!journal.replay_observation_is_current(&observation).expect("unknown outcome"));
    assert!(matches!(
        journal.resolve_command(command(12), Sha256Digest::new([12; 32])).expect("resolve receipt"),
        CommandResolution::Committed(_)
    ));
}

#[test]
fn mutable_owner_ledgers_do_not_invalidate_unchanged_aggregate_history() {
    let temp = TempDir::new().expect("temporary journal");
    let mut journal = open(&temp);
    let observation = journal.observe_replay().expect("initial observation");
    journal
        .allocate_authority_epoch(crate::ExpectedAuthorityEpoch::Absent)
        .expect("mutable authority clock");
    assert!(journal.replay_observation_is_current(&observation).expect("unchanged history"));
    journal.append_observed(genesis(10), &observation).expect("guarded append");
}
