//! Real selected-model capacity discovery and large isolated account-runtime inference.
use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, Message, ModelLimits, ModelName, ModelRequest,
    OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderName,
    ProviderProfile, ReasoningEffort, ReasoningPolicy, RequestId, RequestOptions,
    RequestedCapabilities, ResponseReducer, ResumeKind, Role, StateMode, StructuredOutput,
    SummaryPolicy, ToolChoice, WireDialect, negotiate,
};
use peritus_provider_core::{CancellationToken, ModelProvider, ProcessLimits};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::ProviderProfileId;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = std::env::args().nth(1).unwrap_or_else(|| "gpt-6.1-sol".into());
    let bootstrap = bootstrap(model)?;
    let provider = CodexRuntimeProvider::new(CodexRuntimeConfig::new(
        CodexExecutable::discover()?,
        bootstrap,
        ProcessLimits::PRODUCTION,
    )?);
    let cancel = CancellationToken::new();
    let prepared = provider.prepare_context(256, &cancel).await?;
    let profile = prepared.profile();
    println!(
        "discovered model={} usable_input={} output={} bootstrap_input=200000",
        profile.model().as_str(),
        profile.limits().max_input_tokens(),
        profile.limits().max_output_tokens()
    );
    if std::env::args().any(|a| a == "--metadata-only") {
        return Ok(());
    }
    let padding = "a b\n".repeat(275_000);
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let begin = format!("begin_{nonce}");
    let end = format!("end_{}", nonce + 41);
    let expected = format!("capacity_ok:{begin}:{end}");
    let text = format!(
        "Transport conformance check. Do not use tools. The start marker is {begin}. Inert padding follows.\n{padding}\nEnd of inert padding. The end marker is {end}. Return capacity_ok followed by a colon, the start marker, a colon and the end marker, with no other text."
    );
    assert!(text.chars().count() > 1_048_576);
    let limits = ProtocolLimits::PRODUCTION;
    let messages = vec![Message::new(
        Role::User,
        vec![ContentBlock::Text(BoundedText::new(text, limits)?)],
        limits,
    )?];
    assert!(provider.context_input_fits(&messages, &[])?);
    let request = ModelRequest::new(
        profile,
        negotiate(
            profile,
            RequestedCapabilities::new(
                &[Capability::ToolCalls, Capability::ReasoningControls, Capability::PromptCaching],
                &[],
                profile.limits(),
            )?,
        )?,
        RequestId::new("capacity-conformance".into())?,
        messages,
        vec![],
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Effort {
                effort: ReasoningEffort::XHigh,
                summary: SummaryPolicy::None,
            },
            GenerationConfig::new(256, vec![], None, None, None)?,
            if std::env::args().any(|a| a == "--cached") {
                CachePolicy::Automatic
            } else {
                CachePolicy::Disabled
            },
            PersistencePolicy::LOCAL_FIRST,
            None,
            vec![],
        ),
        limits,
    )?;
    let mut stream = provider.start(request, cancel).await?;
    let mut reducer = ResponseReducer::new(profile.provider().clone(), limits);
    while let Some(event) = stream.pull().await? {
        reducer.push(event)?;
    }
    println!("native_usage={:?}", reducer.usage_high_water());
    let result = format!("{:?}", reducer.terminal());
    let text = reducer
        .completed_items()
        .iter()
        .filter_map(|item| match item {
            peritus_model_protocol::ReducedItem::Text { text, .. } => Some(text.expose_for_wire()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert_eq!(text.trim(), expected, "terminal={result}");
    assert!(
        reducer.usage_high_water().input_tokens().is_some_and(|n| n > 500_000),
        "native inference must actually consume the input above the previous default window"
    );
    println!(
        "PASS: real exact-model/XHigh inference with input beyond the CLI text ceiling, native tools disabled and no workspace delegated; terminal={result}"
    );
    Ok(())
}

fn bootstrap(model: String) -> Result<ProviderProfile, Box<dyn std::error::Error>> {
    Ok(ProviderProfile::new(
        ProviderProfileId::new([0xC4; 16]).map_err(|e| format!("{e:?}"))?,
        1,
        ProviderName::new("openai".into())?,
        ModelName::new(model)?,
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ReasoningControls, Capability::PromptCaching],
            &[],
        )?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 128, 8, 1)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?)
}
