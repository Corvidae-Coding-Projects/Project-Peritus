//! First-run provider selection and focused repeat-launch repair.

use peritus_product_state::{ProviderKind, ProviderRouteIdentity, ProviderSelection};
use peritus_provider_core::CancellationToken;
use peritus_provider_onboarding::{
    AccountLogin, AccountProvider, ProviderCatalog, ProviderObservation, ProviderStatus,
    ProviderEffectStore,
};

use crate::{LauncherError, PreparedProduct, ProductBootstrap};

mod connection;
mod direct;
mod install;
mod models;
mod selection;

use crate::terminal::Terminal;
use selection::{choose_default, choose_failover, choose_provider_set};

const CODEX: ProviderKind = ProviderKind::CodexAccount;
#[cfg(test)]
const CLAUDE: ProviderKind = ProviderKind::ClaudeAccount;

/// Completes first-run provider setup or repairs only unhealthy retained providers.
pub async fn ensure_configured(
    prepared: PreparedProduct,
) -> Result<PreparedProduct, LauncherError> {
    let cancellation = CancellationToken::new();
    let effects = ProviderEffectStore::open(prepared.layout().provider_effects_root())?;
    effects.reconcile_credentials(prepared.state().providers())?;
    let observations = ProviderCatalog::observe(&cancellation).await?;
    if !prepared.state().provider_setup_complete() {
        return first_run(&prepared, &observations, &cancellation, &effects).await;
    }
    repair_if_needed(prepared, &observations, &cancellation, &effects).await
}

async fn first_run(
    prepared: &PreparedProduct,
    observations: &[ProviderObservation],
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
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

    let activated =
        activate_requested(&mut terminal, observations, requested, None, cancellation, effects)
            .await?;
    let default = choose_default(&mut terminal, &activated.enabled)?;
    let default_route = choose_default_route(
        &mut terminal,
        default,
        &activated.direct_profiles,
        None,
    )?;
    let automatic_failover = choose_failover(&mut terminal, activated.route_count(), false)?;
    let selection = ProviderSelection::with_routes_and_failover(
        activated.enabled,
        default_route,
        activated.direct_profiles,
        automatic_failover,
    )?;
    persist(prepared, selection, effects, cancellation).await
}

/// Opens provider settings without replaying unrelated first-run setup.
pub async fn configure(
    prepared: &PreparedProduct,
) -> Result<PreparedProduct, LauncherError> {
    let cancellation = CancellationToken::new();
    let effects = ProviderEffectStore::open(prepared.layout().provider_effects_root())?;
    effects.reconcile_credentials(prepared.state().providers())?;
    let observations = ProviderCatalog::observe(&cancellation).await?;
    let current = prepared.state().providers().clone();
    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("Provider settings")?;
    terminal
        .line("Select one or more providers. Existing credentials stay in the OS key store.")?;
    show_catalog(&mut terminal, &observations, Some(&current))?;
    let (requested, _) =
        choose_provider_set(&mut terminal, current.enabled().to_vec(), "current selection")?;
    let activated = activate_requested(
        &mut terminal,
        &observations,
        requested,
        Some(&current),
        &cancellation,
        &effects,
    )
    .await?;
    let default = choose_default(&mut terminal, &activated.enabled)?;
    let default_route = choose_default_route(
        &mut terminal,
        default,
        &activated.direct_profiles,
        current.default_route(),
    )?;
    let automatic_failover = choose_failover(
        &mut terminal,
        activated.route_count(),
        current.automatic_failover(),
    )?;
    let selection = ProviderSelection::with_routes_and_failover(
        activated.enabled,
        default_route,
        activated.direct_profiles.clone(),
        automatic_failover,
    )?;
    record_replaced_credentials(&effects, &current, &activated.direct_profiles)?;
    let configured = persist(prepared, selection, &effects, &cancellation).await?;
    terminal.line("Provider settings saved.")?;
    Ok(configured)
}

