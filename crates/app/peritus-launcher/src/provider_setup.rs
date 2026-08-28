//! First-run provider selection and focused repeat-launch repair.

use peritus_product_state::{ProviderKind, ProviderSelection};
use peritus_provider_onboarding::{
    AccountLogin, AccountProvider, ProviderCatalog, ProviderObservation, ProviderStatus,
    remove_direct_credential,
};

use crate::{LauncherError, PreparedProduct, ProductBootstrap};

mod direct;
mod selection;

use crate::terminal::Terminal;
use selection::{choose_default, choose_provider_set};

const CODEX: ProviderKind = ProviderKind::CodexAccount;
const CLAUDE: ProviderKind = ProviderKind::ClaudeAccount;
const OPENAI_API: ProviderKind = ProviderKind::OpenAiApi;
const ANTHROPIC_API: ProviderKind = ProviderKind::AnthropicApi;
const GOOGLE_API: ProviderKind = ProviderKind::GoogleGeminiApi;
const COMPATIBLE: ProviderKind = ProviderKind::CompatibleEndpoint;

/// Completes first-run provider setup or repairs only unhealthy retained providers.
pub fn ensure_configured(prepared: PreparedProduct) -> Result<PreparedProduct, LauncherError> {
    let observations = ProviderCatalog::observe();
    if !prepared.state().provider_setup_complete() {
        return first_run(&prepared, &observations);
    }
    repair_if_needed(prepared, &observations)
}

fn first_run(
    prepared: &PreparedProduct,
    observations: &[ProviderObservation],
) -> Result<PreparedProduct, LauncherError> {
    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("Welcome to Peritus")?;
    terminal.line("Choose how Peritus may run coding agents. You can change this later.")?;
    show_catalog(&mut terminal, observations, None)?;

    let ready = ready_kinds(observations);
    let default_text = selection_text(&ready);
    let (requested, used_ready_default) =
        choose_provider_set(&mut terminal, ready, "ready providers")?;
    if used_ready_default && !default_text.is_empty() {
        terminal.line(&format!("Using {default_text}."))?;
    }

    let activated = activate_requested(&mut terminal, observations, requested, None)?;
    let default = choose_default(&mut terminal, &activated.enabled)?;
    let selection = ProviderSelection::with_direct_profiles(
        activated.enabled,
        default,
        activated.direct_profiles,
    )?;
    persist(prepared, selection)
}

/// Opens provider settings without replaying unrelated first-run setup.
pub fn configure(prepared: &PreparedProduct) -> Result<PreparedProduct, LauncherError> {
    let observations = ProviderCatalog::observe();
    let current = prepared.state().providers().clone();
    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("Provider settings")?;
    terminal
        .line("Select one or more providers. Existing credentials stay in the OS key store.")?;
    show_catalog(&mut terminal, &observations, Some(&current))?;
    let (requested, _) =
        choose_provider_set(&mut terminal, current.enabled().to_vec(), "current selection")?;
    let activated = activate_requested(&mut terminal, &observations, requested, Some(&current))?;
    let default = choose_default(&mut terminal, &activated.enabled)?;
    let selection = ProviderSelection::with_direct_profiles(
        activated.enabled,
        default,
        activated.direct_profiles.clone(),
    )?;
    let configured = persist(prepared, selection)?;
    remove_replaced_credentials(&mut terminal, &current, &activated.direct_profiles)?;
    terminal.line("Provider settings saved.")?;
    Ok(configured)
}

fn repair_if_needed(
    prepared: PreparedProduct,
    observations: &[ProviderObservation],
) -> Result<PreparedProduct, LauncherError> {
    let selected = prepared.state().providers().enabled();
    let unhealthy = selected
        .iter()
        .filter_map(|kind| observation(observations, *kind))
        .filter(|item| item.status() != ProviderStatus::Ready)
        .collect::<Vec<_>>();
    if unhealthy.is_empty() {
        return Ok(prepared);
    }

    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("A provider needs attention")?;
    terminal
        .line("Your workspace is safe; repair sign-in now or continue without this provider.")?;
    let mut retained = selected.to_vec();
    for item in unhealthy {
        terminal.line(&format!("  {} — {}", item.kind().label(), item.status().label()))?;
        let sign_in = match item.status() {
            ProviderStatus::SignedOut | ProviderStatus::NeedsAttention => terminal.confirm(
                "Sign in now? Press Enter for yes, or type n to continue without it: ",
                true,
            )?,
            ProviderStatus::Unavailable => {
                installation_guidance(&mut terminal, item.kind())?;
                false
            }
            ProviderStatus::Ready => true,
        };
        let ready = sign_in && login(&mut terminal, item.kind())?;
        if !ready {
            retained.retain(|kind| kind != &item.kind());
        }
    }
    let old_default = prepared.state().providers().default();
    let default =
        old_default.filter(|kind| retained.contains(kind)).or_else(|| retained.first().copied());
    let direct_profiles = prepared
        .state()
        .providers()
        .direct_profiles()
        .iter()
        .filter(|profile| retained.contains(&profile.kind()))
        .cloned()
        .collect();
    let selection = ProviderSelection::with_direct_profiles(retained, default, direct_profiles)?;
    persist(&prepared, selection)
}

