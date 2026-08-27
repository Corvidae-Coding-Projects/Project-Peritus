//! Native D3 retry, restart, finalization, and stale-fence coverage.

use peritus_collaboration::{
    CollaborationBinding, CollaborationCommand, CollaborationCommandKind, CollaborationId,
    CollaborationLimits, CollaborationPhase, CollaborationTaskId, Delegation, JoinPolicy,
    load_collaboration_replay,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_role::HarnessRole;
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding,
    SchedulerCommand, SchedulerCommandKind, SchedulerId, SchedulerLimits, SchedulerPhase, WorkId,
    load_scheduler_replay,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use super::super::children::{commit_collaboration_directive, commit_scheduler_directive};
use crate::{DaemonErrorCode, DaemonRecovery};

#[test]
fn scheduler_lifecycle_retries_exactly_across_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler-child-retry.sqlite3");
    let store_id = StoreId::new(identity(101)).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let genesis = scheduler_genesis();
    let started = peritus_scheduler::start(&genesis).unwrap();
    peritus_scheduler::commit_scheduler_transition(&mut journal, &genesis, &started).unwrap();
    let active = started.into_state();
    let pause = scheduler_command(&active, 102, 103, SchedulerCommandKind::PauseScheduler);

    commit_scheduler_directive(&mut journal, &pause).unwrap();
    drop(journal);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_scheduler_directive(&mut restarted, &pause).unwrap();
    let paused =
        load_scheduler_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap().unwrap();
    assert_eq!(paused.phase(), SchedulerPhase::Paused);
    assert_eq!(paused.sequence().get(), active.sequence().get() + 1);

    let resume = scheduler_command(&paused, 104, 105, SchedulerCommandKind::ResumeScheduler);
    commit_scheduler_directive(&mut restarted, &resume).unwrap();
    drop(restarted);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_scheduler_directive(&mut restarted, &resume).unwrap();
    let resumed =
        load_scheduler_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap().unwrap();
    assert_eq!(resumed.phase(), SchedulerPhase::Active);
    assert_eq!(resumed.sequence().get(), active.sequence().get() + 2);
}

#[test]
fn scheduler_stale_fence_does_not_append_or_claim_success() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler-child-stale.sqlite3");
    let store_id = StoreId::new(identity(110)).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let genesis = scheduler_genesis();
    let started = peritus_scheduler::start(&genesis).unwrap();
    peritus_scheduler::commit_scheduler_transition(&mut journal, &genesis, &started).unwrap();
    let active = started.into_state();
    let stale = scheduler_command(&active, 111, 112, SchedulerCommandKind::PauseScheduler);
    let committed = scheduler_command(&active, 113, 114, SchedulerCommandKind::PauseScheduler);
    commit_scheduler_directive(&mut journal, &committed).unwrap();

    let error = commit_scheduler_directive(&mut journal, &stale)
        .expect_err("another native successor must keep the stale command unsettled");

    assert_eq!(error.code_kind(), DaemonErrorCode::RecoveryRequired);
    assert_eq!(error.recovery(), DaemonRecovery::Reconcile);
    let replay = load_scheduler_replay(&journal, active.run_id()).unwrap();
    assert_eq!(replay.events().len(), 2);
    assert_eq!(replay.rebuild().unwrap().unwrap().phase(), SchedulerPhase::Paused);
}

#[test]
fn collaboration_lifecycle_retries_exactly_across_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("collaboration-child-retry.sqlite3");
    let store_id = StoreId::new(identity(120)).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let genesis = collaboration_genesis();
    let started = peritus_collaboration::start(&genesis).unwrap();
    peritus_collaboration::commit_collaboration_transition(&mut journal, &genesis, &started)
        .unwrap();
    let active = started.into_state();
    let owner = active.binding().root_assignment().owner();
    let pause = collaboration_command(
        &active,
        121,
        122,
        CollaborationCommandKind::Pause { requested_by: owner },
    );

    commit_collaboration_directive(&mut journal, &pause).unwrap();
    drop(journal);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_collaboration_directive(&mut restarted, &pause).unwrap();
    let paused =
        load_collaboration_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap().unwrap();
    assert_eq!(paused.phase(), CollaborationPhase::Paused);

    let resume = collaboration_command(
        &paused,
        123,
        124,
        CollaborationCommandKind::Resume { requested_by: owner },
    );
    commit_collaboration_directive(&mut restarted, &resume).unwrap();
    drop(restarted);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_collaboration_directive(&mut restarted, &resume).unwrap();
    let resumed =
        load_collaboration_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap().unwrap();
    assert_eq!(resumed.phase(), CollaborationPhase::Active);
    assert_eq!(resumed.sequence().get(), active.sequence().get() + 2);
}

