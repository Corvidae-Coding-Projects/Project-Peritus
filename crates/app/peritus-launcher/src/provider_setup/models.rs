//! Provider-owned model menus shared by account and direct setup.

use std::future::Future;

use crate::{LauncherError, terminal::Terminal};
use peritus_model_protocol::{ModelName, WireDialect};
use peritus_product_state::{CompatibleProtocol, ProviderSelection};
use peritus_provider_core::{
    CancellationToken,
    catalog::DiscoveredModel,
    hosted::HostedService,
};
use peritus_provider_onboarding::{AccountProvider, OnboardingError};

pub(super) fn choose_direct(
    terminal: &mut Terminal<'_>,
    kind: peritus_product_state::ProviderKind,
    result: Result<Vec<peritus_provider_core::catalog::DiscoveredModel>, OnboardingError>,
) -> Result<(String, Option<CompatibleProtocol>), LauncherError> {
    let selected = choose(terminal, result)?;
    let metadata = selected.discovered.as_ref();
    terminal.line(match metadata.and_then(|entry| entry.tools) {
        Some(true) => "Tool calling: advertised by provider; connection not tested.",
        Some(false) => "Tool calling: provider advertises no support for this model.",
        None => "Tool calling: unknown; connection not tested.",
    })?;
    if let Some(metadata) = metadata {
        match (metadata.input_tokens, metadata.output_tokens) {
            (Some(input), Some(output)) => terminal.line(&format!(
                "Advertised token limits: {input} input, {output} output."
            ))?,
            (Some(input), None) => {
                terminal.line(&format!("Advertised input-token limit: {input}."))?
            }
            (None, Some(output)) => {
                terminal.line(&format!("Advertised output-token limit: {output}."))?
            }
            (None, None) => {}
        }
    }
    if selected.manual {
        terminal.line("This exact model ID was entered manually; availability remains unverified.")?;
    }
    let model = selected.id;
    if kind.hosted_service().is_none() {
        return Ok((model, None));
    }
    let service = kind
        .hosted_service()
        .and_then(HostedService::parse)
        .ok_or_else(|| LauncherError::Provider(OnboardingError::UnsupportedProvider))?;
    let protocol = match metadata.and_then(|entry| entry.dialect) {
        Some(WireDialect::CompatibleResponses | WireDialect::OpenAiResponses) => {
            CompatibleProtocol::Responses
        }
        Some(WireDialect::CompatibleChatCompletions) => CompatibleProtocol::ChatCompletions,
        Some(WireDialect::AnthropicMessages) => CompatibleProtocol::AnthropicMessages,
        Some(WireDialect::GeminiGenerateContentV1) => CompatibleProtocol::GoogleGenerateContent,
        _ if !matches!(
            kind,
            peritus_product_state::ProviderKind::OpenCodeZen
                | peritus_product_state::ProviderKind::OpenCodeGo
        ) =>
        {
            CompatibleProtocol::ChatCompletions
        }
        _ => {
            terminal.line(
                "Protocol metadata is unavailable for this model. Choose its documented API:",
            )?;
            terminal.line("1. Responses  2. Chat Completions  3. Anthropic Messages  4. Google Generate Content")?;
            loop {
                match terminal.prompt("Protocol: ")?.as_str() {
                    "1" => break CompatibleProtocol::Responses,
                    "2" => break CompatibleProtocol::ChatCompletions,
                    "3" => break CompatibleProtocol::AnthropicMessages,
                    "4" => break CompatibleProtocol::GoogleGenerateContent,
                    _ => terminal.line("Choose 1, 2, 3, or 4.")?,
                }
            }
        }
    };
    validate_hosted_protocol(service, protocol)?;
    Ok((model, Some(protocol)))
}