async fn repair_if_needed(
    prepared: PreparedProduct,
    observations: &[ProviderObservation],
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
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
        show_diagnostic(&mut terminal, item)?;
        if matches!(
            item.status(),
            ProviderStatus::Unknown
                | ProviderStatus::Infrastructure
                | ProviderStatus::NeedsAttention
        ) {
            continue;
        }
        let sign_in = match item.status() {
            ProviderStatus::SignedOut => terminal.confirm(
                "Sign in now? Press Enter for yes, or type n to continue without it: ",
                true,
            )?,
            ProviderStatus::Unavailable => {
                if install::offer(&mut terminal, item.kind(), cancellation, effects).await? {
                    true
                } else {
                    installation_guidance(&mut terminal, item.kind())?;
                    false
                }
            }
            ProviderStatus::Ready => true,
            ProviderStatus::Unknown
            | ProviderStatus::Infrastructure
            | ProviderStatus::NeedsAttention => false,
        };
        let outcome = if sign_in {
            login(&mut terminal, item.kind(), cancellation).await?
        } else {
            LoginOutcome::Unavailable
        };
        if outcome == LoginOutcome::Unavailable {
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
    let default_route = choose_default_route(
        &mut terminal,
        default,
        &direct_profiles,
        prepared.state().providers().default_route(),
    )?;
    let route_count = retained.iter().filter(|kind| kind.is_account()).count()
        + direct_profiles.len();
    let automatic_failover =
        prepared.state().providers().automatic_failover() && route_count > 1;
    let selection = ProviderSelection::with_routes_and_failover(
        retained,
        default_route,
        direct_profiles,
        automatic_failover,
    )?;
    persist(&prepared, selection, effects, cancellation).await
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
        show_diagnostic(terminal, item)?;
    }
    for (index, kind) in ProviderKind::ALL.into_iter().enumerate().skip(2) {
        let configured = current.and_then(|selection| selection.direct_profile(kind)).is_some();
        let status = if configured {
            "selected, configured; connection not tested this session"
        } else {
            "Add key"
        };
        terminal.line(&format!("  {}. {:<38} {status}", index + 1, kind.label()))?;
    }
    terminal.line("  0. Offline browse mode")?;
    terminal.line("")
}

