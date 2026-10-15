//! Model metadata delegated to the vendor's own SDK catalog.

use peritus_model_protocol::WireDialect;
use peritus_provider_core::{
    CancellationToken, Credential, ProcessLimits, ProcessTransport, ProviderCoreError,
    TokioProcessTransport,
    catalog::{DiscoveredModel, MAX_CATALOG_MODELS},
    hosted::HostedService,
};
use serde_json::Value;
use std::time::Duration;

/// Lists the service's advertised models using isolated `opencode models --verbose`.
///
/// Listing does not establish entitlement or prove inference/tool behavior.
///
/// # Errors
/// Returns bounded process, cancellation, credential, or malformed catalog failures.
pub async fn discover_models(
    executable: &super::OpenCodeExecutable,
    service: HostedService,
    credential: &Credential,
    cancellation: &CancellationToken,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    let service = super::service_id(service)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(45);
    let private = super::isolation::Isolation::new(
        service,
        credential,
        "Peritus inference transport. Never invoke tools.",
    )?;
    executable.require_supported(&private, cancellation).await?;
    // A live replay could not obtain a complete native listing once. Retry this
    // metadata-only command once, sharing private vendor state within the same deadline.
    // Never substitute a catalog or retry an inference here.
    for attempt in 0..2 {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(unavailable());
        }
        let limits = ProcessLimits::new(1, 4 * 1024 * 1024, 64 * 1024, remaining)?;
        let process = private.process(
            executable,
            vec!["models".into(), service.into(), "--verbose".into()],
            Vec::new(),
            limits,
        )?;
        let output = TokioProcessTransport.run(process, cancellation).await?;
        if output.exit().success() {
            match parse(output.stdout(), service) {
                Ok(models) => return Ok(models),
                Err(error) if attempt == 1 => return Err(error),
                Err(_) => {}
            }
        } else if attempt == 1 {
            return Err(unavailable());
        }
    }
    Err(unavailable())
}

fn parse(bytes: &[u8], service: &str) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    let mut text = std::str::from_utf8(bytes).map_err(|_| unavailable())?.trim();
    let mut models = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while !text.is_empty() {
        let (name, rest) = text.split_once('\n').ok_or_else(unavailable)?;
        let id = name
            .trim()
            .strip_prefix(service)
            .and_then(|id| id.strip_prefix('/'))
            .ok_or_else(unavailable)?;
        let mut decoder = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
        let value = decoder.next().ok_or_else(unavailable)?.map_err(|_| unavailable())?;
        text = rest.get(decoder.byte_offset()..).ok_or_else(unavailable)?.trim();
        if value.get("id").and_then(Value::as_str) != Some(id)
            || value.get("providerID").and_then(Value::as_str) != Some(service)
            || !seen.insert(id.to_owned())
            || models.len() >= MAX_CATALOG_MODELS
        {
            return Err(unavailable());
        }
        let label = value.get("name").and_then(Value::as_str).unwrap_or(id);
        let mut model = DiscoveredModel::new(id.to_owned(), label.to_owned())?;
        model.dialect = Some(WireDialect::OpenCodeRuntime);
        model.tools = value.pointer("/capabilities/toolcall").and_then(Value::as_bool);
        model.output_tokens =
            value.pointer("/limit/output").and_then(Value::as_u64).filter(|value| *value > 0);
        model.input_tokens = value
            .pointer("/limit/input")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .or_else(|| {
                value
                    .pointer("/limit/context")
                    .and_then(Value::as_u64)
                    .and_then(|context| context.checked_sub(model.output_tokens.unwrap_or(0)))
                    .filter(|value| *value > 0)
            });
        models.push(model);
    }
    if models.is_empty() {
        return Err(unavailable());
    }
    Ok(models)
}

fn unavailable() -> ProviderCoreError {
    peritus_provider_core::catalog::unavailable(
        "OpenCode did not return a complete service model catalog; install a supported official CLI and retry",
    )
}
