//! `OpenRouter` error and terminal-accounting stream regressions.

use super::chunk;
use crate::tests::support::{block_on, chat_profile};
use peritus_model_protocol::{Capability, ModelEvent, ProtocolLimits, WireDialect};
use peritus_provider_core::{
    CancellationToken, HttpLimits, MemoryByteStream, hosted::HostedService,
};
use serde_json::Value;
use std::fmt::Write as _;

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
