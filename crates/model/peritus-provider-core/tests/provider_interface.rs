//! Provider-profile binding and owned normalized-stream contracts.

#[path = "support/runtime.rs"]
mod runtime;

use std::collections::VecDeque;

use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, CapabilityMatrix, CapabilityProvenance,
    Continuation, EventEnvelope, GenerationConfig, Message, ModelEvent, ModelLimits, ModelName,
    ModelRequest, OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits,
    ProviderName, ProviderProfile, ReasoningPolicy, RequestId, RequestOptions,
    RequestedCapabilities, ResumeKind, Role, StateMode, StructuredOutput, ToolChoice, WireDialect,
    negotiate,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ContinuationRestoreOutcome, ModelProvider, ModelStream,
    OwnedModelStream, PersistedContinuation, ProviderCoreErrorKind, ResponseCancellationOutcome,
    validate_request_profile,
};
use peritus_types::{ProviderProfileId, Sha256Digest};

struct Events {
    events: VecDeque<EventEnvelope>,
}

impl ModelStream for Events {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, peritus_provider_core::ProviderCoreError>>
    {
        Box::pin(async move { Ok(self.events.pop_front()) })
    }
}

fn terminal(sequence: u64) -> EventEnvelope {
    EventEnvelope::new(
        sequence,
        None,
        None,
        Sha256Digest::new([u8::try_from(sequence).expect("test sequence"); 32]),
        ModelEvent::ResponseCompleted,
    )
    .expect("terminal envelope")
}

fn profile(revision: u64) -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([7; 16]).expect("profile ID"),
        revision,
        ProviderName::new("test-provider".to_owned()).expect("provider"),
        ModelName::new("test-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[], &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(1_000, 100, 4, 1, 1_024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

fn request(profile: &ProviderProfile) -> ModelRequest {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(&[], &[], profile.limits()).expect("request capabilities"),
    )
    .expect("negotiation");
    let message = Message::new(
        Role::User,
        vec![peritus_model_protocol::ContentBlock::Text(
            BoundedText::new("hello".to_owned(), limits).expect("text"),
        )],
        limits,
    )
    .expect("message");
    let options = RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(32, Vec::new(), None, None, None).expect("generation"),
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        Vec::new(),
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("request-secret".to_owned()).expect("request ID"),
        vec![message],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        options,
        limits,
    )
    .expect("request")
}

#[test]
fn owned_stream_requires_and_remembers_a_normalized_terminal() {
    runtime::block_on(async {
        let cancellation = CancellationToken::new();
        let mut stream = OwnedModelStream::new(
            Events { events: VecDeque::from([terminal(1)]) },
            cancellation.clone(),
        );
        assert!(stream.pull().await.expect("event").is_some());
        assert!(stream.terminal_observed());
        assert!(stream.pull().await.expect("post-terminal EOF").is_none());
        drop(stream);
        assert!(!cancellation.is_cancelled());
    });
}

#[test]
fn early_eof_fails_closed_and_drop_cancels_live_work() {
    runtime::block_on(async {
        let cancellation = CancellationToken::new();
        let mut early =
            OwnedModelStream::new(Events { events: VecDeque::new() }, cancellation.clone());
        let error = early.pull().await.expect_err("early EOF must fail");
        assert_eq!(error.kind(), ProviderCoreErrorKind::MalformedStream);
        assert!(cancellation.is_cancelled());

        let live_cancellation = CancellationToken::new();
        let live = OwnedModelStream::new(
            Events { events: VecDeque::from([terminal(1)]) },
            live_cancellation.clone(),
        );
        drop(live);
        assert!(live_cancellation.is_cancelled());
    });
}

#[test]
fn adapter_boundary_rejects_profile_revision_drift() {
    let first = profile(1);
    let request = request(&first);
    validate_request_profile(&first, &request).expect("exact profile binding");

    let revised = profile(2);
    let error = validate_request_profile(&revised, &request).expect_err("revision drift");
    assert_eq!(error.kind(), ProviderCoreErrorKind::InvalidRequest);
    assert!(!format!("{error:?}").contains("request-secret"));
}

struct DefaultCancellationProvider(ProviderProfile);

impl ModelProvider for DefaultCancellationProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.0
    }

    fn start(
        &self,
        _request: ModelRequest,
        _cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, peritus_provider_core::ProviderCoreError>> {
        Box::pin(async {
            Err(peritus_provider_core::ProviderCoreError::configuration(
                "test_provider",
                "start is unused by this test",
            ))
        })
    }
}

#[test]
fn provider_default_response_cancellation_performs_no_effect() {
    runtime::block_on(async {
        let provider = DefaultCancellationProvider(profile(1));
        let response = peritus_model_protocol::ResponseId::new("response-1".to_owned())
            .expect("response identity");
        let cancellation = CancellationToken::new();
        assert_eq!(
            provider.cancel_response(&response, &cancellation).await.expect("outcome"),
            ResponseCancellationOutcome::Unsupported
        );
        assert!(!cancellation.is_cancelled());
    });
}

#[test]
fn provider_default_continuation_restore_is_explicitly_unsupported() {
    runtime::block_on(async {
        let profile = profile(1);
        let provider = DefaultCancellationProvider(profile.clone());
        let continuation = Continuation::new(
            peritus_model_protocol::ResponseId::new("response-1".to_owned()).expect("response"),
            None,
            Some(7),
        )
        .expect("continuation");
        let persisted = PersistedContinuation::new(profile.profile_id(), 1, continuation)
            .expect("persisted continuation");
        assert_eq!(
            provider.restore_continuation(&persisted).await.expect("outcome"),
            ContinuationRestoreOutcome::Unsupported
        );
    });
}
