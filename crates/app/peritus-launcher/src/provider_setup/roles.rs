//! Startup role defaults: every role has an explicit provider, model and effort.
use crate::{LauncherError, terminal::Terminal};
use peritus_product_state::{ProviderSelection, RoleEffort, RoleModelDefaults, RoleModelSelection};
use peritus_provider_onboarding::{AccountProvider, discover_direct_models};

#[cfg(test)]
#[path = "../../../../../discovery/proptest-praxis/owner/role_setup.rs"]
mod replay;

pub(super) fn select(
    terminal: &mut Terminal<'_>,
    selection: ProviderSelection,
    existing: &ProviderSelection,
    interactive: bool,
) -> Result<ProviderSelection, LauncherError> {
    let retained = existing.role_models().filter(|roles| roles.providers_enabled(&selection));
    if selection.enabled().is_empty() || !interactive {
        return selection.with_role_models(retained.cloned()).map_err(Into::into);
    }
    terminal.line("\nModels for writer, reviewer, and fixer")?;
    terminal.line("Choose all three roles now so work does not stop later for a missing selection. These are defaults for new conversations; existing conversations keep their choices.")?;
    terminal.line("/model opens agent, model, and reasoning-effort menus. /models also opens these settings; /effort changes effort on its own.")?;
    let writer = choose(
        terminal,
        "Writer",
        &selection,
        retained.map(RoleModelDefaults::writer),
        None,
        None,
    )?;
    let reviewer = choose(
        terminal,
        "Reviewer",
        &selection,
        retained.map(RoleModelDefaults::reviewer),
        Some(&writer),
        existing.reviewer(),
    )?;
    let fixer = choose(
        terminal,
        "Fixer",
        &selection,
        retained.map(RoleModelDefaults::fixer),
        Some(&writer),
        None,
    )?;
    terminal.line("\nRole defaults:")?;
    for (name, choice) in [("Writer", &writer), ("Reviewer", &reviewer), ("Fixer", &fixer)] {
        terminal.line(&format!(
            "  {name}: {} / {} / {}",
            choice.provider().label(),
            choice.model(),
            choice.effort().label()
        ))?;
    }
    selection
        .with_role_models(Some(RoleModelDefaults::new(writer, reviewer, fixer)))
        .map_err(Into::into)
}

fn choose(
    terminal: &mut Terminal<'_>,
    name: &str,
    selection: &ProviderSelection,
    previous: Option<&RoleModelSelection>,
    writer: Option<&RoleModelSelection>,
    legacy: Option<&peritus_product_state::ReviewerSelection>,
) -> Result<RoleModelSelection, LauncherError> {
    terminal.line(&format!("\n{name} provider"))?;
    if let Some(writer) = writer {
        terminal.line(&format!(
            "  0. Same as writer: {} / {} / {}",
            writer.provider().label(),
            writer.model(),
            writer.effort().label()
        ))?;
    }
    for (index, kind) in selection.enabled().iter().enumerate() {
        terminal.line(&format!("  {}. {}", index + 1, kind.label()))?;
    }
    let preferred = previous
        .map(RoleModelSelection::provider)
        .or_else(|| legacy.map(peritus_product_state::ReviewerSelection::provider));
    let default = preferred
        .and_then(|p| selection.enabled().iter().position(|kind| *kind == p))
        .map(|i| i + 1)
        .or_else(|| writer.map(|_| 0))
        .or_else(|| {
            selection
                .default()
                .and_then(|kind| selection.enabled().iter().position(|p| *p == kind))
                .map(|i| i + 1)
        });
    let provider = loop {
        let answer = terminal.prompt(&format!(
            "{name} provider [{}]: ",
            default.map_or_else(String::new, |v| v.to_string())
        ))?;
        let index = if answer.is_empty() { default } else { answer.parse::<usize>().ok() };
        if index == Some(0)
            && let Some(writer) = writer
        {
            return Ok(writer.clone());
        }
        if let Some(kind) =
            index.and_then(|i| i.checked_sub(1)).and_then(|i| selection.enabled().get(i))
        {
            break *kind;
        }
        terminal.line("Choose a displayed provider number.")?;
    };
    let retained = previous.filter(|p| p.provider() == provider);
    let default = retained
        .map(|p| (p.model(), p.manual()))
        .or_else(|| legacy.filter(|p| p.provider() == provider).map(|p| (p.model(), p.manual())))
        .or_else(|| selection.account_model(provider).map(|id| (id, true)))
        .or_else(|| selection.direct_profile(provider).map(|p| (p.model(), true)));
    let (model, manual) = choose_model(terminal, name, provider, selection, default)?;
    let prior = retained.filter(|p| p.model() == model).map(RoleModelSelection::effort);
    let effort = loop {
        let choices = RoleEffort::ALL
            .into_iter()
            .filter(|value| !provider.is_account() || *value != RoleEffort::Off)
            .map(RoleEffort::label)
            .collect::<Vec<_>>()
            .join(", ");
        terminal.line(&format!("{name} effort: {choices}. Model support is checked by the provider; values are never substituted."))?;
        let answer = terminal.prompt(&format!(
            "{name} effort{}: ",
            prior.map_or_else(String::new, |v| format!(" [{}]", v.label()))
        ))?;
        let effort = if answer.is_empty() { prior } else { RoleEffort::parse(&answer) };
        if let Some(effort) = effort
            && (!provider.is_account() || effort != RoleEffort::Off)
        {
            break effort;
        }
        terminal.line("Select an explicit displayed effort.")?;
    };
    RoleModelSelection::new(provider, model, manual, effort).map_err(Into::into)
}

fn choose_model(
    terminal: &mut Terminal<'_>,
    name: &str,
    provider: peritus_product_state::ProviderKind,
    selection: &ProviderSelection,
    default: Option<(&str, bool)>,
) -> Result<(String, bool), LauncherError> {
    loop {
        terminal.line("Enter keeps the exact saved model; list discovers models; manual ID explicitly bypasses catalog membership.")?;
        let answer = terminal.prompt(&format!(
            "{name} model{}: ",
            default.map_or_else(String::new, |(id, _)| format!(" [{id}]"))
        ))?;
        if answer.is_empty()
            && let Some((id, manual)) = default
        {
            return Ok((id.to_owned(), manual));
        }
        if let Some(id) = answer.strip_prefix("manual ") {
            if peritus_model_protocol::ModelName::new(id.to_owned()).is_ok() {
                return Ok((id.to_owned(), true));
            }
            terminal.line("Enter a valid model identifier.")?;
            continue;
        }
        let discovered = if provider.is_account() {
            AccountProvider::discover(provider).and_then(|p| p.discover_models())
        } else {
            selection
                .direct_profile(provider)
                .ok_or(peritus_provider_onboarding::OnboardingError::UnsupportedProvider)
                .and_then(discover_direct_models)
                .map(|rows| rows.into_iter().map(|row| row.id.as_str().to_owned()).collect())
        };
        if answer == "list" {
            return super::models::choose_with_default(terminal, discovered, default);
        }
        match discovered {
            Ok(models) if models.iter().any(|id| id == &answer) => return Ok((answer, false)),
            Ok(_) => terminal.line(
                "That model was not advertised. Enter list, another exact ID, or manual ID.",
            )?,
            Err(error) => {
                terminal.line(&format!("{error}. Retry list or explicitly enter manual ID."))?;
            }
        }
    }
}
