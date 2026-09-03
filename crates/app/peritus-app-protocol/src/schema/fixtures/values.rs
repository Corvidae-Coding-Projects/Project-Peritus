//! Checked source values for valid compatibility frames.

use super::{FixtureClass, GeneratedFixtureCase};
use crate::{
    AppDiagnostic, AppEventEnvelope, AppEventPayload, AppProtocolLimits, AppRequestEnvelope,
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ApprovalChallenge,
    ArtifactMetadata, CanonicalMediaType, ClientHello, CommandBinding, CommandSubmissionFrames,
    ControlEnvelope, ControlPayload, CorrelationId, EventCursor, HeartbeatId, HeartbeatReply,
    IdempotencyKey, ImplementationMetadata, NegotiatedProtocol, NegotiationOutcome,
    ProductDeliverable, ProductProviderSelection, ProductRunPhase, ProductRunSettlementSnapshot,
    ProductRunSnapshot, PromptBinding, PromptConstraint, PromptCorrelation, PromptId,
    ProtocolContext, ProtocolFeatureName, ProtocolFeatureSet, ProtocolId, ProtocolVersion,
    RequestId, ServerHello, ShutdownProgress, ShutdownRequest, SubscriptionFilter, SubscriptionId,
    SubscriptionRequest, TerminalAttachmentId, TerminalBinding, TerminalOutput, TerminalStream,
    TransferId, VersionRange, WellKnownProtocolFeature,
};
use peritus_codec::{CanonicalEncode, CodecError, CodecLimits, encode_message};
use peritus_protocol::schema::generated_binary_artifacts;
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceStatus, SettlementCause,
    SettlementReducer,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, ArtifactId, CommandId, Generation, HarnessId, PolicyId, ProcessId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, SessionId, Sha256Digest, WorkspaceId,
};

pub(super) fn generated_valid_cases(
    codec_limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let limits = AppProtocolLimits::PRODUCTION;
    Ok(vec![
        encoded(
            "minimal-client-hello",
            FixtureClass::Minimal,
            &client_hello(limits),
            codec_limits,
        )?,
        encoded(
            "minimal-daemon-status-request",
            FixtureClass::Minimal,
            &request(AppRequestPayload::DaemonStatus),
            codec_limits,
        )?,
        encoded(
            "minimal-diagnostic-event",
            FixtureClass::Minimal,
            &event(AppEventPayload::Diagnostic(
                AppDiagnostic::new("ready".to_owned(), limits.max_diagnostic_bytes())
                    .expect("fixture diagnostic"),
            )),
            codec_limits,
        )?,
        encoded(
            "minimal-heartbeat-control",
            FixtureClass::Minimal,
            &heartbeat_control(),
            codec_limits,
        )?,
        encoded(
            "realistic-server-hello",
            FixtureClass::Realistic,
            &server_hello(limits),
            codec_limits,
        )?,
        encoded(
            "realistic-command-request",
            FixtureClass::Realistic,
            &command_request(limits),
            codec_limits,
        )?,
        encoded(
            "realistic-subscription-request",
            FixtureClass::Realistic,
            &subscription_request(limits),
            codec_limits,
        )?,
        encoded(
            "realistic-artifact-event",
            FixtureClass::Realistic,
            &artifact_event(limits),
            codec_limits,
        )?,
        encoded(
            "realistic-prompt-event",
            FixtureClass::Realistic,
            &prompt_event(limits),
            codec_limits,
        )?,
        encoded(
            "realistic-terminal-event",
            FixtureClass::Realistic,
            &terminal_event(limits),
            codec_limits,
        )?,
        encoded(
            "realistic-daemon-response",
            FixtureClass::Realistic,
            &daemon_response(limits),
            codec_limits,
        )?,
        encoded(
            "realistic-product-run-settlement-response",
            FixtureClass::Realistic,
            &product_run_settlement_response(),
            codec_limits,
        )?,
        encoded(
            "realistic-shutdown-event",
            FixtureClass::Realistic,
            &shutdown_event(limits),
            codec_limits,
        )?,
    ])
}

fn encoded<T: CanonicalEncode>(
    case: &'static str,
    class: FixtureClass,
    value: &T,
    limits: CodecLimits,
) -> Result<GeneratedFixtureCase, CodecError> {
    Ok(GeneratedFixtureCase {
        case,
        class,
        payload: encode_message(value, limits)?,
        expected_family: Some(T::FAMILY),
        accepted: true,
        expected_error: None,
    })
}

