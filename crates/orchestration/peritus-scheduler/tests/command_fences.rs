//! Public command reduction enforces every exact fence and its rejection priority.

mod support;

use peritus_scheduler::{
    SchedulerCommand, SchedulerCommandKind, SchedulerError, SchedulerErrorKind,
    SchedulerRecoveryAction, decide,
};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, HarnessId, PolicyId, ProviderProfileId, RevisionTuple,
    RunId, Sha256Digest, WorkspaceId,
};

use support::{Fixture, bytes, digest};

const STALE_DETAIL: &str =
    "scheduler command run, revision, predecessor, digest, identity, or lifecycle differs";
const TERMINAL_DETAIL: &str = "scheduler aggregate is terminal and fenced closed";

fn command(
    command_id: CommandId,
    run_id: RunId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    revision: RevisionTuple,
    kind: SchedulerCommandKind,
) -> SchedulerCommand {
    SchedulerCommand::new(
        command_id,
        EventId::new(bytes(120)).expect("fixed event identity"),
        run_id,
        expected_sequence,
        expected_previous_event,
        prior_state_digest,
        revision,
        kind,
    )
    .expect("non-genesis command predecessor shape")
}

fn changed_revision(revision: RevisionTuple, component: usize) -> RevisionTuple {
    assert!(component < 7);
    RevisionTuple::new(
        if component == 0 {
            AcceptanceSpecId::new(bytes(99)).expect("fixed acceptance identity")
        } else {
            revision.acceptance_spec_id()
        },
        if component == 1 {
            HarnessId::new(bytes(99)).expect("fixed harness identity")
        } else {
            revision.harness_id()
        },
        if component == 2 {
            WorkspaceId::new(bytes(99)).expect("fixed workspace identity")
        } else {
            revision.workspace_id()
        },
        if component == 3 {
            revision.workspace_generation().checked_next().expect("fixture generation advances")
        } else {
            revision.workspace_generation()
        },
        if component == 4 {
            revision.workspace_revision().checked_next().expect("fixture revision advances")
        } else {
            revision.workspace_revision()
        },
        if component == 5 {
            PolicyId::new(bytes(99)).expect("fixed policy identity")
        } else {
            revision.policy_id()
        },
        if component == 6 {
            ProviderProfileId::new(bytes(99)).expect("fixed provider profile identity")
        } else {
            revision.provider_profile_id()
        },
    )
}

fn assert_rejection(
    result: Result<peritus_scheduler::SchedulerTransition, SchedulerError>,
    kind: SchedulerErrorKind,
    recovery: SchedulerRecoveryAction,
    detail: &str,
) {
    match result {
        Ok(_) => panic!("command unexpectedly passed its fence"),
        Err(error) => {
            assert_eq!(error.kind(), kind);
            assert_eq!(error.recovery(), recovery);
            assert_eq!(error.detail(), detail);
        }
    }
}

#[test]
fn current_unused_non_genesis_fences_are_accepted() {
    let fixture = Fixture::new();
    let (state, _) = fixture.started();
    let transition =
        decide(&state, &Fixture::command(&state, 20, SchedulerCommandKind::PauseScheduler))
            .expect("every exact fence is current and unused");
    assert_eq!(transition.state().sequence().get(), state.sequence().get() + 1);
}

#[test]
fn every_stale_fence_dimension_and_start_payload_is_rejected() {
    let fixture = Fixture::new();
    let (state, _) = fixture.started();
    let fresh_command_id = CommandId::new(bytes(20)).expect("fixed command identity");
    let correct_run = state.run_id();
    let correct_sequence = state.sequence().get();
    let correct_previous = Some(state.last_event_id());
    let correct_digest = state.state_digest();
    let correct_revision = state.binding().revision();

    let stale_commands = [
        command(
            fresh_command_id,
            RunId::new(bytes(99)).expect("fixed alternate run identity"),
            correct_sequence,
            correct_previous,
            correct_digest,
            correct_revision,
            SchedulerCommandKind::PauseScheduler,
        ),
        command(
            fresh_command_id,
            correct_run,
            correct_sequence,
            correct_previous,
            correct_digest,
            changed_revision(correct_revision, 6),
            SchedulerCommandKind::PauseScheduler,
        ),
        command(
            fresh_command_id,
            correct_run,
            correct_sequence + 1,
            correct_previous,
            correct_digest,
            correct_revision,
            SchedulerCommandKind::PauseScheduler,
        ),
        command(
            fresh_command_id,
            correct_run,
            correct_sequence,
            Some(EventId::new(bytes(99)).expect("fixed alternate predecessor")),
            correct_digest,
            correct_revision,
            SchedulerCommandKind::PauseScheduler,
        ),
        command(
            fresh_command_id,
            correct_run,
            correct_sequence,
            correct_previous,
            digest(99),
            correct_revision,
            SchedulerCommandKind::PauseScheduler,
        ),
        command(
            CommandId::new(bytes(1)).expect("genesis command identity"),
            correct_run,
            correct_sequence,
            correct_previous,
            correct_digest,
            correct_revision,
            SchedulerCommandKind::PauseScheduler,
        ),
        command(
            fresh_command_id,
            correct_run,
            correct_sequence,
            correct_previous,
            correct_digest,
            correct_revision,
            SchedulerCommandKind::StartScheduler { binding: fixture.binding },
        ),
    ];

    let before = state.clone();
    for stale in stale_commands {
        assert_rejection(
            decide(&state, &stale),
            SchedulerErrorKind::StaleFence,
            SchedulerRecoveryAction::ReplayAggregate,
            STALE_DETAIL,
        );
        assert_eq!(state, before);
    }
}

