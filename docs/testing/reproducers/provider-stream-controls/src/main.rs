//! Public loopback adapter controls for cancellation and exact event retransmission.
use peritus_model_protocol::*;
use peritus_provider_anthropic::{AnthropicClient, AnthropicConfig};
use peritus_provider_compatible::{
    CompatibleAuth, CompatibleClient, CompatibleConfig, CompatibleProfile,
};
use peritus_provider_core::*;
use peritus_provider_google::{GoogleClient, GoogleConfig};
use peritus_types::ProviderProfileId;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
enum Mode {
    Anthropic,
    Google,
    Chat,
}
struct SyntheticCredential;
impl CredentialSource for SyntheticCredential {
    fn resolve(&self, _: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        Credential::new(b"fixture-only-not-a-user-key".to_vec())
    }
}
fn request(profile: &ProviderProfile, tool: bool) -> Result<ModelRequest, String> {
    let l = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::Streaming, Capability::ToolCalls],
            &[],
            profile.limits(),
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let tooldef=ToolDefinition::new(ToolName::new("lookup".into()).map_err(|e|e.to_string())?,None,JsonSchema::parse(r#"{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false}"#,SchemaDialect::Draft202012,JsonBounds::schema(l)).map_err(|e|e.to_string())?,false);
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("regression".into()).map_err(|e| e.to_string())?,
        vec![
            Message::new(
                Role::User,
                vec![ContentBlock::Text(
                    BoundedText::new("Return fixture text or inert call.".into(), l)
                        .map_err(|e| e.to_string())?,
                )],
                l,
            )
            .map_err(|e| e.to_string())?,
        ],
        vec![tooldef],
        if tool {
            ToolChoice::Specific(ToolName::new("lookup".into()).map_err(|e| e.to_string())?)
        } else {
            ToolChoice::None
        },
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(256, vec![], None, None, None).map_err(|e| e.to_string())?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            vec![],
        ),
        l,
    )
    .map_err(|e| e.to_string())
}
fn wire(mode: Mode, duplicate: bool) -> (String, String) {
    match mode{
 Mode::Anthropic=>(concat!("event: message_start\ndata: ",r#"{"type":"message_start","message":{"id":"r","type":"message","role":"assistant","model":"fixture-model","content":[],"usage":{"input_tokens":7,"output_tokens":0}}}"#,"\n\n"),concat!("event: content_block_start\ndata: ",r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,"\n\nevent: content_block_delta\ndata: ",r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"fixture-distinct"}}"#,"\n\nevent: content_block_stop\ndata: ",r#"{"type":"content_block_stop","index":0}"#,"\n\nevent: message_delta\ndata: ",r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}"#,"\n\nevent: message_stop\ndata: ",r#"{"type":"message_stop"}"#,"\n\n")).map_into(),
 Mode::Google=>(concat!("data: ",r#"{"responseId":"r","modelVersion":"fixture-model","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"fixture-distinct"}]}}]}"#,"\n\n").into(),concat!("data: ",r#"{"responseId":"r","modelVersion":"fixture-model","candidates":[{"index":0,"finishReason":"STOP"}]}"#,"\n\n").into()),
 Mode::Chat=>{let first=concat!("id: first\ndata: ",r#"{"id":"r","object":"chat.completion.chunk","model":"fixture-model","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call-1","type":"function","function":{"name":"lookup","arguments":"{\"value\":"}}]},"finish_reason":null}]}"#,"\n\n");let delta=concat!("id: repeated-fragment\ndata: ",r#"{"id":"r","object":"chat.completion.chunk","model":"fixture-model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"fixture-distinct\"}"}}]},"finish_reason":null}]}"#,"\n\n");let tail=concat!("id: finish\ndata: ",r#"{"id":"r","object":"chat.completion.chunk","model":"fixture-model","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,"\n\ndata: [DONE]\n\n");(format!("{first}{delta}{}{tail}",if duplicate{delta}else{""}),String::new())}
 }
}
trait MapInto {
    fn map_into(self) -> (String, String);
}
impl MapInto for (&str, &str) {
    fn map_into(self) -> (String, String) {
        (self.0.into(), self.1.into())
    }
}
async fn observe(mode: Mode, altered: bool) -> Result<(), String> {
    let listener =
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    let endpoint =
        Endpoint::new(format!("http://{}", listener.local_addr().map_err(|e| e.to_string())?))
            .map_err(|e| e.to_string())?;
    let (prefix, tail) = wire(mode, altered);
    let server = std::thread::spawn(move || -> Result<(), String> {
        let (mut socket, _) = listener.accept().map_err(|e| e.to_string())?;
        socket.set_read_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
        socket.set_write_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            socket.read_exact(&mut b).map_err(|e| e.to_string())?;
            headers.push(b[0]);
            if headers.len() > 65536 {
                return Err("headers too large".into());
            }
        }
        let headers = String::from_utf8(headers).map_err(|e| e.to_string())?;
        let size = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::trim)
                    .map(str::to_owned)
            })
            .ok_or("missing content length")?
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        if size > 65536 {
            return Err("body too large".into());
        }
        let mut body = vec![0; size];
        socket.read_exact(&mut body).map_err(|e| e.to_string())?;
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",prefix.len()+tail.len()).as_bytes()).and_then(|()|socket.write_all(prefix.as_bytes())).map_err(|e|e.to_string())?;
        std::thread::sleep(Duration::from_millis(100));
        let _ = socket.write_all(tail.as_bytes());
        Ok(())
    });
    let (name, dialect) = match mode {
        Mode::Anthropic => ("anthropic", WireDialect::AnthropicMessages),
        Mode::Google => ("google", WireDialect::GeminiGenerateContentV1),
        Mode::Chat => ("compatible", WireDialect::CompatibleChatCompletions),
    };
    let profile = ProviderProfile::new(
        ProviderProfileId::new([56; 16]).map_err(|e| format!("{e:?}"))?,
        1,
        ProviderName::new(name.into()).map_err(|e| e.to_string())?,
        ModelName::new("fixture-model".into()).map_err(|e| e.to_string())?,
        dialect,
        CapabilityMatrix::new(&[Capability::Streaming, Capability::ToolCalls], &[])
            .map_err(|e| e.to_string())?,
        CapabilityProvenance::Probed,
        ModelLimits::new(10000, 4096, 16, 4, 1).map_err(|e| e.to_string())?,
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .map_err(|e| e.to_string())?;
    let credential = CredentialReference::new("synthetic".into()).map_err(|e| e.to_string())?;
    let retry =
        RetryPolicy::new(1, [Duration::from_millis(1); 4], 1048576).map_err(|e| e.to_string())?;
    let provider: Arc<dyn ModelProvider> = match mode {
        Mode::Anthropic => Arc::new(
            AnthropicClient::new(
                AnthropicConfig::new(
                    endpoint,
                    credential,
                    profile,
                    vec![],
                    HttpLimits::PRODUCTION,
                    FramingLimits::PRODUCTION,
                    retry,
                )
                .map_err(|e| e.to_string())?,
                Box::new(SyntheticCredential),
            )
            .map_err(|e| e.to_string())?,
        ),
        Mode::Google => Arc::new(
            GoogleClient::new(
                GoogleConfig::new(
                    endpoint,
                    credential,
                    profile,
                    HttpLimits::PRODUCTION,
                    FramingLimits::PRODUCTION,
                    retry,
                )
                .map_err(|e| e.to_string())?,
                Box::new(SyntheticCredential),
            )
            .map_err(|e| e.to_string())?,
        ),
        Mode::Chat => Arc::new(
            CompatibleClient::new(
                CompatibleConfig::new(
                    endpoint.with_path("/v1/chat/completions").map_err(|e| e.to_string())?,
                    CompatibleAuth::bearer(credential).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?,
                CompatibleProfile::chat_completions(profile).map_err(|e| e.to_string())?,
                Arc::new(SyntheticCredential),
            )
            .map_err(|e| e.to_string())?,
        ),
    };
    let cancellation = CancellationToken::new();
    let mut reducer =
        ResponseReducer::new(provider.profile().provider().clone(), ProtocolLimits::PRODUCTION);
    let mut stream = provider
        .start(request(provider.profile(), matches!(mode, Mode::Chat))?, cancellation.clone())
        .await
        .map_err(|e| e.to_string())?;
    let mut pulls = 0;
    let mut rejection = None;
    while let Some(envelope) = stream.pull().await.map_err(|e| e.to_string())? {
        pulls += 1;
        let transition = reducer.push(envelope.clone());
        println!(
            "mode={mode:?} altered={altered} seq={} event={:?} transition={transition:?}",
            envelope.sequence(),
            envelope.event()
        );
        if let Err(error) = transition {
            rejection = Some(error.to_string());
            break;
        }
        if altered && !matches!(mode, Mode::Chat) && pulls == 1 {
            let _ = cancellation.cancel();
        }
    }
    server.join().map_err(|_| "server panicked")??;
    if let Some(error) = rejection {
        return Err(error);
    }
    if altered && !matches!(mode, Mode::Chat) {
        if !matches!(reducer.terminal(), Some(TerminalOutcome::Cancelled { .. })) {
            return Err(format!("cancel terminal missing: {:?}", reducer.terminal()));
        }
    } else if matches!(mode, Mode::Chat) {
        let calls = reducer
            .completed_items()
            .iter()
            .filter_map(|item| {
                if let ReducedItem::ToolCall { call, .. } = item { Some(call) } else { None }
            })
            .collect::<Vec<_>>();
        if calls.len() != 1
            || serde_json::from_slice::<serde_json::Value>(calls[0].arguments().canonical_bytes())
                .map_err(|e| e.to_string())?
                != serde_json::json!({"value":"fixture-distinct"})
            || !matches!(reducer.terminal(), Some(TerminalOutcome::RequiresAction { .. }))
        {
            return Err(format!("complete exact tool call lost: {:?}", reducer.terminal()));
        }
    } else if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
        return Err("ordinary completion lost".into());
    }
    Ok(())
}
fn main() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let mut failed = 0;
    for mode in [Mode::Anthropic, Mode::Google, Mode::Chat] {
        for altered in [false, true] {
            let result = runtime.block_on(observe(mode, altered));
            println!("RESULT {mode:?} altered={altered}: {result:?}");
            failed += usize::from(result.is_err());
        }
    }
    if failed > 0 { Err(format!("{failed} public stream controls failed")) } else { Ok(()) }
}
