//! Provider-owned model menus shared by account and direct setup.

use crate::{LauncherError, terminal::Terminal};
use peritus_model_protocol::WireDialect;
use peritus_product_state::{CompatibleProtocol, ProviderSelection};
use peritus_provider_onboarding::{AccountProvider, OnboardingError};

pub(super) fn choose_direct(
    terminal: &mut Terminal<'_>,
    kind: peritus_product_state::ProviderKind,
    result: Result<Vec<peritus_provider_core::catalog::DiscoveredModel>, OnboardingError>,
) -> Result<(String, Option<CompatibleProtocol>), LauncherError> {
    choose_direct_with_default(terminal, kind, result, None)
}

pub(super) fn choose_direct_with_default(
    terminal: &mut Terminal<'_>,
    kind: peritus_product_state::ProviderKind,
    result: Result<Vec<peritus_provider_core::catalog::DiscoveredModel>, OnboardingError>,
    existing: Option<&peritus_product_state::DirectProviderProfile>,
) -> Result<(String, Option<CompatibleProtocol>), LauncherError> {
    let models = match result {
        Ok(models) => models,
        Err(error) => {
            terminal.line(&error.to_string())?;
            Vec::new()
        }
    };
    for (index, model) in models.iter().enumerate() {
        terminal.line(&format!("  {}. {} ({})", index + 1, model.label, model.id.as_str()))?;
    }
    let identifiers = models.iter().map(|model| model.id.as_str().to_owned()).collect::<Vec<_>>();
    let (model, _) =
        read_choice(terminal, &identifiers, existing.map(|profile| (profile.model(), false)))?;
    let metadata = models.iter().find(|entry| entry.id.as_str() == model);
    terminal.line(match metadata.and_then(|entry| entry.tools) {
        Some(true) => "Tool calling: advertised by provider; connection not tested.",
        Some(false) => "Tool calling: provider advertises no support for this model.",
        None => "Tool calling: unknown; connection not tested.",
    })?;
    if kind.hosted_service().is_none() {
        return Ok((model, None));
    }
    if matches!(
        kind,
        peritus_product_state::ProviderKind::OpenCodeZen
            | peritus_product_state::ProviderKind::OpenCodeGo
    ) {
        terminal.line("The official OpenCode CLI selects the protocol for this model. Peritus owns all tools and conversation history.")?;
        return Ok((model, Some(CompatibleProtocol::OpenCodeRuntime)));
    }
    let protocol = match metadata.and_then(|entry| entry.dialect) {
        Some(WireDialect::CompatibleResponses | WireDialect::OpenAiResponses) => {
            CompatibleProtocol::Responses
        }
        Some(WireDialect::CompatibleChatCompletions) => CompatibleProtocol::ChatCompletions,
        Some(WireDialect::AnthropicMessages) => CompatibleProtocol::AnthropicMessages,
        Some(WireDialect::GeminiGenerateContentV1) => CompatibleProtocol::GoogleGenerateContent,
        _ => existing
            .and_then(peritus_product_state::DirectProviderProfile::compatible_protocol)
            .unwrap_or(CompatibleProtocol::ChatCompletions),
    };
    Ok((model, Some(protocol)))
}

pub(super) fn choose(
    terminal: &mut Terminal<'_>,
    result: Result<Vec<String>, OnboardingError>,
) -> Result<String, LauncherError> {
    choose_with_default(terminal, result, None).map(|(model, _)| model)
}

pub(super) fn choose_with_default(
    terminal: &mut Terminal<'_>,
    result: Result<Vec<String>, OnboardingError>,
    default: Option<(&str, bool)>,
) -> Result<(String, bool), LauncherError> {
    let models = match result {
        Ok(models) => models,
        Err(error) => {
            terminal.line(&error.to_string())?;
            Vec::new()
        }
    };
    for (index, model) in models.iter().enumerate() {
        terminal.line(&format!("  {}. {model}", index + 1))?;
    }
    read_choice(terminal, &models, default)
}

fn read_choice(
    terminal: &mut Terminal<'_>,
    models: &[String],
    default: Option<(&str, bool)>,
) -> Result<(String, bool), LauncherError> {
    terminal.line("These are provider-advertised models, not capability verification. No inference was requested.")?;
    if let Some((model, _)) = default {
        terminal.line(&format!("Press Enter to keep {model}."))?;
    }
    loop {
        let answer =
            terminal.prompt("Choose a model number, or explicitly enter `manual MODEL_ID`: ")?;
        if answer.is_empty()
            && let Some((model, manual)) = default
        {
            return Ok((model.to_owned(), manual));
        }
        if let Ok(index) = answer.parse::<usize>()
            && let Some(model) = index.checked_sub(1).and_then(|index| models.get(index))
        {
            return Ok((model.clone(), false));
        }
        if let Some(id) = answer.strip_prefix("manual ").map(str::trim)
            && !id.is_empty()
            && id.len() <= 256
            && !id.chars().any(char::is_control)
            && !id.contains(char::is_whitespace)
        {
            terminal.line("Using your explicit manual model ID; availability is unverified.")?;
            return Ok((id.to_owned(), true));
        }
        terminal.line(
            "Choose an advertised number or use manual MODEL_ID. No model is selected by default.",
        )?;
    }
}

pub(super) fn account_selections(
    terminal: &mut Terminal<'_>,
    selection: ProviderSelection,
    existing: &ProviderSelection,
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
            choose(
                terminal,
                AccountProvider::discover(kind).and_then(|account| account.discover_models()),
            )?
        };
        models.insert(kind, model);
    }
    selection.with_account_models(models).map_err(LauncherError::from)
}
