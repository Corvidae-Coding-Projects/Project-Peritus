//! Minimal live-canary request construction and explicit capability negotiation.
use super::ProviderCanaryError;
use peritus_model_protocol::{
    BoundedText, CachePolicy, Capability, ContentBlock, GenerationConfig, Message, ModelRequest,
    ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderProfile, ReasoningEffort,
    ReasoningPolicy, RequestId, RequestOptions, RequestedCapabilities, Role, StructuredOutput,
    SummaryPolicy, ToolChoice, negotiate,
};

pub(super) fn request(profile: &ProviderProfile) -> Result<ModelRequest, ProviderCanaryError> {
    let optional = [Capability::Streaming, Capability::ReasoningControls, Capability::UsageDetail];
    let negotiated =
        negotiate(profile, RequestedCapabilities::new(&[], &optional, profile.limits())?)?;
    let limits = ProtocolLimits::PRODUCTION;
    let prompt = BoundedText::new(
        "Reply with one short word to confirm this provider route is usable.".to_owned(),
        limits,
    )?;
    let messages = vec![Message::new(Role::User, vec![ContentBlock::Text(prompt)], limits)?];
    let reasoning = if negotiated.includes(Capability::ReasoningControls) {
        ReasoningPolicy::Effort { effort: ReasoningEffort::Low, summary: SummaryPolicy::None }
    } else {
        ReasoningPolicy::Disabled
    };
    let options = RequestOptions::new(
        StructuredOutput::Text,
        reasoning,
        GenerationConfig::new(
            256.min(profile.limits().max_output_tokens()),
            Vec::new(),
            None,
            None,
            None,
        )?,
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        Vec::new(),
    );
    Ok(ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("peritus-live-provider-canary".to_owned())?,
        messages,
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        options,
        limits,
    )?)
}
