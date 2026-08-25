//! C5 session integration and durable acknowledgement ordering.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::Mutex;

use peritus_agent::{ModelAdvance, ModelDriveError, ModelSession};
use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, CapabilityMatrix, CapabilityProvenance,
    EventEnvelope, FinishReason, GenerationConfig, ItemId, ItemKind, Message, ModelEvent,
    ModelLimits, ModelName, ModelRequest, OutputLimitEnforcement, ParallelToolPolicy,
    PersistencePolicy, ProtocolLimits, ProviderName, ProviderProfile, ReasoningPolicy, RequestId,
    RequestOptions, RequestedCapabilities, ResumeKind, Role, StateMode, StreamFragment,
    StructuredOutput, ToolChoice, WireDialect, negotiate,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{ProviderProfileId, Sha256Digest};

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

struct ScriptedStream {
    events: VecDeque<EventEnvelope>,
}

impl ModelStream for ScriptedStream {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move { Ok(self.events.pop_front()) })
    }
}

struct FakeProvider {
    profile: ProviderProfile,
    events: Mutex<Option<VecDeque<EventEnvelope>>>,
}

impl ModelProvider for FakeProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            let events = self
                .events
                .lock()
                .map_err(|_| ProviderCoreError::configuration("fake_provider", "lock failed"))?
                .take()
                .ok_or_else(|| {
                    ProviderCoreError::configuration("fake_provider", "script already consumed")
                })?;
            Ok(OwnedModelStream::new(ScriptedStream { events }, cancellation))
        })
    }
}

#[test]
fn each_envelope_waits_for_durable_ack_before_reduction_or_next_pull() {
    block_on(async {
        let limits = ProtocolLimits::PRODUCTION;
        let profile = profile();
        let item = ItemId::new("message-1".to_owned()).expect("item");
        let events = VecDeque::from([
            envelope(1, ModelEvent::ResponseStarted { response_id: None, model: None }),
            envelope(
                2,
                ModelEvent::ItemStarted {
                    item_id: item.clone(),
                    index: 0,
                    kind: ItemKind::Message,
                },
            ),
            envelope(
                3,
                ModelEvent::TextDelta {
                    item_id: item.clone(),
                    fragment: StreamFragment::new(b"done".to_vec(), limits).expect("fragment"),
                },
            ),
            envelope(4, ModelEvent::ItemCompleted(item)),
            envelope(5, ModelEvent::Finish(FinishReason::Stop)),
            envelope(6, ModelEvent::ResponseCompleted),
        ]);
        let provider = FakeProvider { profile: profile.clone(), events: Mutex::new(Some(events)) };
        let mut session =
            ModelSession::start(&provider, request(&profile), limits, CancellationToken::new())
                .await
                .expect("session");

        for expected in 1..=6 {
            assert!(matches!(
                session.pull_one().await.expect("pull"),
                ModelAdvance::EnvelopePending { sequence, .. } if sequence == expected
            ));
            assert!(matches!(session.pull_one().await, Err(ModelDriveError::PendingEnvelope)));
            let encoded = session.encode_pending().expect("durable bytes");
            let decoded = peritus_model_protocol::decode_event_envelope(&encoded, limits)
                .expect("canonical event");
            assert_eq!(decoded.sequence(), expected);
            session.accept_durable_pending().expect("durable acknowledgement");
        }

        assert!(session.is_closed());
        assert!(session.terminal().is_some());
        assert_eq!(session.completed_items().len(), 1);
        assert!(matches!(session.pull_one().await.expect("closed"), ModelAdvance::Closed));
    });
}

#[test]
fn local_cancel_propagates_to_the_owned_provider_stream() {
    block_on(async {
        let profile = profile();
        let provider = FakeProvider {
            profile: profile.clone(),
            events: Mutex::new(Some(VecDeque::from([envelope(1, ModelEvent::ResponseCancelled)]))),
        };
        let cancellation = CancellationToken::new();
        let session = ModelSession::start(
            &provider,
            request(&profile),
            ProtocolLimits::PRODUCTION,
            cancellation.clone(),
        )
        .await
        .expect("session");
        session.cancel();
        assert!(cancellation.is_cancelled());
    });
}

fn envelope(sequence: u64, event: ModelEvent) -> EventEnvelope {
    EventEnvelope::new(
        sequence,
        None,
        None,
        Sha256Digest::new([u8::try_from(sequence).expect("sequence"); 32]),
        event,
    )
    .expect("envelope")
}

fn profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([7; 16]).expect("profile ID"),
        1,
        ProviderName::new("fake-provider".to_owned()).expect("provider"),
        ModelName::new("fake-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[], &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(8_192, 1_024, 8, 4, 64 * 1024).expect("limits"),
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
        RequestedCapabilities::new(&[], &[], profile.limits()).expect("requested capabilities"),
    )
    .expect("negotiation");
    let message = Message::new(
        Role::User,
        vec![peritus_model_protocol::ContentBlock::Text(
            BoundedText::new("work".to_owned(), limits).expect("text"),
        )],
        limits,
    )
    .expect("message");
    let options = RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(256, Vec::new(), None, None, None).expect("generation"),
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        Vec::new(),
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("request-1".to_owned()).expect("request"),
        vec![message],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        options,
        limits,
    )
    .expect("model request")
}
