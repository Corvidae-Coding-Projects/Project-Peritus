#![allow(dead_code, reason = "each integration-test crate uses a focused subset")]

use peritus_app_protocol::{
    AppProtocolLimits, CommandBinding, CommandSubmissionFrames, CorrelationId, IdempotencyKey,
    RequestId,
};
use peritus_protocol::schema::{generated_agent_binary_artifacts, generated_binary_artifacts};
use peritus_types::{
    AcceptanceSpecId, ActorId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, SessionId, WorkspaceId,
};

pub fn fixture_id<T, E: core::fmt::Debug>(
    byte: u8,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> T {
    constructor([byte; 16]).expect("fixture identity is nonzero")
}

pub fn revision(workspace_revision: u64) -> RevisionTuple {
    RevisionTuple::new(
        fixture_id(1, AcceptanceSpecId::new),
        fixture_id(2, HarnessId::new),
        fixture_id(3, WorkspaceId::new),
        Generation::new(4).expect("fixture generation is positive"),
        RevisionNumber::new(workspace_revision).expect("fixture revision is positive"),
        fixture_id(6, PolicyId::new),
        fixture_id(7, ProviderProfileId::new),
    )
}

pub fn command_fixture_bytes() -> (Vec<u8>, Vec<u8>) {
    let artifacts = generated_binary_artifacts().expect("B3 fixtures encode");
    let envelope = artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with("command-envelope.bin"))
        .expect("command-envelope fixture exists")
        .content
        .clone();
    let command = artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with("kernel-command-pause-session.bin"))
        .expect("kernel-command fixture exists")
        .content
        .clone();
    (envelope, command)
}

pub fn event_fixture_bytes() -> Vec<u8> {
    generated_agent_binary_artifacts()
        .expect("B3 agent fixtures encode")
        .into_iter()
        .find(|artifact| artifact.path.ends_with("agent-event.bin"))
        .expect("agent-event fixture exists")
        .content
}

pub fn command_binding(request_byte: u8, key: &[u8]) -> CommandBinding {
    let (envelope, command) = command_fixture_bytes();
    let frames = CommandSubmissionFrames::parse(envelope, command, AppProtocolLimits::PRODUCTION)
        .expect("B3 fixture frames are valid");
    CommandBinding::new(
        fixture_id(40, ActorId::new),
        fixture_id(41, SessionId::new),
        fixture_id(request_byte, RequestId::new),
        fixture_id(43, CorrelationId::new),
        IdempotencyKey::new(key.to_vec()).expect("fixture idempotency key is bounded"),
        Some(revision(5)),
        frames,
    )
    .expect("fixture command binding is valid")
}
