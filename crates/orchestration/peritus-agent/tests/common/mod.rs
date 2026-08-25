#![allow(dead_code, reason = "shared fixture helpers are used by separate integration test crates")]

use peritus_agent::{
    AgentBinding, AgentCommand, AgentCommandKind, AgentEvent, AgentLimits, AgentTurnState,
    CompletionProposal, CompletionRequest, ContextRecord, ModelCallId, ModelTerminalRecord,
    ProfileRevision, SafeText, ToolIdempotency, ToolOrdinal, ToolProposal, ToolResultRecord,
    ToolResultStatus, ToolSideEffect, ToolVersion, TranscriptDigests, reduce, start,
};
use peritus_policy::{ActorRole, AuthorityInstant};
use peritus_role::RoleProfile;
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, AttemptId, CapabilityName, CommandId, EnvironmentId,
    EventId, Generation, HarnessId, IdentifierError, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, SessionId, Sha256Digest, TurnId, WorkspaceId,
};

pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
pub fn id16<T>(value: u8, make: impl FnOnce([u8; 16]) -> Result<T, IdentifierError>) -> T {
    make([value; 16]).expect("nonzero test identifier")
}

pub fn revision() -> RevisionTuple {
    RevisionTuple::new(
        id16(1, AcceptanceSpecId::new),
        id16(2, HarnessId::new),
        id16(3, WorkspaceId::new),
        Generation::new(1).expect("generation"),
        RevisionNumber::new(1).expect("revision"),
        id16(4, PolicyId::new),
        id16(5, ProviderProfileId::new),
    )
}

pub fn binding() -> AgentBinding {
    let role = ActorRole::Writer;
    AgentBinding::new(
        id16(6, TurnId::new),
        id16(7, AttemptId::new),
        id16(8, ActorId::new),
        role,
        RoleProfile::for_actor_role(role),
        id16(9, SessionId::new),
        id16(10, EnvironmentId::new),
        revision(),
        id16(5, ProviderProfileId::new),
        ProfileRevision::new(1).expect("profile revision"),
        RevisionNumber::new(1).expect("limits revision"),
    )
    .expect("binding")
}

pub fn limits(transitions: u32) -> AgentLimits {
    AgentLimits::new(16, 32, 8, 4096, 4096, 4, transitions).expect("limits")
}

pub fn started(transitions: u32) -> (Vec<AgentEvent>, AgentTurnState) {
    let transition =
        start(binding(), limits(transitions), id16(11, CommandId::new), id16(12, EventId::new))
            .expect("start");
    let (event, state) = transition.into_parts();
    (vec![event], state)
}

pub fn apply(state: &mut AgentTurnState, events: &mut Vec<AgentEvent>, kind: AgentCommandKind) {
    let offset = u8::try_from(state.sequence().get()).expect("short test sequence");
    let command = AgentCommand::new(
        id16(30 + offset, CommandId::new),
        id16(90 + offset, EventId::new),
        state.logical_revision(),
        state.state_digest(),
        kind,
    );
    let transition = reduce(state, &command).expect("accepted command");
    let (event, successor) = transition.into_parts();
    events.push(event);
    *state = successor;
}

pub const fn context() -> ContextRecord {
    ContextRecord::new(digest(20), digest(21), digest(22), digest(23), None)
}

pub fn start_model(state: &mut AgentTurnState, events: &mut Vec<AgentEvent>) {
    apply(state, events, AgentCommandKind::ContextPrepared(context()));
    apply(
        state,
        events,
        AgentCommandKind::ModelRequestStarted {
            call_id: ModelCallId::new(digest(24)).expect("model call"),
            request_digest: digest(25),
        },
    );
}

pub const fn terminal() -> ModelTerminalRecord {
    ModelTerminalRecord::new(digest(26), true, false, true)
}

pub fn proposal() -> CompletionProposal {
    CompletionProposal::new(
        SafeText::new("bounded work summary".to_owned()).expect("summary"),
        Vec::new(),
        vec![SafeText::new("gate results remain external".to_owned()).expect("uncertainty")],
        revision(),
        TranscriptDigests::new(digest(20), digest(26), digest(27)),
        CompletionRequest::RunGates,
    )
    .expect("proposal")
}

pub fn tool(ordinal: u16, side_effect: ToolSideEffect) -> ToolProposal {
    ToolProposal::new(
        ToolOrdinal::new(ordinal),
        ModelCallId::new(digest(40 + u8::try_from(ordinal).expect("ordinal"))).expect("call"),
        id16(50 + u8::try_from(ordinal).expect("ordinal"), ActionId::new),
        CapabilityName::new(format!("workspace.tool-{ordinal}")).expect("capability"),
        ToolVersion::new(1, 0).expect("version"),
        digest(51),
        digest(52),
        digest(53 + u8::try_from(ordinal).expect("ordinal")),
        revision(),
        AuthorityInstant::new(Generation::new(1).expect("generation"), 10_000),
        side_effect,
        ToolIdempotency::ReplayTerminalOnly,
    )
}

pub fn result(status: ToolResultStatus, value: u8) -> ToolResultRecord {
    ToolResultRecord::new(status, digest(value), 10, Vec::new()).expect("tool result")
}