fn client_hello(limits: AppProtocolLimits) -> ClientHello {
    ClientHello::new(
        id(1, ProtocolId::new),
        vec![VersionRange::new(1, 0, 0).expect("fixture range")],
        Vec::new(),
        Vec::new(),
        limits,
        "peritus-fixture-client".to_owned(),
    )
    .expect("fixture hello")
}

fn server_hello(limits: AppProtocolLimits) -> ServerHello {
    let feature = ProtocolFeatureName::well_known(WellKnownProtocolFeature::EventSubscriptions)
        .expect("fixture feature");
    let features =
        ProtocolFeatureSet::new(vec![feature], limits.max_features()).expect("fixture feature set");
    ServerHello::new(
        id(1, ProtocolId::new),
        ImplementationMetadata::new("peritus-fixture-daemon".to_owned(), limits)
            .expect("fixture implementation"),
        Some(id(9, SessionId::new)),
        NegotiationOutcome::Compatible(NegotiatedProtocol::new(
            ProtocolVersion::new(1, 0).expect("fixture version"),
            features,
            limits,
        )),
    )
    .expect("fixture server hello")
}

fn heartbeat_control() -> ControlEnvelope {
    ControlEnvelope::new(
        context(),
        id(13, CorrelationId::new),
        ControlPayload::HeartbeatReply(HeartbeatReply::new(id(14, HeartbeatId::new), 0)),
    )
}

fn request(payload: AppRequestPayload) -> AppRequestEnvelope {
    AppRequestEnvelope::new(context(), id(10, RequestId::new), id(11, CorrelationId::new), payload)
        .expect("fixture request")
}

fn command_request(limits: AppProtocolLimits) -> AppRequestEnvelope {
    let artifacts = generated_binary_artifacts().expect("B3 fixture generation");
    let frames = CommandSubmissionFrames::parse(
        fixture_bytes(&artifacts, "command-envelope.bin"),
        fixture_bytes(&artifacts, "kernel-command-pause-session.bin"),
        limits,
    )
    .expect("B3 frames");
    let binding = CommandBinding::new(
        id(12, ActorId::new),
        context().session_id(),
        id(10, RequestId::new),
        id(11, CorrelationId::new),
        IdempotencyKey::new(b"fixture-command".to_vec()).expect("fixture key"),
        Some(revision()),
        frames,
    )
    .expect("fixture command binding");
    request(AppRequestPayload::SubmitCommand(binding))
}

fn subscription_request(limits: AppProtocolLimits) -> AppRequestEnvelope {
    let subscription = SubscriptionRequest::new(
        id(20, SubscriptionId::new),
        SubscriptionFilter::new(
            vec!["agent.events".to_owned(), "run.events".to_owned()],
            limits.max_topics(),
            limits.codec().max_string_bytes,
        )
        .expect("fixture topics"),
        EventCursor::new(41),
        8,
        true,
    )
    .expect("fixture subscription");
    request(AppRequestPayload::Subscribe(subscription))
}

fn event(payload: AppEventPayload) -> AppEventEnvelope {
    AppEventEnvelope::new(context(), payload)
}

fn artifact_event(limits: AppProtocolLimits) -> AppEventEnvelope {
    let metadata = ArtifactMetadata::new(
        id(21, TransferId::new),
        id(22, ArtifactId::new),
        4096,
        CanonicalMediaType::new("application/json".to_owned(), limits.codec().max_string_bytes)
            .expect("fixture media type"),
        Sha256Digest::new([23; 32]),
        1024,
        limits.max_artifact_chunk_bytes(),
    )
    .expect("fixture artifact metadata");
    event(AppEventPayload::ArtifactMetadata(metadata))
}

fn prompt_event(limits: AppProtocolLimits) -> AppEventEnvelope {
    let correlation = PromptCorrelation::new(
        id(10, RequestId::new),
        id(24, PromptId::new),
        context().session_id(),
        id(12, ActorId::new),
        revision(),
        Sha256Digest::new([25; 32]),
        Generation::new(1).expect("fixture generation"),
    );
    let challenge = ApprovalChallenge::new(
        id(28, CommandId::new),
        RevisionNumber::first(),
        vec![0x50, 0x52, 0x54, 0x53],
        limits.codec().max_opaque_bytes,
    )
    .expect("fixture approval challenge");
    let prompt = PromptBinding::approval(
        correlation,
        challenge,
        vec![PromptConstraint::NonEmpty],
        limits.codec().max_collection_items,
    )
    .expect("fixture prompt");
    event(AppEventPayload::PromptRequested(prompt))
}

