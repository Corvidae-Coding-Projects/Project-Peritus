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
    let kind = registry.adapter_kind(id, rev).ok_or("no adapter")?;
    assert_eq!(kind, peritus_daemon::ProviderAdapterKind::OpenCodeRuntime);
    Ok(registry.current_provider(id).ok_or("no provider")?)
}
fn message(role: Role, text: &str) -> Result<Message> {
    Ok(Message::new(
        role,
        vec![ContentBlock::Text(BoundedText::new(text.into(), ProtocolLimits::PRODUCTION)?)],
        ProtocolLimits::PRODUCTION,
    )?)
}
fn request(
    provider: &dyn ModelProvider,
    messages: Vec<Message>,
    tools: Vec<ToolDefinition>,
    choice: ToolChoice,
    parallel: ParallelToolPolicy,
) -> Result<ModelRequest> {
    let p = provider.profile();
    let caps = negotiate(
        p,
        RequestedCapabilities::new(
            if tools.is_empty() { &[] } else { &[Capability::ToolCalls] },
            &[Capability::UsageDetail, Capability::Streaming, Capability::ParallelToolCalls],
            p.limits(),
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
    let req = ModelRequest::new(
        p,
        caps,
        RequestId::new("real-opencode-consumer".into())?,
        messages,
        tools,
        choice,
        parallel,
        options,
        ProtocolLimits::PRODUCTION,
    )?;
    // Exercise the durable public request format as a consumer, not a private decoder probe.
    let bytes = req.canonical_bytes()?;
    Ok(decode_request(
        &bytes,
        p,
        RequestId::new("replayed-consumer-request".into())?,
        ProtocolLimits::PRODUCTION,
    )?)
}
async fn drive(
    provider: &dyn ModelProvider,
    request: ModelRequest,
    token: CancellationToken,
) -> Result<ResponseReducer> {
    let mut reducer =
        ResponseReducer::new(provider.profile().provider().clone(), ProtocolLimits::PRODUCTION);
    let mut stream = provider.start(request, token).await?;
    while let Some(event) = stream.pull().await? {
        if let ModelEvent::ProviderEvent(extension) = event.event() {
            if extension.name().as_str() == "peritus.opencode_turn_correction" {
                println!(
                    "CORRECTION {}",
                    String::from_utf8_lossy(extension.value().canonical_bytes())
                );
            }
        }
        reducer.push(event)?;
    }
    if reducer.terminal().is_none() {
        return Err("missing terminal".into());
    }
    Ok(reducer)
}
fn text(result: &ResponseReducer) -> String {
    result
        .completed_items()
        .iter()
        .filter_map(|i| {
            if let ReducedItem::Text { text, .. } = i { Some(text.expose_for_wire()) } else { None }
        })
        .collect()
}
fn tool(name: &str) -> Result<ToolDefinition> {
    let schema = JsonSchema::parse(
        r#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )?;
    Ok(ToolDefinition::new(ToolName::new(name.into())?, None, schema, false))
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let model = args.get(1).ok_or("model needed")?;
    let root_path = std::env::var("PERITUS_REAL_CASE_ROOT")?;
    let root = Path::new(&root_path);
    let cfg: toml::Value =
        toml::from_str(&std::fs::read_to_string(std::env::var("PERITUS_REAL_CONFIG")?)?)?;
    let selected = cfg["providers"]
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
    route["profile"]["model"] = json!(model);
    route["executable"] = json!(std::env::var("PERITUS_REAL_CAPTURE")?);
    route["profile"]["capabilities"] =
        json!(["parallel-tool-calls", "streaming", "tool-calls", "usage-detail"]);
    if args.get(4).is_some_and(|v| v == "go") {
        route["hosted_service"] = json!("opencode-go");
    }
    let provider = provider(route)?;
    if args.get(2).is_some_and(|v| v == "cancel") {
        let token = CancellationToken::new();
        let cancel = token.clone();
        let req = request(
            provider.as_ref(),
            vec![message(
                Role::User,
                "Write a detailed Rust networking implementation with 10000 lines, continuing without stopping.",
            )?],
            vec![],
            ToolChoice::None,
            ParallelToolPolicy::Disabled,
        )?;
        let future = drive(provider.as_ref(), req, token);
        let cancellation = async move {
            tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
            let _ = cancel.cancel();
        };
        let (result, ()) = tokio::join!(future, cancellation);
        let result = result?;
        if !matches!(result.terminal(), Some(TerminalOutcome::Cancelled)) {
            return Err("cancellation failed".into());
        }
        let req = request(
            provider.as_ref(),
            vec![message(Role::User, "Reply exactly 46, no decoration.")?],
            vec![],
            ToolChoice::None,
            ParallelToolPolicy::Disabled,
        )?;
        let result = drive(provider.as_ref(), req, CancellationToken::new()).await?;
        if text(&result) != "46" {
            return Err("route did not recover after cancellation".into());
        }
        println!("Cancellation and subsequent real inference passed");
        return Ok(());
    }
    let fixtures = root.join(format!("workspace-{model}"));
    std::fs::create_dir_all(&fixtures)?;
    std::fs::write(
        fixtures.join("HANDOFF.md"),
        "# Handoff\nInspect sim.rs before planning. The crime system needs severity-based fines and alignment-based witnesses. Preserve the quoted Windows path C:\\Users\\Admin\\crime.\n",
    )?;
    std::fs::write(
        fixtures.join("sim.rs"),
        "pub struct World { pub money: u32 }\npub fn step_npcs(world: &mut World) { world.money += 1; }\n// observed token: CRIME-CANARY-127-4D9A\n",
    )?;
    let output = root.join(format!(
        "{}-{}.jsonl",
        args.get(2).map(String::as_str).unwrap_or("after"),
        model
    ));
    let mut rows = vec![];
    let mut failures = 0;
    let mut completions = 0;
    for attempt in 0..args.get(3).and_then(|v| v.parse::<usize>().ok()).unwrap_or(8) {
        let nonce = format!(
            "CRIME-CANARY-127-{attempt:04}-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()
        );
        std::fs::write(
            fixtures.join("sim.rs"),
            format!(
                "pub struct World {{ pub money: u32 }}\npub fn step_npcs(world: &mut World) {{ world.money += 1; }}\n// observed token: {nonce}\n"
            ),
        )?;
        let mut actual_reads = std::collections::BTreeSet::new();
        let extra = match attempt % 3 {
            1 => {
                " When requesting tools, omit public commentary (empty content); only the tool proposals are needed."
            }
            2 => {
                " Your final public answer must be exactly a JSON object with token (the observed token), windows_path (the quoted Windows path from HANDOFF.md), label (café 雪 🦀), and plan (a short string)."
            }
            _ => "",
        };
        let mut history=vec![message(Role::System,"You are a coding assistant. Ground a read-only plan in actual files. Use only declared tools; never invent file contents. Do not run unit tests, verification gates, or commands.")?,message(Role::User,("Read HANDOFF.md and sim.rs with workspace_read, then give a short plan for severity-based fines and alignment-based witnesses. Include the exact observed token from sim.rs. No preamble is needed; just read the files before answering.".to_owned()+extra).as_str())?];
        for turn in 0..8 {
            let req = request(
                provider.as_ref(),
                history.clone(),
                vec![tool("workspace_read")?],
                ToolChoice::Auto,
                ParallelToolPolicy::Allowed(2),
            )?;
            std::fs::write(
                root.join(format!(
                    "request-{}-{model}-{attempt}-{turn}.json",
                    args.get(2).map(String::as_str).unwrap_or("after")
                )),
                req.canonical_bytes()?,
            )?;
            let result = drive(provider.as_ref(), req, CancellationToken::new()).await?;
            rows.push(json!({"attempt":attempt,"turn":turn,"terminal":format!("{:?}",result.terminal()),"text":text(&result),"usage":format!("{:?}",result.usage_high_water())}));
            std::fs::write(&output, rows.iter().map(|r| r.to_string() + "\n").collect::<String>())?;
            if let Some(TerminalOutcome::Failed(f)) = result.terminal() {
                failures += 1;
                println!(
                    "model={model} attempt={attempt} turn={turn} FAILED {:?} {}",
                    f.category(),
                    f.diagnostic().code()
                );
                break;
            }
            let mut assistant = vec![];
            let mut tools = vec![];
            for item in result.completed_items() {
                match item {
                    ReducedItem::Text { text, .. } => {
                        assistant.push(ContentBlock::Text(text.clone()))
                    }
                    ReducedItem::ToolCall { call, .. } => {
                        assistant.push(ContentBlock::ToolCall(call.clone()));
                        let input: serde_json::Value =
                            serde_json::from_slice(call.arguments().canonical_bytes())?;
                        let path = input["path"].as_str().ok_or("missing path")?;
                        let file = fixtures.join(path);
                        if !file.canonicalize()?.starts_with(fixtures.canonicalize()?) {
                            return Err("fixture scope escape".into());
                        }
                        actual_reads.insert(path.to_owned());
                        let content = std::fs::read_to_string(file)?;
                        let value = CanonicalJson::parse(
                            &json!({"path":path,"content":content}).to_string(),
                            JsonBounds::value(ProtocolLimits::PRODUCTION),
                        )?;
                        tools.push(ContentBlock::ToolResult(ToolResult::new(
                            call.id().clone(),
                            value,
                            false,
                        )));
                    }
                    _ => {}
                }
            }
            if tools.is_empty() {
                let answer = text(&result);
                let exact_json = if attempt % 3 == 2 {
                    serde_json::from_str::<serde_json::Value>(&answer).is_ok_and(|v| {
                        v["token"] == nonce
                            && v["windows_path"] == r"C:\Users\Admin\crime"
                            && v["label"] == "café 雪 🦀"
                    })
                } else {
                    true
                };
                if answer.contains(&nonce)
                    && actual_reads.contains("HANDOFF.md")
                    && actual_reads.contains("sim.rs")
                    && exact_json
                {
                    completions += 1
                } else {
                    failures += 1
                };
                println!(
                    "model={model} attempt={attempt} turn={turn} complete={} calls=0",
                    text(&result).contains(&nonce)
                );
                break;
            }
            println!("model={model} attempt={attempt} turn={turn} host_reads={}", tools.len());
            history.push(Message::new(Role::Assistant, assistant, ProtocolLimits::PRODUCTION)?);
            history.push(Message::new(Role::Tool, tools, ProtocolLimits::PRODUCTION)?);
        }
    }
    println!("SUMMARY model={model} completions={completions} failures={failures}");
    if failures > 0 || completions != args.get(3).and_then(|v| v.parse::<usize>().ok()).unwrap_or(8)
    {
        return Err("real workflow failed; see preserved output".into());
    }
    Ok(())
}
