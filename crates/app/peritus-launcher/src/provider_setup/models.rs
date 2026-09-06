//! Provider-owned model menus shared by account and direct setup.

use crate::{LauncherError, terminal::Terminal};
use peritus_product_state::ProviderSelection;
use peritus_provider_onboarding::{AccountProvider, OnboardingError};

pub(super) fn choose(
    terminal: &mut Terminal<'_>,
    result: Result<Vec<String>, OnboardingError>,
) -> Result<String, LauncherError> {
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
    terminal.line("These are provider-advertised models, not capability verification. No inference was requested.")?;
    loop {
        let answer =
            terminal.prompt("Choose a model number, or explicitly enter `manual MODEL_ID`: ")?;
        if let Ok(index) = answer.parse::<usize>()
            && let Some(model) = index.checked_sub(1).and_then(|index| models.get(index))
        {
            return Ok(model.clone());
        }
        if let Some(id) = answer.strip_prefix("manual ").map(str::trim)
            && !id.is_empty()
            && id.len() <= 256
            && !id.chars().any(char::is_control)
            && !id.contains(char::is_whitespace)
        {
            terminal.line("Using your explicit manual model ID; availability is unverified.")?;
            return Ok(id.to_owned());
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
