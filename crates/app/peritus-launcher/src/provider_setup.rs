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
    if prepared.state().provider_setup_complete() {
        return Ok(prepared);
    }
    first_run(&prepared, &cancellation, &effects).await
}

async fn first_run(
    prepared: &PreparedProduct,
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
) -> Result<PreparedProduct, LauncherError> {
    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("Welcome to Peritus")?;
    terminal.line("Choose how Peritus may run coding agents. You can change this later.")?;
    show_catalog(&mut terminal, None)?;

    let (requested, _) = choose_provider_set(&mut terminal, Vec::new(), "offline mode")?;

    let activated =
        activate_requested(&mut terminal, requested, None, cancellation, effects).await?;
    let default = choose_default(&mut terminal, &activated.enabled)?;
    let default_route = choose_default_route(
        &mut terminal,
        default,
        &activated.direct_profiles(),
        None,
    )?;
    let automatic_failover = choose_failover(&mut terminal, activated.route_count(), false)?;
    let selection = ProviderSelection::with_routes_and_failover(
        activated.enabled.clone(),
        default_route,
        activated.direct_profiles(),
        automatic_failover,
    )?;
    persist(
        prepared,
        selection,
        effects,
        cancellation,
        &mut terminal,
        activated.direct_routes,
    )
    .await
}

/// Opens provider settings without replaying unrelated first-run setup.
pub async fn configure(
    prepared: &PreparedProduct,
) -> Result<PreparedProduct, LauncherError> {
    let cancellation = CancellationToken::new();
    let effects = ProviderEffectStore::open(prepared.layout().provider_effects_root())?;
    effects.reconcile_credentials(prepared.state().providers())?;
    let current = prepared.state().providers().clone();
    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("Provider settings")?;
    terminal
        .line("Select one or more providers. Existing credentials stay in the OS key store.")?;
    show_catalog(&mut terminal, Some(&current))?;
    let (requested, _) =
        choose_provider_set(&mut terminal, current.enabled().to_vec(), "current selection")?;
    let activated = activate_requested(
        &mut terminal,
        requested,
        Some(&current),
        &cancellation,
        &effects,
    )
    .await?;
    let default = match current.default().filter(|kind| activated.enabled.contains(kind)) {
        Some(default) => Some(default),
        None => choose_default(&mut terminal, &activated.enabled)?,
    };
    let default_route = choose_default_route(
        &mut terminal,
        default,
        &activated.direct_profiles(),
        current.default_route(),
    )?;
    let automatic_failover = choose_failover(
        &mut terminal,
        activated.route_count(),
        current.automatic_failover(),
    )?;
    let selection = ProviderSelection::with_routes_and_failover(
        activated.enabled.clone(),
        default_route,
        activated.direct_profiles(),
        automatic_failover,
    )?;
    let configured = persist(
        prepared,
        selection,
        &effects,
        &cancellation,
        &mut terminal,
        activated.direct_routes,
    )
    .await?;
    terminal.line("Provider settings saved.")?;
    Ok(configured)
}

fn show_catalog(
    terminal: &mut Terminal<'_>,
    current: Option<&ProviderSelection>,
) -> Result<(), LauncherError> {
    terminal.line("")?;
    for (index, kind) in ProviderKind::ALL.into_iter().enumerate() {
        if kind.is_account() {
            let status = if current.is_some_and(|selection| selection.enabled().contains(&kind)) {
                "selected; retained without a launch-time health probe"
            } else {
                "Health checked only when selected"
            };
            terminal.line(&format!("  {}. {:<38} {status}", index + 1, kind.label()))?;
            continue;
        }
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
    requested: Vec<ProviderKind>,
    existing: Option<&ProviderSelection>,
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
) -> Result<ActivatedProviders, LauncherError> {
    let mut enabled = Vec::new();
    let mut direct_routes = Vec::new();
    for kind in requested {
        if kind.is_direct() {
            let existing_profiles = existing
                .into_iter()
                .flat_map(ProviderSelection::direct_profiles)
                .filter(|profile| profile.kind() == kind)
                .collect::<Vec<_>>();
            enabled.push(kind);
            if existing_profiles.is_empty() {
                direct_routes.push(direct::setup(terminal, kind, effects, cancellation).await?);
            } else {
                for profile in existing_profiles {
                    direct_routes.push(
                        connection::existing(terminal, profile, effects, cancellation).await?,
                    );
                }
            }
            continue;
        }
        let retained = existing.is_some_and(|selection| selection.enabled().contains(&kind));
        if retained {
            enabled.push(kind);
            continue;
        }
        let mut observations = ProviderCatalog::observe_selected(&[kind], cancellation).await?;
        let item = observations.pop().ok_or_else(|| {
            LauncherError::Interaction("selected account provider was not observed".to_owned())
        })?;
        terminal.line(&format!("\n{} — {}", kind.label(), item.status().label()))?;
        show_diagnostic(terminal, &item)?;
        match item.status() {
            ProviderStatus::Ready => {}
            ProviderStatus::SignedOut => {
                terminal.line(&format!("\n{} requires sign-in.", kind.label()))?;
                let _outcome = login(terminal, kind, cancellation).await?;
            }
            ProviderStatus::Unavailable => {
                if install::offer(terminal, kind, cancellation, effects).await? {
                    let _outcome = login(terminal, kind, cancellation).await?;
                } else {
                    installation_guidance(terminal, kind)?;
                }
            }
            ProviderStatus::Unknown
            | ProviderStatus::Infrastructure
            | ProviderStatus::NeedsAttention => {
                terminal.line(&format!("{} could not report a usable login status.", kind.label()))?;
            }
        }
        enabled.push(kind);
    }
    if enabled.is_empty() {
        terminal.line("Continuing in offline browse mode. Agent runs will ask for a provider.")?;
    }
    Ok(ActivatedProviders { enabled, direct_routes })
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
    let observation = match {
        let _title = crate::terminal::product_title()?;
        provider.login(mode, cancellation).await
    } {
        Ok(observation) => observation,
        Err(error @ peritus_provider_onboarding::OnboardingError::Cancelled) => {
            return Err(error.into());
        }
        Err(error) => {
            terminal.line(&format!("{} sign-in could not be verified: {error}", kind.label()))?;
            return Ok(LoginOutcome::Unverified);
        }
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
    terminal: &mut Terminal<'_>,
    direct_routes: Vec<direct::PreparedRoute>,
) -> Result<PreparedProduct, LauncherError> {
    let selection = models::account_selections(
        terminal,
        selection,
        prepared.state().providers(),
        cancellation,
    )
    .await?;
    let layout = prepared.layout().clone();
    let configured = ProductBootstrap::new(layout).configure_providers(selection)?;
    for route in direct_routes {
        route.publish(terminal, effects, cancellation).await?;
    }
    effects.reconcile_credentials(configured.state().providers())?;
    Ok(configured)
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
    direct_routes: Vec<direct::PreparedRoute>,
}

impl ActivatedProviders {
    fn route_count(&self) -> usize {
        self.enabled.iter().filter(|kind| kind.is_account()).count()
            + self.direct_routes.len()
    }

    fn direct_profiles(&self) -> Vec<peritus_product_state::DirectProviderProfile> {
        self.direct_routes
            .iter()
            .map(|route| route.profile().clone())
            .collect()
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
