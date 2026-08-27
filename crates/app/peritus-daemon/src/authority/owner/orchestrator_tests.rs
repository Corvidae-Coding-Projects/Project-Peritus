//! Real-journal tests for owner-confined E0 directive acknowledgement.

#![allow(clippy::unwrap_used, reason = "fixed checked authority fixtures")]

use std::path::Path;

use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_orchestrator::{
    DirectiveDeliveryState, DirectiveDestination, DirectiveId, DirectiveKind,
    DirectivePayloadBinding, OrchestratorCommand, OrchestratorCommandFrame,
    OrchestratorCommandKind, PendingDirective, commit_orchestrator_transition,
    directive_payload_digest, load_orchestrator_replay,
};
use peritus_types::{CommandId, EventId};

use super::orchestrator::settle_claimed_directive;
use crate::outbox::{OrchestratorDirectiveClaim, TypedOutboxClaim, decode_claim};
use crate::{DaemonErrorCode, DaemonRecovery};

#[test]
fn owner_settlement_atomically_acknowledges_e0_and_c0() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("owner-e0-success.sqlite3");
    let store = StoreId::new(identity(1)).unwrap();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let claim = claimed_directive(&mut journal, 40);
    let run_id = claim.command().run_id();

    settle_claimed_directive(&mut journal, &claim).unwrap();

    assert!(journal.claim_outbox(30, 40).unwrap().is_none());
    let replay = load_orchestrator_replay(&journal, run_id).unwrap();
    assert_eq!(replay.events().len(), 3);
    let state = replay.rebuild().unwrap().expect("settled E0 state");
    assert_eq!(
        state.pending_directive().unwrap().delivery_state(),
        DirectiveDeliveryState::Acknowledged,
    );
}

#[test]
fn owner_restart_retry_resolves_the_same_acknowledgement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("owner-e0-retry.sqlite3");
    let store = StoreId::new(identity(2)).unwrap();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let claim = claimed_directive(&mut journal, 50);
    let run_id = claim.command().run_id();
    settle_claimed_directive(&mut journal, &claim).unwrap();
    drop(journal);

    let mut reopened = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    settle_claimed_directive(&mut reopened, &claim).unwrap();

    let replay = load_orchestrator_replay(&reopened, run_id).unwrap();
    assert_eq!(replay.events().len(), 3);
    assert!(reopened.claim_outbox(30, 40).unwrap().is_none());
}

#[test]
fn owner_rejects_a_claim_that_is_absent_from_the_target_run_history() {
    let first_directory = tempfile::tempdir().unwrap();
    let first_path = first_directory.path().join("owner-e0-claim-source.sqlite3");
    let first_store = StoreId::new(identity(3)).unwrap();
    let mut first =
        SqliteJournal::open(&first_path, first_store, SqliteJournalOptions::default()).unwrap();
    let claim = claimed_directive(&mut first, 60);

    let second_directory = tempfile::tempdir().unwrap();
    let second_path = second_directory.path().join("owner-e0-claim-target.sqlite3");
    let second_store = StoreId::new(identity(4)).unwrap();
    let mut second =
        SqliteJournal::open(&second_path, second_store, SqliteJournalOptions::default()).unwrap();
    let target_claim = claimed_directive(&mut second, 70);
    let target_run = target_claim.command().run_id();

    let error = settle_claimed_directive(&mut second, &claim)
        .expect_err("another durable publication must not settle this E0 run");

    assert_eq!(error.code_kind(), DaemonErrorCode::CorruptState);
    assert_eq!(error.recovery(), DaemonRecovery::ReadOnly);
    let replay = load_orchestrator_replay(&second, target_run).unwrap();
    assert_eq!(replay.events().len(), 2);
    assert_eq!(
        replay.rebuild().unwrap().unwrap().pending_directive().unwrap().delivery_state(),
        DirectiveDeliveryState::Published,
    );
}

#[test]
fn stale_fence_rolls_back_before_the_current_claim_can_settle() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("owner-e0-stale-fence.sqlite3");
    let store = StoreId::new(identity(5)).unwrap();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let stale = claimed_directive(&mut journal, 80);
    let current_message = journal.claim_outbox(21, 31).unwrap().expect("reclaimed E0 directive");
    let current = orchestrator_claim(&current_message);
    let run_id = stale.command().run_id();

    let error = settle_claimed_directive(&mut journal, &stale)
        .expect_err("expired claim fence must reject atomic settlement");

    assert_eq!(error.code_kind(), DaemonErrorCode::RecoveryRequired);
    assert_eq!(error.recovery(), DaemonRecovery::Reconcile);
    let replay = load_orchestrator_replay(&journal, run_id).unwrap();
    assert_eq!(replay.events().len(), 2);
    assert_eq!(
        replay.rebuild().unwrap().unwrap().pending_directive().unwrap().delivery_state(),
        DirectiveDeliveryState::Published,
    );

    settle_claimed_directive(&mut journal, &current).unwrap();
    assert!(journal.claim_outbox(40, 50).unwrap().is_none());
}

fn claimed_directive(journal: &mut SqliteJournal, seed: u8) -> OrchestratorDirectiveClaim {
    let genesis_bytes = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../orchestration/peritus-orchestrator/tests/fixtures/v1/orchestrator-command.bin",
    ))
    .unwrap();
    let genesis =
        decode_message::<OrchestratorCommandFrame>(&genesis_bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_command();
    let genesis_transition = peritus_orchestrator::start(&genesis).unwrap();
    commit_orchestrator_transition(journal, &genesis, &genesis_transition).unwrap();

    let state = genesis_transition.state();
    let handoff = state.open_handoff().expect("genesis writer handoff");
    let command_id = CommandId::new(identity(seed)).unwrap();
    let event_id = EventId::new(identity(seed.wrapping_add(1))).unwrap();
    let directive_id = DirectiveId::new(identity(seed.wrapping_add(2))).unwrap();
    let directive = PendingDirective::new(
        directive_id,
        DirectiveDestination::Collaboration,
        DirectiveKind::StartWriter,
        directive_payload_digest(
            DirectiveKind::StartWriter,
            DirectiveDestination::Collaboration,
            DirectivePayloadBinding::Handoff(handoff),
        )
        .unwrap(),
        3,
        event_id,
        Some(handoff.task_id()),
        Some(handoff.work_id()),
        state.current_candidate().revision(),
    )
    .unwrap();
    let publish = OrchestratorCommand::new(
        command_id,
        event_id,
        state.binding().run_id(),
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.current_candidate().revision(),
        OrchestratorCommandKind::PublishDirective { directive },
    )
    .unwrap();
    let published = peritus_orchestrator::decide(state, &publish).unwrap();
    commit_orchestrator_transition(journal, &publish, &published).unwrap();
    let message = journal.claim_outbox(10, 20).unwrap().expect("published E0 outbox row");
    orchestrator_claim(&message)
}

fn orchestrator_claim(message: &peritus_journal::OutboxMessage) -> OrchestratorDirectiveClaim {
    let TypedOutboxClaim::OrchestratorCollaboration(claim) = decode_claim(message).unwrap() else {
        panic!("writer directive decoded to another destination");
    };
    claim
}

const fn identity(value: u8) -> [u8; 16] {
    [value; 16]
}
