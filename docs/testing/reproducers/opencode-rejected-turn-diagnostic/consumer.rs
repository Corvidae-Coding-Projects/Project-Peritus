//! Public-library consumer for the OpenCode rejected-turn diagnostic replay.
//!
//! It drives the production provider registry against the scripted vendor stand-in and asserts
//! what the adapter publishes after a refused reply: the bounded `peritus.opencode_turn_correction`
//! extension must carry the rejected model text, and one corrected reply must still complete.
use peritus_daemon::{ProviderRegistry, ProviderRegistryLimits, ProviderRoute};
use peritus_model_protocol::*;
use peritus_provider_core::{
    CancellationToken, Credential, CredentialReference, CredentialSource, ModelProvider,
    ProviderCoreError,
};
use serde_json::json;
use std::{error::Error, path::Path, sync::Arc};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

const PLAIN_PROSE: &str = "Here is the plan.\n\n1. Finish the mechanical survey work.\n2. Bring the product decisions back for review.\n\nNo tool call is needed.";

struct LocalCredential;

impl CredentialSource for LocalCredential {
    fn resolve(
        &self,
        reference: &CredentialReference,
    ) -> std::result::Result<Credential, ProviderCoreError> {
        let _reference = reference;
        let key = std::fs::read(
            std::env::var("PERITUS_REAL_KEY_FILE")
                .map_err(|_| ProviderCoreError::credential("key file not selected"))?,
        )
        .map_err(|_| ProviderCoreError::credential("test credential unavailable"))?;
        let key = std::str::from_utf8(&key)
            .map_err(|_| ProviderCoreError::credential("test credential encoding"))?
            .trim();
        Credential::new(key.as_bytes().to_vec())
    }
}

fn provider(value: serde_json::Value) -> Result<Arc<dyn ModelProvider>> {
    let route: ProviderRoute = serde_json::from_value(value)?;
    let declaration = route.declaration()?;
    let id = declaration.profile().profile_id();
    let rev = declaration.profile().revision();
    let registry = ProviderRegistry::build(
        vec![declaration],
        ProviderRegistryLimits::PRODUCTION,
        Some(Arc::new(LocalCredential)),
    )?;
    assert_eq!(
        registry.adapter_kind(id, rev).ok_or("no adapter")?,
        peritus_daemon::ProviderAdapterKind::OpenCodeRuntime
    );
    Ok(registry.current_provider(id).ok_or("no provider")?)
}

fn message(role: Role, text: &str) -> Result<Message> {
    Ok(Message::new(
        role,
        vec![ContentBlock::Text(BoundedText::new(text.into(), ProtocolLimits::PRODUCTION)?)],
        ProtocolLimits::PRODUCTION,
    )?)
}

fn request(provider: &dyn ModelProvider) -> Result<ModelRequest> {
    let profile = provider.profile();
    let capabilities = negotiate(
        profile,
        RequestedCapabilities::new(
            &[],
            &[Capability::UsageDetail, Capability::Streaming, Capability::ParallelToolCalls],
            profile.limits(),
        )?,
    )?;
    let options = RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(2048, vec![], None, None, None)?,
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        vec![],
    );
    Ok(ModelRequest::new(
        profile,
        capabilities,
        RequestId::new("scripted-rejected-turn-request".into())?,
        vec![message(Role::User, "Reply with the scripted vendor turn.")?],
        vec![],
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        options,
        ProtocolLimits::PRODUCTION,
    )?)
}

fn request_with_tools(provider: &dyn ModelProvider) -> Result<ModelRequest> {
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
    let tools = ["workspace_read", "workspace_search"]
        .into_iter()
        .map(|name| {
            Ok(ToolDefinition::new(ToolName::new(name.to_owned())?, None, schema.clone(), false))
        })
        .collect::<Result<Vec<_>>>()?;
    let options = RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(2048, vec![], None, None, None)?,
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        vec![],
    );
    Ok(ModelRequest::new(
        profile,
        capabilities,
        RequestId::new("scripted-dsml-request".into())?,
        vec![message(Role::User, "Reply with the scripted vendor turn.")?],
        tools,
        ToolChoice::Auto,
        ParallelToolPolicy::Allowed(2),
        options,
        ProtocolLimits::PRODUCTION,
    )?)
}