fn choose(
    terminal: &mut Terminal<'_>,
    result: Result<Vec<DiscoveredModel>, OnboardingError>,
) -> Result<SelectedModel, LauncherError> {
    let models = match result {
        Ok(models) => models,
        Err(error @ OnboardingError::Cancelled) => return Err(error.into()),
        Err(error) => {
            terminal.line(&error.to_string())?;
            Vec::new()
        }
    };
    for (index, model) in models.iter().enumerate() {
        if model.label == model.id.as_str() {
            terminal.line(&format!("  {}. {}", index + 1, model.id.as_str()))?;
        } else {
            terminal.line(&format!(
                "  {}. {} — {}",
                index + 1,
                model.id.as_str(),
                model.label,
            ))?;
        }
    }
    terminal.line("These are provider-advertised models, not capability verification. No inference was requested.")?;
    loop {
        let answer =
            terminal.prompt("Choose a model number, or explicitly enter `manual MODEL_ID`: ")?;
        if let Ok(index) = answer.parse::<usize>()
            && let Some(model) = index.checked_sub(1).and_then(|index| models.get(index))
        {
            if model.tools == Some(false) {
                terminal.line(
                    "That model is advertised without tool calling, which this coding-agent route requires.",
                )?;
                continue;
            }
            return Ok(SelectedModel {
                id: model.id.as_str().to_owned(),
                discovered: Some(model.clone()),
                manual: false,
            });
        }
        if let Some(id) = answer.strip_prefix("manual ").map(str::trim)
            && !id.is_empty()
            && !id.contains(char::is_whitespace)
            && let Ok(id) = ModelName::new(id.to_owned())
        {
            let discovered = models.iter().find(|model| model.id == id).cloned();
            if discovered.as_ref().and_then(|model| model.tools) == Some(false) {
                terminal.line(
                    "That model is advertised without tool calling, which this coding-agent route requires.",
                )?;
                continue;
            }
            terminal.line("Using your explicit manual model ID; availability is unverified.")?;
            return Ok(SelectedModel {
                id: id.as_str().to_owned(),
                discovered,
                manual: true,
            });
        }
        terminal.line(
            "Choose an advertised number or use manual MODEL_ID. No model is selected by default.",
        )?;
    }
}

struct SelectedModel {
    id: String,
    discovered: Option<DiscoveredModel>,
    manual: bool,
}

fn validate_hosted_protocol(
    service: HostedService,
    protocol: CompatibleProtocol,
) -> Result<(), LauncherError> {
    let dialect = match protocol {
        CompatibleProtocol::Responses => WireDialect::CompatibleResponses,
        CompatibleProtocol::ChatCompletions => WireDialect::CompatibleChatCompletions,
        CompatibleProtocol::AnthropicMessages => WireDialect::AnthropicMessages,
        CompatibleProtocol::GoogleGenerateContent => WireDialect::GeminiGenerateContentV1,
    };
    service.route(dialect).map(|_| ()).map_err(|error| {
        LauncherError::Provider(OnboardingError::ModelDiscovery(error))
    })
}

pub(super) async fn account_selections(
    terminal: &mut Terminal<'_>,
    selection: ProviderSelection,
    existing: &ProviderSelection,
    cancellation: &CancellationToken,
) -> Result<ProviderSelection, LauncherError> {
    let mut models = std::collections::BTreeMap::new();
    for kind in selection.enabled().iter().copied().filter(|kind| kind.is_account()) {
        let model = if let Some(model) =
            selection.account_model(kind).or_else(|| existing.account_model(kind))
        {
            model.to_owned()
        } else {
            terminal.line(&format!(
                "Discovering {} models through its official executable…",
                kind.label()
            ))?;
            let discovered = match AccountProvider::discover(kind) {
                Ok(account) => {
                    interruptible(cancellation, account.discover_models(cancellation)).await
                }
                Err(error) => Err(error),
            };
            choose(terminal, discovered)?.id
        };
        models.insert(kind, model);
    }
    selection.with_account_models(models).map_err(LauncherError::from)
}

pub(super) async fn interruptible<T>(
    cancellation: &CancellationToken,
    operation: impl Future<Output = Result<T, OnboardingError>>,
) -> Result<T, OnboardingError> {
    let mut operation = Box::pin(operation);
    tokio::select! {
        result = &mut operation => result,
        signal = tokio::signal::ctrl_c() => {
            if signal.is_ok() {
                let _ = cancellation.cancel();
            }
            operation.await
        }
    }
}
