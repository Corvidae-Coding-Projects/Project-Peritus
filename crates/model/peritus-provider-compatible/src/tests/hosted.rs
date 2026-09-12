//! Synthetic SDK/API contract fixtures; provenance: docs/provider-contracts.md (2026-09-10).

use super::support::{StaticCredential, block_on, chat_profile, credential_reference};
use crate::{CompatibleAuth, CompatibleClient, CompatibleConfig, CompatibleProfile};
use peritus_model_protocol::{Capability, ModelEvent, ProtocolLimits, WireDialect};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Endpoint, Header, HeaderName, HttpHeaders, HttpLimits,
    HttpRequest, HttpResponse, HttpTransport, MemoryByteStream, ProviderCoreError, StatusCode,
    connection::{ConnectionStage, verify_provider_connection},
    hosted::HostedService,
};
use serde_json::Value;
use std::{
    fmt::Write as _,
    sync::{Arc, Mutex},
};

struct ContractTransport {
    service: HostedService,
    requests: Mutex<Vec<Value>>,
    fail_at: Option<usize>,
}

impl HttpTransport for ContractTransport {
    fn send<'a>(
        &'a self,
        request: HttpRequest,
        _: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<HttpResponse, ProviderCoreError>> {
        Box::pin(async move {
            assert_eq!(
                request.endpoint().as_str(),
                self.service.route(WireDialect::CompatibleChatCompletions).expect("route").endpoint
            );
            let body: Value = serde_json::from_slice(request.body()).expect("request JSON");
            let step = {
                let mut requests = self.requests.lock().expect("requests");
                requests.push(body.clone());
                requests.len()
            };
            assert_eq!(body["stream"], true);
            let token_field = if self.service == HostedService::Groq {
                "max_completion_tokens"
            } else {
                "max_tokens"
            };
            assert!(body[token_field].as_u64().is_some_and(|limit| limit <= 512));
            assert!(body.get("response_format").is_none());
            assert!(body["messages"][0]["content"].is_string());
            if step == 2 {
                assert_eq!(body["tool_choice"], "auto");
                assert_eq!(body["tools"].as_array().expect("tools").len(), 1);
                assert_eq!(body["tools"][0]["function"]["name"], "peritus_connection_check");
                assert!(body["tools"][0]["function"].get("strict").is_none());
            }
            if step == 3 {
                let assistant = &body["messages"][1];
                assert_eq!(assistant["tool_calls"][0]["id"], "check-call");
                assert!(
                    assistant.get(reasoning_field(self.service)).is_some(),
                    "reasoning must survive tool-result replay"
                );
                assert!(
                    body["messages"][2]["content"]
                        .as_str()
                        .expect("result")
                        .contains("peritus-connection-ok")
                );
            }
            let failed = self.fail_at == Some(step);
            let bytes = if failed {
                br#"{"error":{"message":"secret-provider-body","code":"insufficient_quota"}}"#
                    .to_vec()
            } else {
                success(self.service, step)
            };
            let limits = HttpLimits::PRODUCTION;
            HttpResponse::new(
                StatusCode::new(if failed { 402 } else { 200 })?,
                HttpHeaders::new(
                    vec![Header::new(
                        HeaderName::new("content-type".to_owned())?,
                        if failed {
                            b"application/json".to_vec()
                        } else {
                            b"text/event-stream".to_vec()
                        },
                    )?],
                    limits,
                )?,
                Box::new(MemoryByteStream::new(
                    bytes.chunks(11).map(<[u8]>::to_vec).collect(),
                    limits,
                )?),
                limits,
            )
        })
    }
}

fn reasoning_field(service: HostedService) -> &'static str {
    match service {
        HostedService::OpenRouter => "reasoning_details",
        HostedService::Groq => "reasoning",
        _ => "reasoning_content",
    }
}

fn chunk(step: usize, delta: Value, finish: Value) -> Value {
    let mut value = serde_json::json!({"id":format!("request-{step}"),"object":"chat.completion.chunk","created":1,"model":"resolved-model-version","choices":[{"index":0,"delta":null,"finish_reason":null}]});
    value["choices"][0]["delta"] = delta;
    value["choices"][0]["finish_reason"] = finish;
    value
}

