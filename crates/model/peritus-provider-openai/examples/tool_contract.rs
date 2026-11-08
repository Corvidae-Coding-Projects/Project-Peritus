//! Public adapter consumer; --live performs one real Codex inference.
use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, JsonBounds, JsonSchema, Message, ModelLimits, ModelName,
    ModelRequest, OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits,
    ProviderName, ProviderProfile, ReasoningEffort, ReasoningPolicy, ReducedItem, RequestId,
    RequestOptions, RequestedCapabilities, ResponseReducer, ResumeKind, Role, SchemaDialect,
    StateMode, StructuredOutput, SummaryPolicy, ToolArgumentSchema, ToolChoice, ToolDefinition,
    ToolName, WireDialect, negotiate,
};
use peritus_provider_core::{CancellationToken, ModelProvider, ProcessLimits};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::ProviderProfileId;
use serde_json::Value;
use std::time::Duration;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let limits = ProtocolLimits::PRODUCTION;
    let profile = ProviderProfile::new(
        ProviderProfileId::new([0xA7; 16]).map_err(|e| format!("{e:?}"))?,
        1,
        ProviderName::new("openai".into())?,
        ModelName::new("gpt-6.1-sol".into())?,
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(&[Capability::ToolCalls, Capability::ReasoningControls], &[])?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 128, 8, 1)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?;
    let parameters = JsonSchema::parse(
        r#"{"type":"object","properties":{"label":{"type":"string","enum":["exact"]},"nullable":{"type":["string","null"]},"optional":{"type":"integer","minimum":0},"reference":{"type":"object","properties":{"kind":{"const":"rationale"},"invariant":{"type":"string","minLength":1}},"required":["kind","invariant"],"additionalProperties":false}},"required":["label","nullable","reference"],"additionalProperties":false}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(limits),
    )?;
    let contract = ToolArgumentSchema::compile(&parameters)?;
    let request=ModelRequest::new(&profile,negotiate(&profile,RequestedCapabilities::new(&[Capability::ToolCalls,Capability::ReasoningControls],&[],profile.limits())?)?,RequestId::new("contract-replay".into())?,vec![Message::new(Role::User,vec![ContentBlock::Text(BoundedText::new("Call record exactly once. Set label to exact, nullable to null, reference kind to rationale and invariant to Preserve committed state. Omit optional. This is an inert tool proposal verification; execute no native tools.".into(),limits)?)],limits)?],vec![ToolDefinition::new(ToolName::new("record".into())?,None,parameters,true)],ToolChoice::Specific(ToolName::new("record".into())?),ParallelToolPolicy::Disabled,RequestOptions::new(StructuredOutput::Text,ReasoningPolicy::Effort{effort:ReasoningEffort::XHigh,summary:SummaryPolicy::None},GenerationConfig::new(4000,vec![],None,None,None)?,CachePolicy::Disabled,PersistencePolicy::LOCAL_FIRST,None,vec![]),limits)?;
    if std::env::args().any(|a| a == "--live") {
        let provider = CodexRuntimeProvider::new(CodexRuntimeConfig::new(
            CodexExecutable::discover()?,
            profile.clone(),
            ProcessLimits::new(4 * 1024 * 1024, 1024 * 1024, 65_536, Duration::from_mins(3))?,
        )?);
        let mut stream = provider.start(request, CancellationToken::new()).await?;
        let mut reducer = ResponseReducer::new(profile.provider().clone(), limits);
        while let Some(envelope) = stream.pull().await? {
            reducer.push(envelope)?;
        }
        let calls = reducer
            .completed_items()
            .iter()
            .filter_map(|item| {
                if let ReducedItem::ToolCall { call, .. } = item { Some(call) } else { None }
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 1, "terminal={:?}", reducer.terminal());
        contract.validate(calls[0].arguments())?;
        let args: Value = serde_json::from_slice(calls[0].arguments().canonical_bytes())?;
        assert_eq!(args["label"], "exact");
        assert_eq!(args["nullable"], Value::Null);
        assert!(args.get("optional").is_none());
        assert_eq!(args["reference"]["kind"], "rationale");
        println!(
            "PASS: real gpt-6.1-sol/XHigh Codex inference, typed nested arguments and exact optional/null restoration"
        );
    }

    Ok(())
}
