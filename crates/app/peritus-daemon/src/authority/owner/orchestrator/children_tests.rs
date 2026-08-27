//! Real-journal child lifecycle retry and production-router coverage.

#![allow(clippy::unwrap_used, reason = "fixed checked durable fixtures")]

use std::path::Path;

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_orchestrator::{DirectiveDestination, DirectiveId};
use peritus_review::{
    ReviewCommand, ReviewCommandFrame, ReviewCommandKind, ReviewRunPhase, commit_review_transition,
    load_review_replay, start,
};
use peritus_types::{CommandId, EventId};
use tokio::sync::mpsc;

use super::children::{child_ids, commit_review_lifecycle};
use crate::{AuthorityHandle, outbox::DestinationRouter};

#[test]
fn review_child_admission_retries_exactly_across_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("review-child-retry.sqlite3");
    let store_id = StoreId::new(identity(210)).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let genesis = review_genesis();
    let started = start(&genesis).unwrap();
    commit_review_transition(&mut journal, &genesis, &started).unwrap();
    let active = started.into_state();
    let pause = lifecycle_command(&active, 211, 212, ReviewCommandKind::PauseRun);

    commit_review_lifecycle(&mut journal, &pause).unwrap();
    drop(journal);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_review_lifecycle(&mut restarted, &pause).unwrap();
    let paused =
        load_review_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap().unwrap();
    assert_eq!(paused.phase(), ReviewRunPhase::Paused);
    assert_eq!(paused.sequence().get(), active.sequence().get() + 1);

    let resume = lifecycle_command(&paused, 213, 214, ReviewCommandKind::ResumeRun);
    commit_review_lifecycle(&mut restarted, &resume).unwrap();
    drop(restarted);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_review_lifecycle(&mut restarted, &resume).unwrap();
    let resumed =
        load_review_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap().unwrap();
    assert_eq!(resumed.phase(), ReviewRunPhase::Active);
    assert_eq!(resumed.sequence().get(), active.sequence().get() + 2);
}

#[test]
fn production_router_registers_both_supported_child_destinations() {
    let (sender, _receiver) = mpsc::channel(4);
    let authority = AuthorityHandle::new(sender);
    let router = DestinationRouter::production_children(&authority, 4).unwrap();

    assert!(router.contains(DirectiveDestination::Gates.outbox_destination()));
    assert!(router.contains(DirectiveDestination::Review.outbox_destination()));
}

#[test]
fn child_command_and_event_identities_are_restart_stable_and_domain_separated() {
    let run_id = review_genesis().run_id();
    let directive_id = DirectiveId::new(identity(215)).unwrap();
    let first = child_ids(
        b"test.child.command.v1\0",
        b"test.child.event.v1\0",
        run_id,
        directive_id,
        "derive test child identities",
    )
    .unwrap();
    let retried = child_ids(
        b"test.child.command.v1\0",
        b"test.child.event.v1\0",
        run_id,
        directive_id,
        "derive test child identities",
    )
    .unwrap();

    assert_eq!(first, retried);
    assert_ne!(first.0.as_bytes(), first.1.as_bytes());
}

fn review_genesis() -> ReviewCommand {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../orchestration/peritus-review/tests/fixtures/v1/review-command.bin"),
    )
    .unwrap();
    decode_message::<ReviewCommandFrame>(&bytes, CodecLimits::PRODUCTION).unwrap().0
}

fn lifecycle_command(
    state: &peritus_review::ReviewRunState,
    command_seed: u8,
    event_seed: u8,
    kind: ReviewCommandKind,
) -> ReviewCommand {
    ReviewCommand::new(
        CommandId::new(identity(command_seed)).unwrap(),
        EventId::new(identity(event_seed)).unwrap(),
        state.run_id(),
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.binding().revision(),
        kind,
    )
    .unwrap()
}

const fn identity(value: u8) -> [u8; 16] {
    [value; 16]
}