async fn drive(
    provider: &dyn ModelProvider,
    request: ModelRequest,
) -> Result<(ResponseReducer, Vec<serde_json::Value>, Vec<serde_json::Value>, Vec<serde_json::Value>)>
{
    let mut reducer =
        ResponseReducer::new(provider.profile().provider().clone(), ProtocolLimits::PRODUCTION);
    let mut corrections = Vec::new();
    let mut healings = Vec::new();
    let mut normalizations = Vec::new();
    let mut stream = provider.start(request, CancellationToken::new()).await?;
    while let Some(event) = stream.pull().await? {
        if let ModelEvent::ProviderEvent(extension) = event.event() {
            match extension.name().as_str() {
                "peritus.opencode_turn_correction" => {
                    let value: serde_json::Value =
                        serde_json::from_slice(extension.value().canonical_bytes())?;
                    println!("CORRECTION {}", serde_json::to_string(&value)?);
                    corrections.push(value);
                }
                "peritus.response_healing" => {
                    let value: serde_json::Value =
                        serde_json::from_slice(extension.value().canonical_bytes())?;
                    println!(
                        "HEALING {} {}",
                        value["policy"].as_str().unwrap_or(""),
                        value["target"].as_str().unwrap_or("")
                    );
                    healings.push(value);
                }
                "peritus.opencode_turn_normalization" => {
                    let value: serde_json::Value =
                        serde_json::from_slice(extension.value().canonical_bytes())?;
                    println!(
                        "NORMALIZATION {} {} {}",
                        value["policy"].as_str().unwrap_or(""),
                        value["target"].as_str().unwrap_or(""),
                        value["tool_calls"].as_u64().unwrap_or(0)
                    );
                    normalizations.push(value);
                }
                _ => {}
            }
        }
        reducer.push(event)?;
    }
    if reducer.terminal().is_none() {
        return Err("missing terminal".into());
    }
    Ok((reducer, corrections, healings, normalizations))
}

fn check_healing(healings: &[serde_json::Value]) -> Result<()> {
    if healings.len() != 1 {
        return Err(format!("expected one envelope repair, saw {}", healings.len()).into());
    }
    let value = &healings[0];
    if value["policy"].as_str() != Some("private-envelope-v1")
        || value["target"].as_str() != Some("opencode.turn")
    {
        return Err(format!("healing has unexpected fields: {value}").into());
    }
    Ok(())
}

fn check_text(reducer: &ResponseReducer, expected: &str) -> Result<()> {
    let mut text = String::new();
    for item in reducer.completed_items() {
        if let ReducedItem::Text { text: value, .. } = item {
            text.push_str(value.expose_for_wire());
        }
    }
    if text != expected {
        return Err(format!("unexpected recovered content: {text:?}").into());
    }
    Ok(())
}

fn check_plain_text_normalization(normalizations: &[serde_json::Value]) -> Result<()> {
    if normalizations.len() != 1 {
        return Err(
            format!("expected one plain-text normalization, saw {}", normalizations.len()).into()
        );
    }
    let value = &normalizations[0];
    if value["policy"].as_str() != Some("plain-text-v1")
        || value["target"].as_str() != Some("opencode.turn")
        || value["tool_calls"].as_u64() != Some(0)
        || value["tools_executed"].as_bool() != Some(false)
    {
        return Err(format!("plain-text normalization has unexpected fields: {value}").into());
    }
    Ok(())
}

