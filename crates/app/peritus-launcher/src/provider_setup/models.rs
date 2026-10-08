//! Provider-owned model menus shared by account and direct setup.

use std::future::Future;

use crate::{LauncherError, terminal::Terminal};
use peritus_model_protocol::{ModelName, WireDialect};
use peritus_product_state::{
    CompatibleProtocol, ProviderKind, ProviderModelCapability, ProviderModelFactSource,
    ProviderModelFacts, ProviderSelection,
};
use peritus_provider_core::{
    CancellationToken,
    catalog::DiscoveredModel,
    hosted::HostedService,
};
use peritus_provider_onboarding::{AccountProvider, OnboardingError};

pub(super) fn choose_direct(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
    result: Result<Vec<peritus_provider_core::catalog::DiscoveredModel>, OnboardingError>,
) -> Result<DirectModelSelection, LauncherError> {
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
    let protocol = if kind.hosted_service().is_none() {
        None
    } else {
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
            Some(WireDialect::GeminiGenerateContentV1) => {
                CompatibleProtocol::GoogleGenerateContent
            }
            _ if !matches!(kind, ProviderKind::OpenCodeZen | ProviderKind::OpenCodeGo) => {
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
        Some(protocol)
    };
    let facts = complete_facts(terminal, kind, &selected)?;
    Ok(DirectModelSelection { model: selected.id, protocol, facts })
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

pub(super) struct DirectModelSelection {
    pub(super) model: String,
    pub(super) protocol: Option<CompatibleProtocol>,
    pub(super) facts: ProviderModelFacts,
}

fn complete_facts(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
    selected: &SelectedModel,
) -> Result<ProviderModelFacts, LauncherError> {
    let discovered = selected.discovered.as_ref();
    if discovered.and_then(|model| model.tools) != Some(true)
        && !terminal.confirm(
            "Do the provider's docs confirm tool calling for this exact model? [y/N]: ",
            false,
        )?
    {
        return Err(LauncherError::Interaction(
            "the selected model has no confirmed tool-calling support; choose another model or rerun setup with its documented facts"
                .to_owned(),
        ));
    }
    let mut capabilities = vec![ProviderModelCapability::ToolCalls];
    if !kind.is_account() {
        if !terminal.confirm(
            "Do the provider's docs confirm streaming for this exact model and API? [y/N]: ",
            false,
        )? {
            return Err(LauncherError::Interaction(
                "the selected direct model has no confirmed streaming support; choose another model or API"
                    .to_owned(),
            ));
        }
        capabilities.push(ProviderModelCapability::Streaming);
    }
    let choices = feature_choices(kind);
    terminal.line("Select every additional feature documented for this exact model:")?;
    for (index, (_, label)) in choices.iter().enumerate() {
        terminal.line(&format!("  {}. {label}", index + 1))?;
    }
    let selected_features = loop {
        let answer = terminal.prompt("Features (comma-separated numbers, or 0 for none): ")?;
        if answer == "0" {
            break Vec::new();
        }
        let mut features = Vec::new();
        let valid = !answer.is_empty()
            && answer.split(',').all(|item| {
                item.trim()
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| index.checked_sub(1))
                    .and_then(|index| choices.get(index))
                    .is_some_and(|(capability, _)| {
                        features.push(*capability);
                        true
                    })
            });
        features.sort_unstable();
        features.dedup();
        if valid {
            break features;
        }
        terminal.line("Choose displayed feature numbers separated by commas, or 0 for none.")?;
    };
    capabilities.extend(selected_features);
    let input = match discovered.and_then(|model| model.input_tokens) {
        Some(value) => value,
        None => prompt_u64(terminal, "Documented maximum input tokens: ")?,
    };
    let output = match discovered.and_then(|model| model.output_tokens) {
        Some(value) => value,
        None => prompt_u64(terminal, "Documented maximum output tokens: ")?,
    };
    let max_tools = prompt_u32(terminal, "Documented maximum tools per request: ")?;
    let max_parallel = if capabilities.contains(&ProviderModelCapability::ParallelToolCalls) {
        loop {
            let value =
                prompt_u32(terminal, "Documented maximum simultaneous tool calls: ")?;
            if value <= max_tools {
                break value;
            }
            terminal.line("The simultaneous-call maximum cannot exceed the tool maximum.")?;
        }
    } else {
        1
    };
    let max_media = if capabilities.contains(&ProviderModelCapability::ImageInput) {
        prompt_u64(terminal, "Documented maximum inline image bytes: ")?
    } else {
        0
    };
    let source = if discovered.is_some() {
        ProviderModelFactSource::DiscoveredAndExplicit
    } else {
        ProviderModelFactSource::Explicit
    };
    ProviderModelFacts::new(
        source,
        selected.id.clone(),
        capabilities,
        input,
        output,
        max_tools,
        max_parallel,
        max_media,
    )
    .map_err(LauncherError::from)
}

fn feature_choices(
    kind: ProviderKind,
) -> Vec<(ProviderModelCapability, &'static str)> {
    let mut choices = vec![
        (ProviderModelCapability::ParallelToolCalls, "parallel tool calls"),
        (ProviderModelCapability::PromptCaching, "prompt caching"),
        (ProviderModelCapability::ReasoningControls, "reasoning controls"),
        (ProviderModelCapability::UsageDetail, "detailed usage counters"),
    ];
    if kind != ProviderKind::ClaudeAccount {
        choices.push((ProviderModelCapability::ImageInput, "image input"));
    }
    if !kind.is_account() {
        choices.push((ProviderModelCapability::ReasoningSummaries, "reasoning summaries"));
    }
    if kind.hosted_service().is_some() {
        choices.push((ProviderModelCapability::ReasoningReplay, "reasoning replay"));
    }
    choices
}

fn prompt_u64(terminal: &mut Terminal<'_>, prompt: &str) -> Result<u64, LauncherError> {
    loop {
        if let Ok(value) = terminal.prompt(prompt)?.parse::<u64>()
            && value > 0
        {
            return Ok(value);
        }
        terminal.line("Enter the positive documented maximum for this exact model.")?;
    }
}

fn prompt_u32(terminal: &mut Terminal<'_>, prompt: &str) -> Result<u32, LauncherError> {
    loop {
        if let Ok(value) = terminal.prompt(prompt)?.parse::<u32>()
            && value > 0
        {
            return Ok(value);
        }
        terminal.line("Enter the positive documented maximum for this exact model.")?;
    }
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
    let mut facts = std::collections::BTreeMap::new();
    for kind in selection.enabled().iter().copied().filter(|kind| kind.is_account()) {
        let retained_model =
            selection.account_model(kind).or_else(|| existing.account_model(kind))
            .map(str::to_owned);
        let retained_facts = selection
            .account_model_facts(kind)
            .or_else(|| existing.account_model_facts(kind))
            .filter(|facts| retained_model.as_deref() == Some(facts.model()));
        if let (Some(model), Some(model_facts)) = (&retained_model, retained_facts) {
            models.insert(kind, model.clone());
            facts.insert(kind, model_facts.clone());
            continue;
        }
        terminal.line(&format!(
            "Discovering {} models through its official executable…",
            kind.label()
        ))?;
        let discovered = match AccountProvider::discover(kind) {
            Ok(account) => interruptible(cancellation, account.discover_models(cancellation)).await,
            Err(error) => Err(error),
        };
        let selected = if let Some(model) = retained_model {
            let catalog = match discovered {
                Ok(catalog) => catalog,
                Err(error @ OnboardingError::Cancelled) => return Err(error.into()),
                Err(error) => {
                    terminal.line(&error.to_string())?;
                    Vec::new()
                }
            };
            SelectedModel {
                discovered: catalog.into_iter().find(|entry| entry.id.as_str() == model),
                id: model,
                manual: true,
            }
        } else {
            choose(terminal, discovered)?
        };
        let model_facts = complete_facts(terminal, kind, &selected)?;
        models.insert(kind, selected.id);
        facts.insert(kind, model_facts);
    }
    selection
        .with_account_models(models)?
        .with_account_model_facts(facts)
        .map_err(LauncherError::from)
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
