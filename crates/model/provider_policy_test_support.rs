//! Shared provider-test construction for elevated-policy projection checks.

use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, Message, ModelLimits, ModelName, ModelRequest,
    OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderName,
    ProviderProfile, ReasoningPolicy, RequestId, RequestedCapabilities, ResumeKind, Role,
    StateMode, StructuredOutput, ToolChoice, WireDialect, negotiate,
};
use peritus_types::ProviderProfileId;

pub const POLICY_MARKER: &str = "prompt-vitamins-domain-design-v2";
pub const POLICY_TEXT: &str =
    "Domain policy [prompt-vitamins-domain-design-v2]: preserve the established capability.";
pub const TASK_TEXT: &str = "CURRENT-USER-TASK-SENTINEL-8317";

pub fn profile(
    provider: &str,
    dialect: WireDialect,
    enforcement: OutputLimitEnforcement,
) -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([0x83; 16]).expect("fixture profile identity"),
        1,
        ProviderName::new(provider.to_owned()).expect("fixture provider name"),
        ModelName::new("fixture-model".to_owned()).expect("fixture model name"),
        dialect,
        CapabilityMatrix::new(&[Capability::Streaming], &[]).expect("fixture capabilities"),
        CapabilityProvenance::Profiled,
        ModelLimits::new(65_536, 8_192, 16, 4, 1_048_576).expect("fixture model limits"),
        enforcement,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("fixture provider profile")
}

pub fn request(profile: &ProviderProfile) -> ModelRequest {
    let limits = ProtocolLimits::PRODUCTION;
    let requested = RequestedCapabilities::new(&[Capability::Streaming], &[], profile.limits())
        .expect("fixture requested capabilities");
    let negotiated = negotiate(profile, requested).expect("fixture capability negotiation");
    let text = |role, value: &str| {
        Message::new(
            role,
            vec![ContentBlock::Text(
                BoundedText::new(value.to_owned(), limits).expect("fixture text"),
            )],
            limits,
        )
        .expect("fixture message")
    };
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("policy-projection-fixture".to_owned()).expect("fixture request identity"),
        vec![text(Role::System, POLICY_TEXT), text(Role::User, TASK_TEXT)],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        peritus_model_protocol::RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(1_024, Vec::new(), None, None, None)
                .expect("fixture generation controls"),
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .expect("fixture model request")
}

pub fn count(text: &str, needle: &str) -> usize {
    text.match_indices(needle).count()
}
