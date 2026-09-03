//! Provider qualification and stable recovery classification.

#[path = "support/runtime.rs"]
mod runtime;

use std::{collections::VecDeque, sync::Arc};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, EventEnvelope,
    FailureCategory, ItemId, ModelEvent, ModelFailure, ModelLimits, ModelName, ModelRequest,
    OutcomeCertainty, OutputLimitEnforcement, ProtocolLimits, ProviderName, ProviderProfile,
    RedactedDiagnostic, ResumeKind, Retryability, StateMode, StreamFragment, TransportPhase,
    WireDialect,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream,
    ProviderAvailability, ProviderCandidate, ProviderCoreError, ProviderQualification,
    ProviderRecoveryDisposition, ProviderRequirement, ProviderRoute, ProviderTerminal,
    ProviderTerminalCause, select_qualified_provider, verify_live_provider,
};
use peritus_types::{ProviderProfileId, Sha256Digest};

struct FixtureProvider {
    profile: ProviderProfile,
    availability: ProviderAvailability,
}

impl ModelProvider for FixtureProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn availability(&self) -> ProviderAvailability {
        self.availability
    }

    fn start(
        &self,
        _request: ModelRequest,
        _cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async {
            Err(ProviderCoreError::configuration(
                "qualification_fixture",
                "fixture does not execute model turns",
            ))
        })
    }
}

#[test]
fn image_role_selects_only_an_authorized_capable_available_fallback() {
    let primary = provider(1, WireDialect::AnthropicClaudeRuntime, false, 200_000);
    let denied = provider(2, WireDialect::OpenAiCodexRuntime, true, 200_000);
    let fallback = provider(3, WireDialect::OpenAiCodexRuntime, true, 200_000);
    let requirement = ProviderRequirement::new(true, 100_000, true).expect("requirement");

    let (selected, qualification) = select_qualified_provider(
        ProviderCandidate::new(primary.as_ref(), true),
        &[
            ProviderCandidate::new(denied.as_ref(), false),
            ProviderCandidate::new(fallback.as_ref(), true),
        ],
        requirement,
    )
    .expect("authorized image route");

    assert_eq!(selected.profile().profile_id(), fallback.profile().profile_id());
    assert_eq!(qualification.route(), ProviderRoute::AccountRuntime);
    assert!(qualification.image_input());
    assert!(qualification.tool_protocol());
    assert_eq!(qualification.maximum_context_tokens(), 200_000);
    assert_eq!(qualification.availability(), ProviderAvailability::LiveCanary);
}

#[test]
fn unavailable_or_small_context_routes_fail_before_invocation() {
    let unavailable = profile(4, WireDialect::OpenAiResponses, true, 200_000);
    let requirement = ProviderRequirement::new(false, 100_000, true).expect("requirement");
    assert!(
        ProviderQualification::evaluate(
            &unavailable,
            ProviderAvailability::Unchecked,
            requirement,
        )
        .is_err()
    );

    let small = profile(5, WireDialect::GeminiInteractionsV1, true, 32_000);
    assert!(
        ProviderQualification::evaluate(
            &small,
            ProviderAvailability::CredentialPresent,
            requirement,
        )
        .is_err()
    );
    assert_eq!(ProviderRoute::from_dialect(small.dialect()), ProviderRoute::FirstPartyApi);
    assert_eq!(
        ProviderRoute::from_dialect(WireDialect::CompatibleResponses),
        ProviderRoute::CompatibleApi,
    );
}

