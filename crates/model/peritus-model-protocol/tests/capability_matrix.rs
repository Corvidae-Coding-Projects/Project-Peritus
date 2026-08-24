//! Exhaustive independent feature and numeric-limit negotiation coverage.

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, CapabilityState,
    ModelLimits, ModelName, OutputLimitEnforcement, ProtocolErrorKind, ProviderName,
    ProviderProfile, RequestedCapabilities, ResumeKind, StateMode, WireDialect, negotiate,
};
use peritus_types::ProviderProfileId;

const fn capabilities() -> [Capability; 17] {
    [
        Capability::Streaming,
        Capability::ToolCalls,
        Capability::ParallelToolCalls,
        Capability::StrictStructuredOutput,
        Capability::PromptCaching,
        Capability::ImageInput,
        Capability::AudioInput,
        Capability::DocumentInput,
        Capability::ReasoningControls,
        Capability::ReasoningSummaries,
        Capability::ResumableResponse,
        Capability::ConfirmedCancellation,
        Capability::UsageDetail,
        Capability::RateLimitDetail,
        Capability::StoredState,
        Capability::ProviderExtensions,
        Capability::SamplingControls,
    ]
}

fn limits() -> ModelLimits {
    ModelLimits::new(1_000, 500, 10, 4, 1_000).expect("limits")
}

fn profile(matrix: CapabilityMatrix) -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([5; 16]).expect("profile identity"),
        1,
        ProviderName::new("matrix-provider".to_owned()).expect("provider"),
        ModelName::new("matrix-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        matrix,
        CapabilityProvenance::Profiled,
        limits(),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

#[test]
fn every_independent_capability_has_supported_unknown_and_unsupported_behavior() {
    for capability in capabilities() {
        let supported_matrix = CapabilityMatrix::new(&[capability], &[]).expect("supported");
        assert_eq!(supported_matrix.state(capability), CapabilityState::Supported);
        let supported_profile = profile(supported_matrix);
        let required = RequestedCapabilities::new(&[capability], &[], limits()).expect("required");
        assert!(negotiate(&supported_profile, required).expect("supported").includes(capability));

        let unknown_matrix = CapabilityMatrix::new(&[], &[capability]).expect("unknown");
        assert_eq!(unknown_matrix.state(capability), CapabilityState::Unknown);
        let unknown_profile = profile(unknown_matrix);
        let required = RequestedCapabilities::new(&[capability], &[], limits()).expect("required");
        assert_eq!(
            negotiate(&unknown_profile, required).expect_err("unknown is not support").kind(),
            ProtocolErrorKind::UnsupportedCapability
        );

        let unsupported_matrix = CapabilityMatrix::new(&[], &[]).expect("unsupported");
        assert_eq!(unsupported_matrix.state(capability), CapabilityState::Unsupported);
        let unsupported_profile = profile(unsupported_matrix);
        let required = RequestedCapabilities::new(&[capability], &[], limits()).expect("required");
        assert_eq!(
            negotiate(&unsupported_profile, required)
                .expect_err("unsupported required capability")
                .kind(),
            ProtocolErrorKind::UnsupportedCapability
        );
        let optional = RequestedCapabilities::new(&[], &[capability], limits()).expect("optional");
        assert!(
            !negotiate(&unsupported_profile, optional)
                .expect("optional unsupported capability is omitted")
                .includes(capability)
        );
    }
}

#[test]
fn capability_iteration_is_complete_stable_and_three_valued() {
    let supported = &capabilities()[..6];
    let unknown = &capabilities()[6..12];
    let matrix = CapabilityMatrix::new(supported, unknown).expect("matrix");
    let observed = matrix.iter().collect::<Vec<_>>();
    assert_eq!(observed.len(), capabilities().len());
    for (index, (capability, state)) in observed.into_iter().enumerate() {
        assert_eq!(capability, capabilities()[index]);
        let expected = if index < 6 {
            CapabilityState::Supported
        } else if index < 12 {
            CapabilityState::Unknown
        } else {
            CapabilityState::Unsupported
        };
        assert_eq!(state, expected);
    }
}

#[test]
fn every_model_limit_is_intersected_independently() {
    let profile = profile(CapabilityMatrix::new(&[], &[]).expect("matrix"));
    let requested_limits = ModelLimits::new(800, 800, 8, 6, 2_000).expect("requested limits");
    let negotiated = negotiate(
        &profile,
        RequestedCapabilities::new(&[], &[], requested_limits).expect("requested"),
    )
    .expect("negotiated")
    .limits();
    assert_eq!(negotiated.max_input_tokens(), 800);
    assert_eq!(negotiated.max_output_tokens(), 500);
    assert_eq!(negotiated.max_tools(), 8);
    assert_eq!(negotiated.max_parallel_tool_calls(), 4);
    assert_eq!(negotiated.max_inline_media_bytes(), 1_000);

    for invalid in [
        (0, 1, 1, 1, 1),
        (1, 0, 1, 1, 1),
        (1, 1, 0, 1, 1),
        (1, 1, 1, 0, 1),
        (1, 1, 1, 1, 0),
        (1, 1, 1, 2, 1),
    ] {
        assert_eq!(
            ModelLimits::new(invalid.0, invalid.1, invalid.2, invalid.3, invalid.4)
                .expect_err("invalid independent limit")
                .kind(),
            ProtocolErrorKind::InvalidLimit
        );
    }
}

#[test]
fn lifecycle_claims_require_matching_capabilities() {
    let matrix = CapabilityMatrix::new(&[], &[]).expect("matrix");
    for (state, resume, cancellation) in [
        (StateMode::ProviderStored, ResumeKind::Unsupported, CancellationKind::Unsupported),
        (StateMode::StatelessReplay, ResumeKind::ExactCursor, CancellationKind::Unsupported),
        (StateMode::StatelessReplay, ResumeKind::Unsupported, CancellationKind::Confirmed),
    ] {
        let error = ProviderProfile::new(
            ProviderProfileId::new([8; 16]).expect("profile identity"),
            1,
            ProviderName::new("matrix-provider".to_owned()).expect("provider"),
            ModelName::new("matrix-model".to_owned()).expect("model"),
            WireDialect::CompatibleResponses,
            matrix,
            CapabilityProvenance::Profiled,
            limits(),
            OutputLimitEnforcement::ProviderEnforced,
            state,
            resume,
            cancellation,
        )
        .expect_err("unbacked lifecycle claim");
        assert_eq!(error.kind(), ProtocolErrorKind::InvalidProfile);
    }
}