fn terminal_event(limits: AppProtocolLimits) -> AppEventEnvelope {
    let binding = TerminalBinding::new(
        id(26, TerminalAttachmentId::new),
        id(27, ProcessId::new),
        id(10, RequestId::new),
    );
    let output = TerminalOutput::new(
        binding,
        0,
        0,
        TerminalStream::Stdout,
        b"peritus ready\n".to_vec(),
        limits.max_terminal_chunk_bytes(),
    )
    .expect("fixture terminal output");
    event(AppEventPayload::TerminalOutput(output))
}

fn daemon_response(limits: AppProtocolLimits) -> AppResponseEnvelope {
    AppResponseEnvelope::new(
        context(),
        id(10, RequestId::new),
        id(11, CorrelationId::new),
        AppResponsePayload::DaemonStatus(
            crate::DaemonStatus::new(
                crate::DaemonReadiness::ReadyReadWrite,
                None,
                limits.max_diagnostic_bytes(),
            )
            .expect("fixture daemon status"),
        ),
    )
}

fn product_run_settlement_response() -> AppResponseEnvelope {
    let run_id = id(31, RunId::new);
    let workspace_id = id(32, WorkspaceId::new);
    let providers = ProductProviderSelection::new(
        id(33, ProviderProfileId::new),
        id(34, ProviderProfileId::new),
        id(35, ProviderProfileId::new),
    );
    let deliverable = ProductDeliverable::candidate(
        "/worktrees/fixture".to_owned(),
        vec!["src/lib.rs".to_owned()],
        Vec::new(),
        "cargo test".to_owned(),
        CandidateStage::Changed,
    )
    .expect("fixture candidate deliverable");
    let snapshot = ProductRunSnapshot::new(
        run_id,
        workspace_id,
        providers,
        ProductRunPhase::Failed,
        1,
        "implement the requested change".to_owned(),
        "provider ended after writing a candidate".to_owned(),
        "src/lib.rs changed".to_owned(),
        String::new(),
        String::new(),
        "candidate is preserved for continuation".to_owned(),
    )
    .expect("fixture product snapshot")
    .with_deliverable(deliverable);
    let checkpoint = CandidateCheckpoint::new(
        CandidateIdentity::new(run_id, workspace_id, Sha256Digest::new([36; 32]), 4, 2)
            .expect("fixture candidate identity"),
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("fixture candidate checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("fixture candidate observation");
    let settlement = reducer.settle(SettlementCause::Provider).expect("fixture settlement");
    let settled = ProductRunSettlementSnapshot::new(snapshot, settlement)
        .expect("fixture product settlement snapshot");
    AppResponseEnvelope::new(
        context(),
        id(10, RequestId::new),
        id(11, CorrelationId::new),
        AppResponsePayload::ProductRunSettled(settled),
    )
}

fn shutdown_event(limits: AppProtocolLimits) -> AppEventEnvelope {
    let request = ShutdownRequest::new(id(10, RequestId::new), id(11, CorrelationId::new));
    let progress =
        ShutdownProgress::new(request, 2, 3, Vec::new(), limits.max_remaining_work_items())
            .expect("fixture shutdown progress");
    event(AppEventPayload::ShutdownProgress(progress))
}

fn context() -> ProtocolContext {
    ProtocolContext::new(
        id(1, ProtocolId::new),
        ProtocolVersion::new(1, 0).expect("fixture version"),
        id(2, SessionId::new),
    )
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        id(1, AcceptanceSpecId::new),
        id(2, HarnessId::new),
        id(3, WorkspaceId::new),
        Generation::new(4).expect("fixture generation"),
        RevisionNumber::new(5).expect("fixture revision"),
        id(6, PolicyId::new),
        id(7, ProviderProfileId::new),
    )
}

fn fixture_bytes(
    artifacts: &[peritus_protocol::schema::GeneratedBinaryArtifact],
    suffix: &str,
) -> Vec<u8> {
    artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with(suffix))
        .expect("named B3 fixture")
        .content
        .clone()
}

fn id<T, E: core::fmt::Debug>(byte: u8, constructor: impl FnOnce([u8; 16]) -> Result<T, E>) -> T {
    constructor([byte; 16]).expect("fixture identity is nonzero")
}
