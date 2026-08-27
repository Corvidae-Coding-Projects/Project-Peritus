//! Registered B3 scheduler command fixture for runtime integration tests.

use peritus_app_protocol::{
    AppProtocolLimits, CommandBinding, CommandSubmissionFrames, CorrelationId, IdempotencyKey,
    RequestId,
};
use peritus_codec::{CodecLimits, encode_message};
use peritus_kernel::CommandEnvelope;
use peritus_protocol::CommandEnvelopeDto;
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding,
    SchedulerCommand, SchedulerCommandFrame, SchedulerCommandKind, SchedulerId, SchedulerLimits,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, SessionId, Sha256Digest, WorkspaceId,
};

pub(super) fn command_binding(
    session: SessionId,
    request_identity: u8,
    key: &[u8],
) -> CommandBinding {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([10; 16]).expect("acceptance identity"),
        HarnessId::new([11; 16]).expect("harness identity"),
        WorkspaceId::new([12; 16]).expect("workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([13; 16]).expect("policy identity"),
        ProviderProfileId::new([14; 16]).expect("provider identity"),
    );
    let limits = SchedulerLimits::new(128, 512, 16, 16, 8, 16, 4, 2, 8, 1_048_576, 4_194_304)
        .expect("scheduler limits");
    let resources = ResourceVector::new(
        vec![
            ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(8).expect("CPU capacity")),
            ResourceEntry::new(
                ResourceKind::MEMORY_BYTES,
                ResourceQuantity::new(8_192).expect("memory capacity"),
            ),
        ],
        8,
    )
    .expect("resource capacity");
    let run_id = RunId::new([15; 16]).expect("run identity");
    let scheduler = SchedulerBinding::new(
        run_id,
        SchedulerId::new([16; 16]).expect("scheduler identity"),
        revision,
        limits,
        resources,
    )
    .expect("scheduler binding");
    let command_id = CommandId::new([17; 16]).expect("command identity");
    let event_id = EventId::new([18; 16]).expect("event identity");
    let command = SchedulerCommand::new(
        command_id,
        event_id,
        run_id,
        0,
        None,
        Sha256Digest::new([0; 32]),
        revision,
        SchedulerCommandKind::StartScheduler { binding: scheduler },
    )
    .expect("scheduler genesis command");
    let envelope = CommandEnvelope::new(command_id, event_id, None, revision);
    let envelope_bytes =
        encode_message(&CommandEnvelopeDto::from(envelope), CodecLimits::PRODUCTION)
            .expect("command envelope frame");
    let command_bytes =
        encode_message(&SchedulerCommandFrame::from_command(&command), CodecLimits::PRODUCTION)
            .expect("scheduler command frame");
    let submission = CommandSubmissionFrames::parse(
        envelope_bytes,
        command_bytes,
        AppProtocolLimits::PRODUCTION,
    )
    .expect("scheduler submission frames");
    CommandBinding::new(
        ActorId::new([0x22; 16]).expect("configured actor"),
        session,
        RequestId::new([request_identity; 16]).expect("request identity"),
        CorrelationId::new([request_identity.wrapping_add(64); 16]).expect("correlation identity"),
        IdempotencyKey::new(key.to_vec()).expect("idempotency key"),
        Some(revision),
        submission,
    )
    .expect("scheduler application binding")
}