fn check_normalization(
    reducer: &ResponseReducer,
    normalizations: &[serde_json::Value],
) -> Result<()> {
    if normalizations.len() != 1 {
        return Err(format!("expected one DSML normalization, saw {}", normalizations.len()).into());
    }
    let value = &normalizations[0];
    if value["policy"].as_str() != Some("dsml-v1")
        || value["target"].as_str() != Some("opencode.turn")
        || value["tool_calls"].as_u64() != Some(2)
        || value["tools_executed"].as_bool() != Some(false)
    {
        return Err(format!("normalization has unexpected fields: {value}").into());
    }
    let mut text = None;
    let mut calls = Vec::new();
    for item in reducer.completed_items() {
        match item {
            ReducedItem::Text { text: value, .. } => text = Some(value.expose_for_wire()),
            ReducedItem::ToolCall { call, .. } => {
                let arguments = serde_json::from_slice(call.arguments().canonical_bytes())?;
                calls.push((call.name().as_str().to_owned(), arguments));
            }
            _ => {}
        }
    }
    if text != Some("I will inspect the README and search the source.") {
        return Err(format!("unexpected normalized content: {text:?}").into());
    }
    let expected = vec![
        ("workspace_read".to_owned(), json!({"path": "README.md"})),
        ("workspace_search".to_owned(), json!({"query": "TODO", "path": "src"})),
    ];
    if calls != expected {
        return Err(format!("unexpected normalized calls: {calls:?}").into());
    }
    Ok(())
}

fn check_correction(value: &serde_json::Value, attempt: u64) -> Result<()> {
    if value["attempt"].as_u64() != Some(attempt)
        || value["reason"].as_str() != Some("opencode.runtime.invalid_turn_json")
        || value["tools_executed"].as_bool() != Some(false)
        || value["rejected_truncated"].as_bool() != Some(false)
    {
        return Err(format!("correction {attempt} has unexpected fields: {value}").into());
    }
    let rejected = value["rejected"].as_str().ok_or("correction retained no rejected text")?;
    if !rejected.starts_with("{\"content\":\"Scripted reply")
        || !rejected.ends_with("}}")
        || value["rejected_bytes"].as_u64() != Some(rejected.len() as u64)
    {
        return Err(format!("correction {attempt} retained unexpected text: {rejected}").into());
    }
    Ok(())
}