async fn activate_requested(
    terminal: &mut Terminal<'_>,
    observations: &[ProviderObservation],
    requested: Vec<ProviderKind>,
    existing: Option<&ProviderSelection>,
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
) -> Result<ActivatedProviders, LauncherError> {
    let mut enabled = Vec::new();
    let mut direct_profiles = Vec::new();
    for kind in requested {
        if kind.is_direct() {
            let existing_profiles = existing
                .into_iter()
                .flat_map(ProviderSelection::direct_profiles)
                .filter(|profile| profile.kind() == kind)
                .collect::<Vec<_>>();
            enabled.push(kind);
            if existing_profiles.is_empty() {
                direct_profiles.push(direct::setup(terminal, kind, effects, cancellation).await?);
            } else {
                for profile in existing_profiles {
                    direct_profiles.push(
                        connection::existing(terminal, profile, effects, cancellation).await?,
                    );
                }
            }
            continue;
        }
        let Some(item) = observation(observations, kind) else {
            continue;
        };
        let retained = existing.is_some_and(|selection| selection.enabled().contains(&kind));
        let is_ready = match item.status() {
            ProviderStatus::Ready => true,
            ProviderStatus::SignedOut => {
                terminal.line(&format!("\n{} requires sign-in.", kind.label()))?;
                match login(terminal, kind, cancellation).await? {
                    LoginOutcome::Ready => true,
                    LoginOutcome::Unverified => retained,
                    LoginOutcome::Unavailable => false,
            }
            ProviderStatus::Unavailable => {
                if install::offer(terminal, kind, cancellation, effects).await? {
                    match login(terminal, kind, cancellation).await? {
                        LoginOutcome::Ready => true,
                        LoginOutcome::Unverified => retained,
                        LoginOutcome::Unavailable => false,
                    }
                } else {
                    installation_guidance(terminal, kind)?;
                    false
                }
            }
            ProviderStatus::Unknown
            | ProviderStatus::Infrastructure
            | ProviderStatus::NeedsAttention => {
                terminal.line(&format!("{} could not report a usable login status.", kind.label()))?;
                show_diagnostic(terminal, item)?;
                retained
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

fn record_replaced_credentials(
    effects: &ProviderEffectStore,
    previous: &ProviderSelection,
    retained: &[peritus_product_state::DirectProviderProfile],
) -> Result<(), LauncherError> {
    for old in previous.direct_profiles() {
        if retained.iter().any(|profile| profile == old) {
            continue;
        }
        effects.record_credential_cleanup(old.credential_reference())?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoginOutcome {
    Ready,
    Unverified,
    Unavailable,
}

async fn login(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
    cancellation: &CancellationToken,
) -> Result<LoginOutcome, LauncherError> {
    let Ok(provider) = AccountProvider::discover(kind) else {
        installation_guidance(terminal, kind)?;
        return Ok(LoginOutcome::Unavailable);
    };
    let mode = if kind == CODEX {
        let answer =
            terminal.prompt("Login method: Enter for browser, or type 2 for device code: ")?;
        if answer == "2" { AccountLogin::Device } else { AccountLogin::Browser }
    } else {
        AccountLogin::Browser
    };
    terminal.line("Handing the terminal to the official provider login…")?;
    let observation = {
        let _title = crate::terminal::product_title()?;
        provider.login(mode, cancellation).await?
    };
    if observation.status() == ProviderStatus::Ready {
        terminal.line(&format!("{} is ready.", observation.kind().label()))?;
        return Ok(LoginOutcome::Ready);
    }
    terminal.line(&format!(
        "{} login finished, but its status is {}.",
        observation.kind().label(),
        observation.status().label()
    ))?;
    show_diagnostic(terminal, &observation)?;
    Ok(LoginOutcome::Unverified)
}

fn show_diagnostic(
    terminal: &mut Terminal<'_>,
    observation: &ProviderObservation,
) -> Result<(), LauncherError> {
    if let Some(diagnostic) = observation.diagnostic() {
        terminal.line(&format!("     {diagnostic}"))?;
    }
    Ok(())
}

async fn persist(
    prepared: &PreparedProduct,
    selection: ProviderSelection,
    effects: &ProviderEffectStore,
    cancellation: &CancellationToken,
) -> Result<PreparedProduct, LauncherError> {
    let selection = models::account_selections(
        &mut Terminal::stdio(),
        selection,
        prepared.state().providers(),
        cancellation,
    )
    .await?;
    let layout = prepared.layout().clone();
    let configured = ProductBootstrap::new(layout).configure_providers(selection)?;
    effects.reconcile_credentials(configured.state().providers())?;
    Ok(configured)
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

impl ActivatedProviders {
    fn route_count(&self) -> usize {
        self.enabled.iter().filter(|kind| kind.is_account()).count()
            + self.direct_profiles.len()
    }
}

fn choose_default_route(
    terminal: &mut Terminal<'_>,
    default: Option<ProviderKind>,
    direct_profiles: &[peritus_product_state::DirectProviderProfile],
    previous: Option<ProviderRouteIdentity>,
) -> Result<Option<ProviderRouteIdentity>, LauncherError> {
    let Some(kind) = default else {
        return Ok(None);
    };
    if kind.is_account() {
        return Ok(Some(kind.route_identity()));
    }
    let candidates = direct_profiles
        .iter()
        .filter(|profile| profile.kind() == kind)
        .collect::<Vec<_>>();
    if let [only] = candidates.as_slice() {
        return Ok(Some(only.route_identity()));
    }
    if let Some(previous) = previous
        && candidates.iter().any(|profile| profile.route_identity() == previous)
    {
        return Ok(Some(previous));
    }
    terminal.line(&format!("\nDefault {} route for new runs:", kind.label()))?;
    for (index, profile) in candidates.iter().enumerate() {
        terminal.line(&format!("  {}. {}", index + 1, profile.model()))?;
    }
    loop {
        let answer = terminal.prompt("Default route: ")?;
        if let Ok(index) = answer.parse::<usize>()
            && let Some(profile) =
                index.checked_sub(1).and_then(|index| candidates.get(index))
        {
            return Ok(Some(profile.route_identity()));
        }
        terminal.line("Choose one of the displayed route numbers.")?;
    }
}
