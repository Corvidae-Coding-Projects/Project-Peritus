use super::*;
use crate::{AgentFailure, ProfileRevision, SafeText, TerminalKind};
use peritus_policy::ActorRole;
use peritus_role::RoleProfile;
use peritus_types::{
    AcceptanceSpecId, ActorId, AttemptId, EnvironmentId, Generation, HarnessId, IdentifierError,
    PolicyId, ProviderProfileId, RevisionTuple, SessionId, TurnId, WorkspaceId,
};

#[test]
fn replay_rejects_correctly_fenced_failure_events_after_terminal_state() {
    let revision = RevisionTuple::new(
        id16(1, AcceptanceSpecId::new),
        id16(2, HarnessId::new),
        id16(3, WorkspaceId::new),
        Generation::new(1).expect("generation"),
        RevisionNumber::new(1).expect("revision"),
        id16(4, PolicyId::new),
        id16(5, ProviderProfileId::new),
    );
    let role = ActorRole::Writer;
    let binding = AgentBinding::new(
        id16(6, TurnId::new),
        id16(7, AttemptId::new),
        id16(8, ActorId::new),
        role,
        RoleProfile::for_actor_role(role),
        id16(9, SessionId::new),
        id16(10, EnvironmentId::new),
        revision,
        id16(5, ProviderProfileId::new),
        ProfileRevision::new(1).expect("profile revision"),
        RevisionNumber::new(1).expect("limits revision"),
    )
    .expect("binding");
    let limits = AgentLimits::new(16, 32, 8, 4_096, 4_096, 4, 16).expect("limits");
    let started =
        start(binding, limits, id16(11, CommandId::new), id16(12, EventId::new)).expect("start");
    let (genesis, active) = started.into_parts();
    let terminal_command = AgentCommand::new(
        id16(13, CommandId::new),
        id16(14, EventId::new),
        active.logical_revision(),
        active.state_digest(),
        AgentCommandKind::Failed(failure(AgentFailureKind::Provider, "initial failure")),
    );
    let terminal = reduce(&active, &terminal_command).expect("terminal failure");
    let (terminal_event, terminal_state) = terminal.into_parts();
    let prefix = [genesis, terminal_event];

    for (identifier, kind) in [
        (20, AgentCommandKind::Failed(failure(AgentFailureKind::Tool, "late failure"))),
        (
            30,
            AgentCommandKind::Exhausted(failure(
                AgentFailureKind::Exhausted(crate::AgentLimitDimension::ProviderEvents),
                "late exhaustion",
            )),
        ),
    ] {
        let event_id = id16(identifier, EventId::new);
        let command_id = id16(identifier + 1, CommandId::new);
        let sequence = terminal_state.sequence.checked_next().expect("next sequence");
        let successor_revision =
            terminal_state.logical_revision.checked_next().expect("next logical revision");
        let mut legacy_successor = terminal_state.clone();
        legacy_successor
            .counters
            .transition(legacy_successor.limits)
            .expect("legacy transition count");
        match &kind {
            AgentCommandKind::Failed(value) => {
                fail(&mut legacy_successor, value.clone(), false).expect("legacy failure");
            }
            AgentCommandKind::Exhausted(value) => {
                fail(&mut legacy_successor, value.clone(), true).expect("legacy exhaustion");
            }
            _ => unreachable!("test uses failure commands"),
        }
        legacy_successor.phase = AgentPhase::Terminal(TerminalKind::Failed);
        legacy_successor.sequence = sequence;
        legacy_successor.logical_revision = successor_revision;
        legacy_successor.last_event_id = event_id;
        legacy_successor.state_digest = crate::canonical::state_digest(&legacy_successor);
        let late_event = AgentEvent::new(
            event_id,
            command_id,
            sequence,
            Some(terminal_state.last_event_id),
            Some(terminal_state.phase),
            terminal_state.paused_from,
            Some(terminal_state.logical_revision),
            successor_revision,
            terminal_state.state_digest,
            legacy_successor.state_digest,
            AgentEventKind::CommandAccepted(kind),
        );
        assert_eq!(late_event.successor_state_digest(), legacy_successor.state_digest);

        let error = replay(&[prefix[0].clone(), prefix[1].clone(), late_event])
            .expect_err("post-terminal failure event must be rejected");
        assert_eq!(error.code(), AgentErrorCode::ReplayMismatch);
        assert_eq!(error.operation(), AgentOperation::Replay);
        assert_eq!(error.recovery(), AgentRecovery::RestartTurn);
        assert_eq!(error.detail(), "event payload is illegal for replayed state");
    }
}

fn failure(kind: AgentFailureKind, detail: &str) -> AgentFailure {
    AgentFailure::new(kind, SafeText::new(detail.to_owned()).expect("safe failure detail"))
}

fn id16<T>(value: u8, make: impl FnOnce([u8; 16]) -> Result<T, IdentifierError>) -> T {
    make([value; 16]).expect("nonzero test identifier")
}