#[test]
fn terminals_keep_distinct_causes_and_safe_next_actions() {
    let context = model_failure(
        FailureCategory::InvalidRequest,
        OutcomeCertainty::Terminal,
        "openai.codex_runtime.context_limit",
    );
    let context = ProviderTerminal::from_model_failure(&context);
    assert_eq!(context.cause(), ProviderTerminalCause::ContextOverflow);
    assert_eq!(context.recovery(), ProviderRecoveryDisposition::CompactThenRetry);

    let capacity = model_failure(
        FailureCategory::TransientProvider,
        OutcomeCertainty::Terminal,
        "anthropic.claude_runtime.capacity",
    );
    let capacity = ProviderTerminal::from_model_failure(&capacity);
    assert_eq!(capacity.cause(), ProviderTerminalCause::Capacity);
    assert_eq!(capacity.recovery(), ProviderRecoveryDisposition::TryAuthorizedFallback);

    let ambiguous = model_failure(
        FailureCategory::Transport,
        OutcomeCertainty::MaybeAccepted,
        "compatible.submission.ambiguous",
    );
    let ambiguous = ProviderTerminal::from_model_failure(&ambiguous);
    assert_eq!(ambiguous.cause(), ProviderTerminalCause::AmbiguousAcceptance);
    assert_eq!(ambiguous.recovery(), ProviderRecoveryDisposition::Stop);

    assert_eq!(ProviderTerminal::empty_response().cause(), ProviderTerminalCause::EmptyResponse,);
    let timeout = ProviderTerminal::from_core_error(&ProviderCoreError::transport(
        "process_timeout",
        "owned subprocess exceeded its wall-clock limit",
    ));
    assert_eq!(timeout.cause(), ProviderTerminalCause::SubprocessTimeout);

    let authentication = model_failure(
        FailureCategory::Authentication,
        OutcomeCertainty::Terminal,
        "provider.authentication",
    );
    let authentication = ProviderTerminal::from_model_failure(&authentication);
    assert_eq!(authentication.cause(), ProviderTerminalCause::Authentication);
    assert_eq!(authentication.recovery(), ProviderRecoveryDisposition::AwaitCredentialRepair,);

    let malformed = model_failure(
        FailureCategory::MalformedPayload,
        OutcomeCertainty::MaybeAccepted,
        "provider.malformed",
    );
    let malformed = ProviderTerminal::from_model_failure(&malformed);
    assert_eq!(malformed.cause(), ProviderTerminalCause::MalformedResponse);
    assert_eq!(malformed.recovery(), ProviderRecoveryDisposition::RetrySameRoute);
}

struct CanaryProvider {
    profile: ProviderProfile,
    text: bool,
}

struct CanaryEvents(VecDeque<EventEnvelope>);

impl ModelStream for CanaryEvents {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move { Ok(self.0.pop_front()) })
    }
}

impl ModelProvider for CanaryProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        let mut events = VecDeque::new();
        if self.text {
            events.push_back(envelope(
                1,
                ModelEvent::TextDelta {
                    item_id: ItemId::new("canary-item".to_owned()).expect("item"),
                    fragment: StreamFragment::new(b"ok".to_vec(), ProtocolLimits::PRODUCTION)
                        .expect("fragment"),
                },
            ));
        }
        events.push_back(envelope(2, ModelEvent::ResponseCompleted));
        Box::pin(async move { Ok(OwnedModelStream::new(CanaryEvents(events), cancellation)) })
    }
}

#[test]
fn real_minimal_canary_requires_usable_text_before_completion() {
    runtime::block_on(async {
        let requirement = ProviderRequirement::new(false, 1, true).expect("requirement");
        let live = CanaryProvider {
            profile: profile(7, WireDialect::OpenAiCodexRuntime, true, 8_000),
            text: true,
        };
        let qualification = verify_live_provider(&live, requirement, CancellationToken::new())
            .await
            .expect("live canary");
        assert_eq!(qualification.availability(), ProviderAvailability::LiveCanary);

        let empty = CanaryProvider {
            profile: profile(8, WireDialect::AnthropicClaudeRuntime, false, 8_000),
            text: false,
        };
        let error = verify_live_provider(&empty, requirement, CancellationToken::new())
            .await
            .expect_err("empty canary");
        assert_eq!(
            error.terminal().expect("terminal").cause(),
            ProviderTerminalCause::EmptyResponse,
        );
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

fn provider(seed: u8, dialect: WireDialect, image: bool, context: u64) -> Arc<FixtureProvider> {
    Arc::new(FixtureProvider {
        profile: profile(seed, dialect, image, context),
        availability: ProviderAvailability::LiveCanary,
    })
}

fn profile(seed: u8, dialect: WireDialect, image: bool, context: u64) -> ProviderProfile {
    let mut capabilities = vec![Capability::ToolCalls];
    if image {
        capabilities.push(Capability::ImageInput);
    }
    ProviderProfile::new(
        ProviderProfileId::new([seed; 16]).expect("profile identity"),
        1,
        ProviderName::new(format!("provider-{seed}")).expect("provider"),
        ModelName::new(format!("model-{seed}")).expect("model"),
        dialect,
        CapabilityMatrix::new(&capabilities, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        ModelLimits::new(context, 4_096, 16, 4, 4 * 1024 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

fn model_failure(
    category: FailureCategory,
    certainty: OutcomeCertainty,
    code: &str,
) -> ModelFailure {
    ModelFailure::new(
        ProviderName::new("fixture".to_owned()).expect("provider"),
        category,
        TransportPhase::Completed,
        certainty,
        Retryability::Never,
        None,
        None,
        None,
        RedactedDiagnostic::new(code.to_owned(), None, None, None).expect("diagnostic"),
    )
}