#[test]
fn terminal_state_rejection_has_priority_over_every_stale_fence() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(&mut state, &mut events, 20, SchedulerCommandKind::FinalizeScheduler);

    let conflicting = command(
        CommandId::new(bytes(1)).expect("already-used genesis command identity"),
        RunId::new(bytes(99)).expect("fixed alternate run identity"),
        state.sequence().get() + 1,
        Some(EventId::new(bytes(99)).expect("fixed alternate predecessor")),
        digest(99),
        changed_revision(state.binding().revision(), 6),
        SchedulerCommandKind::StartScheduler { binding: fixture.binding },
    );
    assert_rejection(
        decide(&state, &conflicting),
        SchedulerErrorKind::IllegalTransition,
        SchedulerRecoveryAction::CorrectInput,
        TERMINAL_DETAIL,
    );
}

#[test]
fn every_revision_component_is_fenced_independently() {
    let fixture = Fixture::new();
    let (state, _) = fixture.started();
    for component in 0..7 {
        let mismatched = command(
            CommandId::new(bytes(20)).expect("fixed command identity"),
            state.run_id(),
            state.sequence().get(),
            Some(state.last_event_id()),
            state.state_digest(),
            changed_revision(state.binding().revision(), component),
            SchedulerCommandKind::PauseScheduler,
        );
        assert_rejection(
            decide(&state, &mismatched),
            SchedulerErrorKind::StaleFence,
            SchedulerRecoveryAction::ReplayAggregate,
            STALE_DETAIL,
        );
    }
}

#[test]
fn every_identity_and_digest_byte_participates_in_fence_equality() {
    let fixture = Fixture::new();
    let (state, _) = fixture.started();
    let fresh = CommandId::new(bytes(20)).expect("fixed fresh command identity");
    for index in 0..16 {
        let mut run = *state.run_id().as_bytes();
        run[index] ^= 1;
        let mut predecessor = *state.last_event_id().as_bytes();
        predecessor[index] ^= 1;
        for (run_id, previous) in [
            (RunId::new(run).expect("single-byte alternate run"), state.last_event_id()),
            (state.run_id(), EventId::new(predecessor).expect("single-byte alternate event")),
        ] {
            let mismatched = command(
                fresh,
                run_id,
                state.sequence().get(),
                Some(previous),
                state.state_digest(),
                state.binding().revision(),
                SchedulerCommandKind::PauseScheduler,
            );
            assert_rejection(
                decide(&state, &mismatched),
                SchedulerErrorKind::StaleFence,
                SchedulerRecoveryAction::ReplayAggregate,
                STALE_DETAIL,
            );
        }
        let mut unused = *state.used_commands()[0].as_bytes();
        unused[index] ^= 1;
        let exact_new_identity = command(
            CommandId::new(unused).expect("single-byte unused command"),
            state.run_id(),
            state.sequence().get(),
            Some(state.last_event_id()),
            state.state_digest(),
            state.binding().revision(),
            SchedulerCommandKind::PauseScheduler,
        );
        assert!(decide(&state, &exact_new_identity).is_ok(), "command identity byte {index}");
    }
    for index in 0..32 {
        let mut digest = *state.state_digest().as_bytes();
        digest[index] ^= 1;
        let mismatched = command(
            fresh,
            state.run_id(),
            state.sequence().get(),
            Some(state.last_event_id()),
            Sha256Digest::new(digest),
            state.binding().revision(),
            SchedulerCommandKind::PauseScheduler,
        );
        assert_rejection(
            decide(&state, &mismatched),
            SchedulerErrorKind::StaleFence,
            SchedulerRecoveryAction::ReplayAggregate,
            STALE_DETAIL,
        );
    }
}
