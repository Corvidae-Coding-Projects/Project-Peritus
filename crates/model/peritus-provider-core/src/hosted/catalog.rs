//! Authenticated service inventory enriched by `OpenCode`'s live, credential-free metadata.

use serde_json::Value;
use std::time::Duration;

use super::HostedService;
use crate::{
    CancellationToken, Endpoint, HttpHeaders, HttpLimits, HttpMethod, HttpRequest, HttpTransport,
    ProviderCoreError,
    catalog::{CatalogDialect, DiscoveredModel, discover_http_models, unavailable},
};

/// Discovers currently advertised IDs; metadata failure leaves protocol/capabilities unknown.
///
/// `OpenCode`'s SDK metadata enriches only IDs returned by the selected service. It cannot add a
/// model, provide a credential header, redirect inference, or execute an SDK package.
///
/// # Errors
/// Returns a bounded service catalog failure; no bundled catalog replaces a failed request.
pub async fn discover_hosted_models(
    service: HostedService,
    transport: &dyn HttpTransport,
    headers: &(dyn Fn() -> Result<HttpHeaders, ProviderCoreError> + Sync),
    limits: HttpLimits,
    cancellation: &CancellationToken,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    let dialect = match service {
        HostedService::Together => CatalogDialect::Together,
        HostedService::Fireworks => CatalogDialect::Fireworks,
        _ => CatalogDialect::OpenAi,
    };
    let mut models = discover_http_models(
        transport,
        &Endpoint::new(service.models_endpoint().to_owned())?,
        dialect,
        headers,
        limits,
        cancellation,
    )
    .await?;
    if service.mixed_protocols() {
        if let Ok(Ok(metadata)) =
            tokio::time::timeout(Duration::from_secs(10), metadata(transport, limits, cancellation))
                .await
        {
            enrich(service, &mut models, &metadata)?;
        }
    } else {
        for model in &mut models {
            model.dialect = Some(peritus_model_protocol::WireDialect::CompatibleChatCompletions);
        }
    }
    if cancellation.is_cancelled() {
        return Err(ProviderCoreError::cancelled("hosted_model_discovery"));
    }
    Ok(models)
}

async fn metadata(
    transport: &dyn HttpTransport,
    limits: HttpLimits,
    cancellation: &CancellationToken,
) -> Result<Value, ProviderCoreError> {
    let request = HttpRequest::new(
        HttpMethod::Get,
        Endpoint::new("https://models.opencode.ai/api.json".to_owned())?,
        HttpHeaders::new(Vec::new(), limits)?,
        Vec::new(),
        limits,
    )?;
    let response = transport.send(request, cancellation).await?;
    if !response.status().is_success() {
        return Err(unavailable("OpenCode protocol metadata is unavailable"));
    }
    let (_, _, mut body) = response.into_parts();
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next(cancellation).await? {
        if bytes.len().saturating_add(chunk.len()) > 16 * 1024 * 1024 {
            return Err(unavailable("OpenCode metadata exceeds its byte bound"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable("OpenCode metadata is malformed"))
}

fn enrich(
    service: HostedService,
    models: &mut [DiscoveredModel],
    metadata: &Value,
) -> Result<(), ProviderCoreError> {
    use peritus_model_protocol::WireDialect;
    let key = if service == HostedService::OpenCodeGo { "opencode-go" } else { "opencode" };
    let Some(provider) = metadata.get(key) else {
        return Ok(());
    };
    for model in models {
        let Some(value) = provider.get("models").and_then(|models| models.get(model.id.as_str()))
        else {
            continue;
        };
        // Interpret a closed protocol vocabulary; never load a package or trust a metadata URL.
        let package =
            value.pointer("/provider/npm").or_else(|| provider.get("npm")).and_then(Value::as_str);
        model.dialect = match package {
            Some("@ai-sdk/openai") => Some(WireDialect::OpenAiResponses),
            Some("@ai-sdk/openai-compatible") => Some(WireDialect::CompatibleChatCompletions),
            Some("@ai-sdk/anthropic") => Some(WireDialect::AnthropicMessages),
            Some("@ai-sdk/google") => Some(WireDialect::GeminiGenerateContentV1),
            _ => None,
        };
        if let Some(dialect) = model.dialect {
            service.route(dialect)?;
        }
        model.tools = value.get("tool_call").and_then(Value::as_bool);
        model.input_tokens = value
            .pointer("/limit/input")
            .or_else(|| value.pointer("/limit/context"))
            .and_then(Value::as_u64)
            .filter(|n| *n > 0);
        model.output_tokens =
            value.pointer("/limit/output").and_then(Value::as_u64).filter(|n| *n > 0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_models_use_live_metadata_and_unknown_protocols_remain_unknown() {
        let mut models = vec![
            DiscoveredModel::new("future-model".into(), "future-model".into()).expect("model"),
            DiscoveredModel::new("unknown".into(), "unknown".into()).expect("model"),
        ];
        let metadata = serde_json::json!({"opencode":{"models":{
            "future-model":{"provider":{"npm":"@ai-sdk/anthropic", "api":"https://untrusted.invalid"}, "tool_call":true, "limit":{"context":12345,"output":321}},
            "unknown":{"provider":{"npm":"unreviewed-package"}},
            "not-in-service-catalog":{"provider":{"npm":"@ai-sdk/openai"}}
        }}});
        enrich(HostedService::OpenCodeZen, &mut models, &metadata).expect("enrich");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].dialect, Some(peritus_model_protocol::WireDialect::AnthropicMessages));
        assert_eq!(models[0].tools, Some(true));
        assert_eq!(models[0].output_tokens, Some(321));
        assert_eq!(models[1].dialect, None);
        assert_eq!(models[1].tools, None);
    }
}
