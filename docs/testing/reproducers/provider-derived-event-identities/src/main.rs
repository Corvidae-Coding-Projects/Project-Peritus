//! Real loopback adapters and the ordinary reducer must retain every derived output event.

use peritus_model_protocol::*;
use peritus_provider_anthropic::{AnthropicClient, AnthropicConfig};
use peritus_provider_core::*;
use peritus_provider_google::{GoogleClient, GoogleConfig};
use peritus_types::ProviderProfileId;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};

struct SyntheticCredential;
impl CredentialSource for SyntheticCredential {
    fn resolve(&self, _: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        Credential::new(b"fixture-only-not-a-user-key".to_vec())
    }
}

fn request(profile: &ProviderProfile) -> Result<ModelRequest, String> {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::Streaming],
            &[Capability::UsageDetail],
            profile.limits(),
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("regression".into()).map_err(|e| e.to_string())?,
        vec![
            Message::new(
                Role::User,
                vec![ContentBlock::Text(
                    BoundedText::new("Return fixture text.".into(), limits)
                        .map_err(|e| e.to_string())?,
                )],
                limits,
            )
            .map_err(|e| e.to_string())?,
        ],
        vec![],
        ToolChoice::None,
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
        limits,
    )
    .map_err(|e| e.to_string())
}

fn wire(google: bool, ids: bool) -> String {
    let frames = if google {
        vec![
            (
                "interaction.created",
                r#"{"event_type":"interaction.created","interaction":{"id":"response-fixture","model":"fixture-model"}}"#,
            ),
            (
                "step.start",
                r#"{"event_type":"step.start","index":0,"step":{"type":"model_output"}}"#,
            ),
            (
                "step.delta",
                r#"{"event_type":"step.delta","index":0,"delta":{"type":"text","text":"fixture-distinct"}}"#,
            ),
            ("step.stop", r#"{"event_type":"step.stop","index":0}"#),
            (
                "interaction.completed",
                r#"{"event_type":"interaction.completed","interaction":{"id":"response-fixture","status":"completed","usage":{"total_input_tokens":7,"total_output_tokens":3,"total_tokens":10}}}"#,
            ),
        ]
    } else {
        vec![
            (
                "message_start",
                r#"{"type":"message_start","message":{"id":"response-fixture","type":"message","role":"assistant","model":"fixture-model","content":[],"usage":{"input_tokens":7,"output_tokens":0}}}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"fixture-distinct"}}"#,
            ),
            ("content_block_stop", r#"{"type":"content_block_stop","index":0}"#),
            (
                "message_delta",
                r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}"#,
            ),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ]
    };
    frames
        .iter()
        .enumerate()
        .map(|(index, (event, data))| {
            format!(
                "{}event: {event}\ndata: {data}\n\n",
                if ids { format!("id: unique-event-{index}\n") } else { String::new() }
            )
        })
        .collect()
}

async fn observe(google: bool, ids: bool) -> Result<(), String> {
    let listener =
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    let origin =
        Endpoint::new(format!("http://{}", listener.local_addr().map_err(|e| e.to_string())?))
            .map_err(|e| e.to_string())?;
    let bytes = wire(google, ids).into_bytes();
    let server = std::thread::spawn(move || -> Result<(), String> {
        let (mut socket, _) = listener.accept().map_err(|e| e.to_string())?;
        socket.set_read_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
        socket.set_write_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).map_err(|e| e.to_string())?;
            headers.push(byte[0]);
            if headers.len() > 64 * 1024 {
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
            .ok_or("missing body length")?
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        if size > 64 * 1024 {
            return Err("body too large".into());
        }
        let mut body = vec![0; size];
        socket.read_exact(&mut body).map_err(|e| e.to_string())?;
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).as_bytes()).and_then(|()|socket.write_all(&bytes)).map_err(|e|e.to_string())?;
        Ok(())
    });
    let profile = ProviderProfile::new(
        ProviderProfileId::new([55; 16]).map_err(|e| format!("{e:?}"))?,
        1,
        ProviderName::new(if google { "google" } else { "anthropic" }.into())
            .map_err(|e| e.to_string())?,
        ModelName::new("fixture-model".into()).map_err(|e| e.to_string())?,
        if google { WireDialect::GeminiInteractionsV1 } else { WireDialect::AnthropicMessages },
        CapabilityMatrix::new(&[Capability::Streaming, Capability::UsageDetail], &[])
            .map_err(|e| e.to_string())?,
        CapabilityProvenance::Probed,
        ModelLimits::new(10_000, 4096, 16, 4, 1).map_err(|e| e.to_string())?,
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .map_err(|e| e.to_string())?;
    let credential = CredentialReference::new("synthetic".into()).map_err(|e| e.to_string())?;
    let retry = RetryPolicy::new(1, [Duration::from_millis(1); 4], 1024 * 1024)
        .map_err(|e| e.to_string())?;
    let provider: Arc<dyn ModelProvider> = if google {
        Arc::new(
            GoogleClient::new(
                GoogleConfig::new(
                    origin,
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
        )
    } else {
        Arc::new(
            AnthropicClient::new(
                AnthropicConfig::new(
                    origin,
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
        )
    };
    let mut reducer =
        ResponseReducer::new(provider.profile().provider().clone(), ProtocolLimits::PRODUCTION);
    let mut stream = provider
        .start(request(provider.profile())?, CancellationToken::new())
        .await
        .map_err(|e| e.to_string())?;
    let mut error = None;
    while let Some(event) = stream.pull().await.map_err(|e| e.to_string())? {
        let transition = reducer.push(event.clone());
        println!(
            "provider={} ids={ids} seq={} id={:?} event={:?} transition={transition:?}",
            if google { "google" } else { "anthropic" },
            event.sequence(),
            event.provider_event_id(),
            event.event()
        );
        if let Err(failure) = transition {
            error = Some(failure.to_string());
            break;
        }
    }
    server.join().map_err(|_| "server panic")??;
    if let Some(error) = error {
        return Err(error);
    }
    let text = reducer
        .completed_items()
        .iter()
        .filter_map(|item| {
            if let ReducedItem::Text { text, .. } = item {
                Some(text.expose_for_wire())
            } else {
                None
            }
        })
        .collect::<String>();
    if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. }))
        || text != "fixture-distinct"
    {
        return Err(format!(
            "lost usable completion: terminal={:?}, text={text:?}",
            reducer.terminal()
        ));
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let mut failed = 0;
    for google in [false, true] {
        for ids in [false, true] {
            let result = runtime.block_on(observe(google, ids));
            println!("RESULT google={google} ids={ids}: {result:?}");
            if result.is_err() {
                failed += 1;
            }
        }
    }
    if failed > 0 {
        Err(format!("{failed} valid streams lost required derived events"))
    } else {
        Ok(())
    }
}