fn success(service: HostedService, step: usize) -> Vec<u8> {
    let mut chunks =
        vec![chunk(step, serde_json::json!({"role":"assistant","content":""}), Value::Null)];
    if step == 2 {
        let reasoning = if service == HostedService::OpenRouter {
            serde_json::json!([{"type":"reasoning.text","text":"checked ","index":0,"id":"r0","format":"openai-responses-v1"}])
        } else {
            serde_json::json!("checked ")
        };
        chunks.push(chunk(
            step,
            serde_json::json!({reasoning_field(service):reasoning}),
            Value::Null,
        ));
        if service == HostedService::OpenRouter {
            chunks.push(chunk(step, serde_json::json!({"reasoning_details":[{"type":"reasoning.text","text":"tool","index":0,"id":"r0","format":"openai-responses-v1"}]}), Value::Null));
        }
        chunks.push(chunk(step, serde_json::json!({"tool_calls":[{"index":0,"id":"check-call","type":"function","function":{"name":"peritus_connection_check","arguments":"{}"}}]}), Value::Null));
    } else {
        chunks.push(chunk(
            step,
            serde_json::json!({"content":if step == 1 { "ok" } else { "peritus-connection-ok" }}),
            Value::Null,
        ));
    }
    if service == HostedService::OpenCodeZen {
        // Zen may stream more than one cumulative usage snapshot before its terminal accounting.
        let mut usage = chunk(step, serde_json::json!({}), Value::Null);
        usage["usage"] =
            serde_json::json!({"prompt_tokens":1,"completion_tokens":1,"total_tokens":2});
        chunks.push(usage);
    }
    let reason = if step == 2 { "tool_calls" } else { "stop" };
    let mut last = chunk(step, serde_json::json!({}), serde_json::json!(reason));
    let usage = serde_json::json!({"prompt_tokens":2,"completion_tokens":3,"total_tokens":5});
    if service == HostedService::Groq {
        last["x_groq"] = serde_json::json!({"id":"groq-request","usage":usage});
    } else if service != HostedService::OpenRouter {
        last["usage"] = usage.clone();
    }
    chunks.push(last);
    if service == HostedService::OpenRouter {
        let mut accounting =
            chunk(step, serde_json::json!({"content":""}), serde_json::json!(reason));
        accounting["choices"][0]["native_finish_reason"] = serde_json::json!(reason);
        accounting["provider"] = serde_json::json!("fixture-vendor");
        accounting["usage"] = usage;
        chunks.push(accounting);
    }
    let mut text = String::new();
    for chunk in chunks {
        writeln!(text, "data: {chunk}\n").expect("fixture formatting");
    }
    text.push_str("data: [DONE]\n\n");
    text.into_bytes()
}

fn client(
    service: HostedService,
    fail_at: Option<usize>,
) -> (CompatibleClient, Arc<ContractTransport>) {
    let transport =
        Arc::new(ContractTransport { service, fail_at, requests: Mutex::new(Vec::new()) });
    let config = CompatibleConfig::new(
        Endpoint::new(
            service
                .route(WireDialect::CompatibleChatCompletions)
                .expect("route")
                .endpoint
                .to_owned(),
        )
        .expect("endpoint"),
        CompatibleAuth::bearer(credential_reference()).expect("auth"),
    )
    .expect("config")
    .with_hosted_service(service)
    .expect("hosted");
    let profile = CompatibleProfile::hosted_chat_completions(chat_profile(&[
        Capability::Streaming,
        Capability::ToolCalls,
        Capability::UsageDetail,
        Capability::ReasoningReplay,
    ]))
    .expect("profile");
    (
        CompatibleClient::with_transport(
            config,
            profile,
            Arc::new(StaticCredential::new()),
            transport.clone(),
        ),
        transport,
    )
}

#[test]
fn every_named_chat_contract_completes_generation_tools_usage_and_reasoning_replay() {
    block_on(async {
        for service in HostedService::ALL {
            let (client, transport) = client(service, None);
            let report = verify_provider_connection(&client, CancellationToken::new())
                .await
                .unwrap_or_else(|error| panic!("{service:?}: {error}"));
            assert_eq!(report.completed.len(), 3);
            assert_eq!(transport.requests.lock().expect("requests").len(), 3);
        }
    });
}

#[test]
fn connection_failures_identify_the_stage_and_preserve_http_without_provider_body() {
    block_on(async {
        for (step, stage) in [
            (1, ConnectionStage::Generation),
            (2, ConnectionStage::ToolCalling),
            (3, ConnectionStage::ToolResult),
        ] {
            let (client, _) = client(HostedService::DeepSeek, Some(step));
            let error = verify_provider_connection(&client, CancellationToken::new())
                .await
                .expect_err("rejected");
            assert_eq!(error.stage, stage);
            assert!(error.to_string().contains("HTTP 402"));
            assert!(!format!("{error:?}").contains("secret-provider-body"));
        }
    });
}

#[test]
fn hosted_contract_cannot_be_attached_to_an_unrelated_endpoint() {
    let config = CompatibleConfig::new(
        Endpoint::new("https://unrelated.invalid/v1/chat/completions".to_owned())
            .expect("endpoint"),
        CompatibleAuth::bearer(credential_reference()).expect("auth"),
    )
    .expect("config");
    assert!(config.with_hosted_service(HostedService::OpenRouter).is_err());
}