fn show_catalog(
    terminal: &mut Terminal<'_>,
    observations: &[ProviderObservation],
    current: Option<&ProviderSelection>,
) -> Result<(), LauncherError> {
    terminal.line("")?;
    for (index, item) in observations.iter().enumerate() {
        let selected = current.is_some_and(|selection| selection.enabled().contains(&item.kind()));
        let marker = if selected { "selected, " } else { "" };
        terminal.line(&format!(
            "  {}. {:<38} {marker}{}",
            index + 1,
            item.kind().label(),
            item.status().label()
        ))?;
    }
    for (index, kind) in [OPENAI_API, ANTHROPIC_API, GOOGLE_API, COMPATIBLE].into_iter().enumerate()
    {
        let configured = current.and_then(|selection| selection.direct_profile(kind)).is_some();
        let status = if configured { "selected, Configured" } else { "Add key" };
        terminal.line(&format!("  {}. {:<38} {status}", index + 3, kind.label()))?;
    }
    terminal.line("  0. Offline browse mode")?;
    terminal.line("")
}

fn activate_requested(
    terminal: &mut Terminal<'_>,
    observations: &[ProviderObservation],
    requested: Vec<ProviderKind>,
    existing: Option<&ProviderSelection>,
) -> Result<ActivatedProviders, LauncherError> {
    let mut enabled = Vec::new();
    let mut direct_profiles = Vec::new();
    for kind in requested {
        if kind.is_direct() {
            let profile = match existing.and_then(|selection| selection.direct_profile(kind)) {
                Some(profile) => {
                    let answer = terminal.prompt(&format!(
                        "{} is already configured. Enter to keep it, or type r to replace its key: ",
                        kind.label()
                    ))?;
                    if answer.eq_ignore_ascii_case("r") {
                        direct::setup(terminal, kind)?
                    } else if answer.is_empty() {
                        profile.clone()
                    } else {
                        return Err(LauncherError::Interaction(
                            "enter r to replace the key, or press Enter to keep it".to_owned(),
                        ));
                    }
                }
                None => direct::setup(terminal, kind)?,
            };
            enabled.push(kind);
            direct_profiles.push(profile);
            continue;
        }
        let Some(item) = observation(observations, kind) else {
            continue;
        };
        let is_ready = match item.status() {
            ProviderStatus::Ready => true,
            ProviderStatus::SignedOut => {
                terminal.line(&format!("\n{} requires sign-in.", kind.label()))?;
                login(terminal, kind)?
            }
            ProviderStatus::Unavailable => {
                installation_guidance(terminal, kind)?;
                false
            }
            ProviderStatus::NeedsAttention => {
                terminal.line(&format!(
                    "{} could not report a supported login status. Update its CLI and retry later.",
                    kind.label()
                ))?;
                false
            }
        };
        if is_ready {
            enabled.push(kind);
        }
    }
    if enabled.is_empty() {
        terminal.line("Continuing in offline browse mode. Agent runs will ask for a provider.")?;
    }
    Ok(ActivatedProviders { enabled, direct_profiles })
}

fn remove_replaced_credentials(
    terminal: &mut Terminal<'_>,
    previous: &ProviderSelection,
    retained: &[peritus_product_state::DirectProviderProfile],
) -> Result<(), LauncherError> {
    for old in previous.direct_profiles() {
        if retained.iter().any(|profile| profile == old) {
            continue;
        }
        if let Err(error) = remove_direct_credential(old) {
            terminal.line(&format!(
                "The old {} key could not be removed automatically: {error}",
                old.kind().label()
            ))?;
        }
    }
    Ok(())
}

fn login(terminal: &mut Terminal<'_>, kind: ProviderKind) -> Result<bool, LauncherError> {
    let Ok(provider) = AccountProvider::discover(kind) else {
        installation_guidance(terminal, kind)?;
        return Ok(false);
    };
    let mode = if kind == CODEX {
        let answer =
            terminal.prompt("Login method: Enter for browser, or type 2 for device code: ")?;
        if answer == "2" { AccountLogin::Device } else { AccountLogin::Browser }
    } else {
        AccountLogin::Browser
    };
    terminal.line("Handing the terminal to the official provider login…")?;
    let observation = provider.login(mode)?;
    terminal.line(&format!("{} is ready.", observation.kind().label()))?;
    Ok(true)
}

fn persist(
    prepared: &PreparedProduct,
    selection: ProviderSelection,
) -> Result<PreparedProduct, LauncherError> {
    let layout = prepared.layout().clone();
    ProductBootstrap::new(layout).configure_providers(selection)
}

fn observation(
    observations: &[ProviderObservation],
    kind: ProviderKind,
) -> Option<&ProviderObservation> {
    observations.iter().find(|item| item.kind() == kind)
}

fn ready_kinds(observations: &[ProviderObservation]) -> Vec<ProviderKind> {
    observations
        .iter()
        .filter(|item| item.status() == ProviderStatus::Ready)
        .map(ProviderObservation::kind)
        .collect()
}

fn selection_text(kinds: &[ProviderKind]) -> String {
    kinds.iter().map(|kind| kind.label()).collect::<Vec<_>>().join(" and ")
}

fn installation_guidance(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
) -> Result<(), LauncherError> {
    let executable = if kind == CODEX { "codex" } else { "claude" };
    terminal.line(&format!(
        "{} is not available. Install or update the official `{executable}` CLI, then open Peritus again.",
        kind.label()
    ))
}

struct ActivatedProviders {
    enabled: Vec<ProviderKind>,
    direct_profiles: Vec<peritus_product_state::DirectProviderProfile>,
}