fn check_single_rejection(corrections: &[serde_json::Value], reason: &str) -> Result<()> {
    if corrections.len() != 1 {
        return Err(format!("expected one correction, saw {}", corrections.len()).into());
    }
    if corrections[0]["reason"].as_str() != Some(reason)
        || corrections[0]["tools_executed"].as_bool() != Some(false)
    {
        return Err(format!("unexpected correction: {}", corrections[0]).into());
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let arguments = std::env::args().collect::<Vec<_>>();
    let expectation = arguments.get(1).ok_or(
        "expectation needed (accepted|rejected|prelude_*|dsml|dsml_unknown_tool|dsml_malformed)",
    )?;
    let _root = Path::new(&std::env::var("PERITUS_REAL_CASE_ROOT")?);
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(std::env::var("PERITUS_REAL_CONFIG")?)?)?;
    let selected = config["providers"]
        .as_array()
        .ok_or("provider array missing")?
        .iter()
        .find(|route| {
            route
                .get("hosted_service")
                .and_then(toml::Value::as_str)
                .is_some_and(|service| matches!(service, "opencode-zen" | "opencode-go"))
        })
        .ok_or("configuration has no saved OpenCode route")?;
    let mut route = serde_json::to_value(selected)?;
    route["profile"]["model"] = json!("scripted-reply-model");
    route["executable"] = json!(std::env::var("PERITUS_REAL_CAPTURE")?);
    route["profile"]["capabilities"] =
        json!(["parallel-tool-calls", "streaming", "tool-calls", "usage-detail"]);
    let provider = provider(route)?;
    let request = if matches!(
        expectation.as_str(),
        "raw_newlines"
            | "missing_tool_calls"
            | "trailing_tool_calls_fragment"
            | "plain_prose"
            | "plain_tool_markup"
            | "dsml"
            | "dsml_unknown_tool"
            | "dsml_malformed"
    ) {
        request_with_tools(provider.as_ref())?
    } else {
        request(provider.as_ref())?
    };
    let (reducer, corrections, healings, normalizations) =
        drive(provider.as_ref(), request).await?;
    match reducer.terminal() {
        Some(TerminalOutcome::Succeeded { .. }) => println!("TERMINAL succeeded"),
        Some(TerminalOutcome::Failed(failure)) => {
            println!("TERMINAL failed {:?} {}", failure.category(), failure.diagnostic().code())
        }
        Some(TerminalOutcome::RequiresAction { .. }) => println!("TERMINAL requires_action"),
        other => println!("TERMINAL {other:?}"),
    }
    match expectation.as_str() {
        "accepted" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
                return Err("the corrected reply did not complete the turn".into());
            }
            if corrections.len() != 1 {
                return Err(format!("expected one correction, saw {}", corrections.len()).into());
            }
            check_correction(&corrections[0], 1)?;
        }
        "rejected" => {
            let code = match reducer.terminal() {
                Some(TerminalOutcome::Failed(failure)) => failure.diagnostic().code().to_owned(),
                _ => return Err("the refused turn did not fail".into()),
            };
            if code != "opencode.runtime.invalid_turn_json" {
                return Err(format!("unexpected terminal diagnostic {code}").into());
            }
            if corrections.len() != 2 {
                return Err(format!("expected two corrections, saw {}", corrections.len()).into());
            }
            check_correction(&corrections[0], 1)?;
            check_correction(&corrections[1], 2)?;
        }
        "prelude_backticks" | "prelude_quoted" | "prelude_fenced" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
                return Err("the repaired prelude did not complete the turn".into());
            }
            if !corrections.is_empty() {
                return Err(format!("expected no refusal, saw {}", corrections.len()).into());
            }
            check_healing(&healings)?;
        }
        "raw_newlines" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
                return Err("the raw-newline envelope did not complete the turn".into());
            }
            if !corrections.is_empty() {
                return Err(format!("expected no refusal, saw {}", corrections.len()).into());
            }
            check_healing(&healings)?;
            check_text(
                &reducer,
                "First planning step.\n\nSecond step.\tEvidence is in `PROJECT_PLAN.md`.",
            )?;
        }
        "missing_tool_calls" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
                return Err("the content-only envelope did not complete the turn".into());
            }
            if !corrections.is_empty() || !healings.is_empty() {
                return Err("the content-only envelope was needlessly refused or repaired".into());
            }
            check_text(&reducer, "Plan with omitted empty tool calls.")?;
        }
        "trailing_tool_calls_fragment" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
                return Err(
                    "the duplicated trailing tool-call fragment did not complete the turn".into()
                );
            }
            if !corrections.is_empty() {
                return Err(format!("expected no refusal, saw {}", corrections.len()).into());
            }
            check_healing(&healings)?;
            check_text(&reducer, "Plan with a duplicated trailing empty tool-call fragment.")?;
        }
        "plain_prose" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
                return Err("the unenveloped prose turn did not complete".into());
            }
            if !corrections.is_empty() || !healings.is_empty() {
                return Err("the unenveloped prose turn was needlessly refused or repaired".into());
            }
            check_plain_text_normalization(&normalizations)?;
            check_text(&reducer, PLAIN_PROSE)?;
        }
        "plain_tool_markup" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
                return Err("the corrected pseudo-tool prose did not complete".into());
            }
            check_single_rejection(&corrections, "opencode.runtime.invalid_turn_json")?;
        }
        "dsml" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::RequiresAction { .. })) {
                return Err("the normalized DSML turn did not request its tools".into());
            }
            if !corrections.is_empty() {
                return Err(format!("expected no refusal, saw {}", corrections.len()).into());
            }
            check_normalization(&reducer, &normalizations)?;
        }
        "dsml_unknown_tool" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
                return Err("the corrected undeclared-tool turn did not complete".into());
            }
            check_single_rejection(&corrections, "opencode.runtime.invalid_turn_tools")?;
        }
        "dsml_malformed" => {
            if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
                return Err("the corrected malformed-DSML turn did not complete".into());
            }
            check_single_rejection(&corrections, "opencode.runtime.invalid_turn_json")?;
        }
        other => return Err(format!("unknown expectation {other}").into()),
    }
    println!("RESULT {expectation} checks passed");
    Ok(())
}