#[test]
fn openrouter_error_with_http_200_never_qualifies_a_connection() {
    block_on(async {
        let body = MemoryByteStream::new(
            vec![
                b"data: {\"error\":{\"code\":402,\"message\":\"secret-provider-body\"}}\n\n"
                    .to_vec(),
            ],
            HttpLimits::PRODUCTION,
        )
        .expect("body");
        let profile = chat_profile(&[Capability::Streaming]);
        let stream = crate::stream::CompatibleStream::new(
            Box::new(body),
            peritus_provider_core::FramingLimits::PRODUCTION,
            profile.provider().clone(),
            profile.model().clone(),
            WireDialect::CompatibleChatCompletions,
            false,
            false,
            false,
            ProtocolLimits::PRODUCTION,
            Vec::new(),
        )
        .expect("stream")
        .with_hosted_service(Some(HostedService::OpenRouter));
        let mut stream =
            peritus_provider_core::OwnedModelStream::new(stream, CancellationToken::new());
        let event = stream.pull().await.expect("pull").expect("failure");
        assert!(
            matches!(event.event(), ModelEvent::ResponseFailed(failure) if failure.category() == peritus_model_protocol::FailureCategory::QuotaExhausted)
        );
        assert!(!format!("{event:?}").contains("secret-provider-body"));
        assert!(stream.pull().await.expect("end").is_none());
    });
}

#[test]
fn openrouter_accounting_cannot_carry_output_unknown_fields_or_repeated_finishes() {
    block_on(async {
        for mutation in 0..5 {
            let mut accounting =
                chunk(1, serde_json::json!({"content":""}), serde_json::json!("stop"));
            accounting["usage"] =
                serde_json::json!({"prompt_tokens":2,"completion_tokens":3,"total_tokens":5});
            match mutation {
                0 => {
                    accounting["choices"][0]["delta"]["content"] = serde_json::json!("late output");
                }
                1 => accounting["choices"][0]["finish_reason"] = serde_json::json!("length"),
                2 => accounting["choices"][0]["unmapped"] = serde_json::json!(true),
                3 => accounting["choices"][0]["delta"]["tool_calls"] = serde_json::json!([]),
                _ => {}
            }
            let mut bytes = format!(
                "data: {}\n\ndata: {}\n\ndata: {accounting}\n\n",
                chunk(1, serde_json::json!({"content":"ok"}), Value::Null),
                chunk(1, serde_json::json!({}), serde_json::json!("stop"))
            );
            if mutation == 4 {
                writeln!(bytes, "data: {accounting}\n").expect("fixture formatting");
            }
            bytes.push_str("data: [DONE]\n\n");
            let profile = chat_profile(&[Capability::Streaming, Capability::UsageDetail]);
            let stream = crate::stream::CompatibleStream::new(
                Box::new(
                    MemoryByteStream::new(vec![bytes.into_bytes()], HttpLimits::PRODUCTION)
                        .expect("body"),
                ),
                peritus_provider_core::FramingLimits::PRODUCTION,
                profile.provider().clone(),
                profile.model().clone(),
                WireDialect::CompatibleChatCompletions,
                false,
                false,
                true,
                ProtocolLimits::PRODUCTION,
                Vec::new(),
            )
            .expect("stream")
            .with_hosted_service(Some(HostedService::OpenRouter));
            let mut stream =
                peritus_provider_core::OwnedModelStream::new(stream, CancellationToken::new());
            let mut failed = false;
            while let Some(event) = stream.pull().await.expect("pull") {
                assert!(!matches!(event.event(), ModelEvent::ResponseCompleted));
                failed |= matches!(event.event(), ModelEvent::ResponseFailed(_));
            }
            assert!(failed, "mutation {mutation}");
        }
    });
}

#[test]
fn hosted_required_tool_choice_rejects_missing_or_different_calls() {
    block_on(async {
        for step in [1, 2] {
            let profile = chat_profile(&[
                Capability::Streaming,
                Capability::ToolCalls,
                Capability::UsageDetail,
            ]);
            let stream = crate::stream::CompatibleStream::new(
                Box::new(
                    MemoryByteStream::new(
                        vec![success(HostedService::DeepSeek, step)],
                        HttpLimits::PRODUCTION,
                    )
                    .expect("body"),
                ),
                peritus_provider_core::FramingLimits::PRODUCTION,
                profile.provider().clone(),
                profile.model().clone(),
                WireDialect::CompatibleChatCompletions,
                false,
                true,
                true,
                ProtocolLimits::PRODUCTION,
                Vec::new(),
            )
            .expect("stream")
            .with_hosted_service(Some(HostedService::DeepSeek))
            .with_tool_choice(peritus_model_protocol::ToolChoice::Specific(
                peritus_model_protocol::ToolName::new("different_required_tool".to_owned())
                    .expect("name"),
            ));
            let mut stream =
                peritus_provider_core::OwnedModelStream::new(stream, CancellationToken::new());
            let mut failed = false;
            while let Some(event) = stream.pull().await.expect("pull") {
                assert!(!matches!(event.event(), ModelEvent::ResponseCompleted));
                failed |= matches!(event.event(), ModelEvent::ResponseFailed(_));
            }
            assert!(failed, "step {step}");
        }
    });
}
