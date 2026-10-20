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

async fn drive(
    provider: &dyn ModelProvider,
    request: ModelRequest,
) -> Result<(ResponseReducer, Vec<serde_json::Value>, Vec<serde_json::Value>)> {
    let mut reducer =
        ResponseReducer::new(provider.profile().provider().clone(), ProtocolLimits::PRODUCTION);
    let mut corrections = Vec::new();
    let mut healings = Vec::new();
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
                _ => {}
            }
        }
        reducer.push(event)?;
    }
    if reducer.terminal().is_none() {
        return Err("missing terminal".into());
    }
    Ok((reducer, corrections, healings))
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let arguments = std::env::args().collect::<Vec<_>>();
    let expectation = arguments.get(1).ok_or(
        "expectation needed (accepted|rejected|prelude_backticks|prelude_quoted|prelude_fenced)",
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
    let request = request(provider.as_ref())?;
    let (reducer, corrections, healings) = drive(provider.as_ref(), request).await?;
    match reducer.terminal() {
        Some(TerminalOutcome::Succeeded { .. }) => println!("TERMINAL succeeded"),
        Some(TerminalOutcome::Failed(failure)) => {
            println!("TERMINAL failed {:?} {}", failure.category(), failure.diagnostic().code())
        }
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
        other => return Err(format!("unknown expectation {other}").into()),
    }
    println!("RESULT {expectation} checks passed");
    Ok(())
}
