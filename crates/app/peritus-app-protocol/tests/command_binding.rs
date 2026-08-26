//! Exact B3 command-binding integration tests.

mod support;

use peritus_app_protocol::{
    AppErrorCode, AppProtocolLimits, CommandBinding, CommandSubmissionFrames, CorrelationId,
    IdempotencyKey, RequestId,
};
use peritus_codec::sha256;
use peritus_protocol::schema::generated_binary_artifacts;
use peritus_types::{ActorId, SessionId};
use support::{command_fixture_bytes, fixture_id, revision};

#[test]
fn binding_preserves_exact_b3_frames_and_rejects_revision_drift() {
    let (envelope_bytes, command_bytes) = command_fixture_bytes();
    let frames = CommandSubmissionFrames::parse(
        envelope_bytes.clone(),
        command_bytes.clone(),
        AppProtocolLimits::PRODUCTION,
    )
    .expect("registered fixture frames parse");

    assert_eq!(frames.envelope_frame().bytes(), envelope_bytes);
    assert_eq!(frames.envelope_frame().digest(), sha256(&envelope_bytes));
    assert_eq!(frames.command_frame().bytes(), command_bytes);
    assert_eq!(frames.command_frame().digest(), sha256(&command_bytes));
    assert_eq!(frames.envelope().as_domain().revision(), revision(5));

    let binding = CommandBinding::new(
        fixture_id(40, ActorId::new),
        fixture_id(41, SessionId::new),
        fixture_id(42, RequestId::new),
        fixture_id(43, CorrelationId::new),
        IdempotencyKey::new(b"retry-key".to_vec()).expect("bounded key"),
        Some(revision(5)),
        frames.clone(),
    )
    .expect("matching revision binds");
    assert_eq!(binding.frames().envelope_frame().bytes(), envelope_bytes);
    assert_eq!(binding.frames().command_frame().bytes(), command_bytes);
    assert_eq!(binding.expected_revision(), Some(revision(5)));

    let drift = CommandBinding::new(
        fixture_id(40, ActorId::new),
        fixture_id(41, SessionId::new),
        fixture_id(42, RequestId::new),
        fixture_id(43, CorrelationId::new),
        IdempotencyKey::new(b"retry-key".to_vec()).expect("bounded key"),
        Some(revision(6)),
        frames,
    )
    .expect_err("outer revision drift is rejected");
    assert_eq!(drift.code(), AppErrorCode::CommandBindingMismatch);

    let artifacts = generated_binary_artifacts().expect("B3 fixtures encode");
    let non_command = artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with("budget-amounts.bin"))
        .expect("non-command fixture exists")
        .content
        .clone();
    let wrong_role =
        CommandSubmissionFrames::parse(envelope_bytes, non_command, AppProtocolLimits::PRODUCTION)
            .expect_err("a registered record is not a command");
    assert_eq!(wrong_role.code(), AppErrorCode::InvalidCommandFrame);
}