#[test]
fn collaboration_cancel_and_finalize_retry_without_manufacturing_success() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("collaboration-child-finalize.sqlite3");
    let store_id = StoreId::new(identity(130)).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let genesis = collaboration_genesis();
    let started = peritus_collaboration::start(&genesis).unwrap();
    peritus_collaboration::commit_collaboration_transition(&mut journal, &genesis, &started)
        .unwrap();
    let active = started.into_state();
    let root = active.binding().root_assignment();
    let cancel = collaboration_command(
        &active,
        131,
        132,
        CollaborationCommandKind::CancelTask {
            task_id: root.task_id(),
            requested_by: root.owner(),
            reason_digest: digest(133),
        },
    );
    commit_collaboration_directive(&mut journal, &cancel).unwrap();
    drop(journal);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_collaboration_directive(&mut restarted, &cancel).unwrap();
    let cancelled =
        load_collaboration_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap().unwrap();
    let finalize = collaboration_command(&cancelled, 134, 135, CollaborationCommandKind::Finalize);
    commit_collaboration_directive(&mut restarted, &finalize).unwrap();
    drop(restarted);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    commit_collaboration_directive(&mut restarted, &finalize).unwrap();
    let terminal =
        load_collaboration_replay(&restarted, active.run_id()).unwrap().rebuild().unwrap().unwrap();
    assert_eq!(terminal.phase(), CollaborationPhase::Terminal);
    assert_eq!(
        terminal.terminal().unwrap().kind(),
        peritus_collaboration::CollaborationTerminalKind::Cancelled,
    );
}

fn scheduler_genesis() -> SchedulerCommand {
    let limits = SchedulerLimits::new(8, 16, 4, 4, 4, 4, 3, 2, 2, 65_536, 262_144).unwrap();
    let capacity = ResourceVector::new(
        vec![ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(4).unwrap())],
        limits.resource_dimensions(),
    )
    .unwrap();
    let binding = SchedulerBinding::new(
        RunId::new(identity(140)).unwrap(),
        SchedulerId::new(identity(141)).unwrap(),
        revision(),
        limits,
        capacity,
    )
    .unwrap();
    SchedulerCommand::new(
        CommandId::new(identity(142)).unwrap(),
        EventId::new(identity(143)).unwrap(),
        binding.run_id(),
        0,
        None,
        digest(0),
        binding.revision(),
        SchedulerCommandKind::StartScheduler { binding },
    )
    .unwrap()
}

fn scheduler_command(
    state: &peritus_scheduler::SchedulerState,
    command_seed: u8,
    event_seed: u8,
    kind: SchedulerCommandKind,
) -> SchedulerCommand {
    SchedulerCommand::new(
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

fn collaboration_genesis() -> CollaborationCommand {
    let root_id = CollaborationTaskId::new(identity(150)).unwrap();
    let owner = ActorId::new(identity(151)).unwrap();
    let root = Delegation::root(
        root_id,
        owner,
        HarnessRole::Writer,
        WorkId::new(identity(152)).unwrap(),
        digest(153),
        JoinPolicy::NoChildren,
    )
    .unwrap();
    let binding = CollaborationBinding::new(
        CollaborationId::new(identity(154)).unwrap(),
        RunId::new(identity(155)).unwrap(),
        revision(),
        SchedulerId::new(identity(156)).unwrap(),
        CollaborationLimits::new(16, 4, 4, 16, 4, 4_096, 8, 65_536, 262_144).unwrap(),
        root,
    )
    .unwrap();
    CollaborationCommand::new(
        CommandId::new(identity(157)).unwrap(),
        EventId::new(identity(158)).unwrap(),
        binding.run_id(),
        0,
        None,
        digest(0),
        binding.revision(),
        CollaborationCommandKind::Start { binding },
    )
    .unwrap()
}

fn collaboration_command(
    state: &peritus_collaboration::CollaborationState,
    command_seed: u8,
    event_seed: u8,
    kind: CollaborationCommandKind,
) -> CollaborationCommand {
    CollaborationCommand::new(
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

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(identity(160)).unwrap(),
        HarnessId::new(identity(161)).unwrap(),
        WorkspaceId::new(identity(162)).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(identity(163)).unwrap(),
        ProviderProfileId::new(identity(164)).unwrap(),
    )
}

const fn identity(value: u8) -> [u8; 16] {
    [value; 16]
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
