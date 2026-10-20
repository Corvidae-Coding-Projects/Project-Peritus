//! Real public provider adapter consumer; the external vendor supplies a recorded reply fixture.
use peritus_daemon::{ProviderRegistry, ProviderRegistryLimits, ProviderRoute};
use peritus_model_protocol::*;
use peritus_provider_core::{
    CancellationToken, Credential, CredentialReference, CredentialSource, ProviderCoreError,
};
use serde_json::json;
use std::{error::Error, io::Write as _, sync::Arc};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct PlaceholderCredential;
impl CredentialSource for PlaceholderCredential {
    fn resolve(
        &self,
        _: &CredentialReference,
    ) -> std::result::Result<Credential, ProviderCoreError> {
        Credential::new(b"disposable-wire-replay-placeholder".to_vec())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let case: serde_json::Value =
        serde_json::from_slice(&std::fs::read(std::env::var("PERITUS_DSML_CASE")?)?)?;
    let route: ProviderRoute = serde_json::from_value(json!({
        "kind":"hosted", "hosted_service":case["service"].as_str().unwrap_or("opencode-zen"),
        "wire_protocol":"opencode-runtime", "executable":std::env::var("PERITUS_DSML_VENDOR")?,
        "credential_reference":"peritus-secret-v1:00000000000000000000000000000001:0000000000000000000000000000000000000000000000000000000000000001",
        "profile":{"profile_id":"a8000000000000000000000000000001", "revision":1,
            "model":"scripted-reply-model", "capabilities":["parallel-tool-calls","streaming","tool-calls","usage-detail"],
            "max_input_tokens":32768,"max_output_tokens":4096,"max_tools":64,
            "max_parallel_tool_calls":8,"max_inline_media_bytes":1}
    }))?;
    let declaration = route.declaration()?;
    let profile_id = declaration.profile().profile_id();
    let registry = ProviderRegistry::build(
        vec![declaration],
        ProviderRegistryLimits::PRODUCTION,
        Some(Arc::new(PlaceholderCredential)),
    )?;
    let provider = registry.current_provider(profile_id).ok_or("provider missing")?;
    let profile = provider.profile();
    let capabilities = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::ToolCalls],
            &[Capability::UsageDetail, Capability::Streaming, Capability::ParallelToolCalls],
            profile.limits(),
        )?,
    )?;
    let schema = JsonSchema::parse(
        r#"{"type":"object"}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )?;
    let tools = ["workspace_read", "workspace_search", "workspace_write", "workspace_remove"]
        .into_iter()
        .map(|name| {
            Ok(ToolDefinition::new(ToolName::new(name.to_owned())?, None, schema.clone(), false))
        })
        .collect::<Result<Vec<_>>>()?;
    let choice = match case["choice"].as_str().unwrap_or("auto") {
        "none" => ToolChoice::None,
        "required" => ToolChoice::Required,
        "specific" => ToolChoice::Specific(ToolName::new("workspace_read".to_owned())?),
        _ => ToolChoice::Auto,
    };
    let request = ModelRequest::new(
        profile,
        capabilities,
        RequestId::new("dsml-integrity".into())?,
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text(BoundedText::new(
                "Use the supplied wire fixture.".into(),
                ProtocolLimits::PRODUCTION,
            )?)],
            ProtocolLimits::PRODUCTION,
        )?],
        tools,
        choice,
        ParallelToolPolicy::Allowed(2),
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(2048, vec![], None, None, None)?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            vec![],
        ),
        ProtocolLimits::PRODUCTION,
    )?;
    let mut reducer = ResponseReducer::new(profile.provider().clone(), ProtocolLimits::PRODUCTION);
    let mut stream = provider.start(request, CancellationToken::new()).await?;
    let mut corrections = Vec::new();
    let mut normalizations = 0;
    let mut trace = std::fs::File::create(std::env::var("PERITUS_DSML_TRACE")?)?;
    while let Some(envelope) = stream.pull().await? {
        let encoded = encode_event_envelope(&envelope, ProtocolLimits::PRODUCTION)?;
        trace.write_all(&[1])?;
        trace.write_all(&(encoded.len() as u64).to_le_bytes())?;
        trace.write_all(&encoded)?;
        if let ModelEvent::ProviderEvent(extension) = envelope.event() {
            if extension.name().as_str() == "peritus.opencode_turn_correction" {
                corrections.push(serde_json::from_slice::<serde_json::Value>(
                    extension.value().canonical_bytes(),
                )?);
            }
            if extension.name().as_str() == "peritus.opencode_turn_normalization" {
                normalizations += 1;
            }
        }
        reducer.push(envelope)?;
    }
    let mut calls = Vec::new();
    let mut text = Vec::new();
    for item in reducer.completed_items() {
        match item {
            ReducedItem::ToolCall { call, .. } => calls.push(json!({"name":call.name().as_str(),
                "arguments":serde_json::from_slice::<serde_json::Value>(call.arguments().canonical_bytes())?})),
            ReducedItem::Text { text: value, .. } => text.push(value.expose_for_wire()),
            _ => {}
        }
    }
    let terminal = match reducer.terminal() {
        Some(TerminalOutcome::RequiresAction { .. }) => "requires_action",
        Some(TerminalOutcome::Succeeded { .. }) => "succeeded",
        Some(TerminalOutcome::Failed(_)) => "failed",
        _ => "other",
    };
    println!(
        "{}",
        json!({"terminal":terminal,"calls":calls,"text":text,"corrections":corrections,"normalizations":normalizations})
    );
    Ok(())
}
