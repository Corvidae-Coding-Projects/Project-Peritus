//! Durable reviewer routing selected through the ordinary setup terminal.

use peritus_product_state::{ProviderSelection, ReviewerSelection};
use peritus_provider_onboarding::{AccountProvider, discover_direct_models};

use crate::{LauncherError, terminal::Terminal};

pub(super) fn select(
    terminal: &mut Terminal<'_>,
    selection: ProviderSelection,
    existing: &ProviderSelection,
    interactive: bool,
) -> Result<ProviderSelection, LauncherError> {
    let retained =
        existing.reviewer().filter(|choice| selection.enabled().contains(&choice.provider()));
    if !interactive || selection.enabled().is_empty() {
        if existing.reviewer().is_some() && retained.is_none() {
            terminal.line("The saved reviewer provider is no longer selected; review will use the default provider and model.")?;
        }
        return selection.with_reviewer(retained.cloned()).map_err(LauncherError::from);
    }
    terminal.line("\nReview model for new conversations")?;
    terminal.line("Choose the provider and model used for independent review. This leaves your coding model unchanged.")?;
    terminal.line("  0. Use the default provider and its selected model")?;
    for (index, kind) in selection.enabled().iter().enumerate() {
        terminal.line(&format!("  {}. {}", index + 1, kind.label()))?;
    }
    let current = retained
        .and_then(|choice| selection.enabled().iter().position(|kind| *kind == choice.provider()))
        .map_or(0, |index| index + 1);
    let kind = loop {
        let answer = terminal.prompt(&format!("Reviewer provider [{current}]: "))?;
        let index = if answer.is_empty() { Some(current) } else { answer.parse::<usize>().ok() };
        if index == Some(0) {
            return selection.with_reviewer(None).map_err(LauncherError::from);
        }
        if let Some(kind) = index
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| selection.enabled().get(index))
        {
            break *kind;
        }
        terminal.line("Choose a displayed provider number, or 0 to use the default.")?;
    };
    let previous = retained.filter(|choice| choice.provider() == kind);
    let configured = selection.account_model(kind).or_else(|| {
        selection.direct_profile(kind).map(peritus_product_state::DirectProviderProfile::model)
    });
    let default = previous
        .map(|choice| (choice.model(), choice.manual()))
        .or_else(|| configured.map(|model| (model, true)));
    terminal.line(&format!("\nReviewer model — {}", kind.label()))?;
    let discovered = if kind.is_account() {
        AccountProvider::discover(kind).and_then(|provider| provider.discover_models())
    } else {
        selection
            .direct_profile(kind)
            .ok_or(peritus_provider_onboarding::OnboardingError::UnsupportedProvider)
            .and_then(discover_direct_models)
            .map(|models| models.into_iter().map(|model| model.id.as_str().to_owned()).collect())
    };
    let (model, manual) = super::models::choose_with_default(terminal, discovered, default)?;
    terminal.line(&format!("Reviewer: {} / {model}", kind.label()))?;
    selection
        .with_reviewer(Some(ReviewerSelection::new(kind, model, manual)?))
        .map_err(LauncherError::from)
}
