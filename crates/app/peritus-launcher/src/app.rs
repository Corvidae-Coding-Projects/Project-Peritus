//! End-to-end interactive product launch composition.

use std::{future::Future, path::PathBuf};

use peritus_product_state::WorkspaceProfile;
use peritus_provider_core::CancellationToken;
use peritus_tui::{ExitReason, ProductLaunchContext, ProductProviderOption, TuiConfig};
use peritus_types::{ProviderProfileId, WorkspaceId};

use crate::{
    AppLayout, DaemonSupervisor, LauncherError, ProductBootstrap, SiblingBinaries, provider_setup,
    update, workspace_setup,
};

mod diagnostics;
mod navigation;

/// Prepares local state, starts or reuses the daemon, and runs the interactive application.
///
/// # Errors
///
/// Returns an actionable product-boundary failure when platform setup, daemon readiness, or the
/// terminal application cannot complete.
pub async fn launch_interactive() -> Result<ExitReason, LauncherError> {
    launch_interactive_at(None).await
}

/// Launches the product with an optional explicit repository path.
///
/// # Errors
///
/// Returns an actionable product-boundary failure when workspace setup, platform setup, daemon
/// readiness, or the terminal application cannot complete.
pub async fn launch_interactive_at(
    repository: Option<PathBuf>,
) -> Result<ExitReason, LauncherError> {
    launch_interactive_run(repository, None, None).await
}

/// Opens an exact conversation while preserving normal workspace setup and recovery.
///
/// # Errors
/// Returns a setup failure, mismatched endpoint, or terminal failure without changing targets.
pub async fn launch_interactive_run(
    repository: Option<PathBuf>,
    run: Option<peritus_types::RunId>,
    endpoint: Option<std::ffi::OsString>,
) -> Result<ExitReason, LauncherError> {
    launch_interactive_target(repository, InitialConversation::Run(run), endpoint).await
}

/// Opens the most recently active conversation for the current directory's workspace.
///
/// # Errors
/// Returns a setup failure, mismatched endpoint, or terminal failure without changing targets.
pub async fn launch_interactive_resume(
    endpoint: Option<std::ffi::OsString>,
) -> Result<ExitReason, LauncherError> {
    launch_interactive_target(None, InitialConversation::Latest, endpoint).await
}

#[derive(Clone, Copy)]
enum InitialConversation {
    Run(Option<peritus_types::RunId>),
    Latest,
}

async fn launch_interactive_target(
    repository: Option<PathBuf>,
    initial: InitialConversation,
    endpoint: Option<std::ffi::OsString>,
) -> Result<ExitReason, LauncherError> {
    let _title = crate::terminal::product_title()?;
    let layout = AppLayout::discover()?.prepare()?;
    if update::offer_on_startup(&layout).await? {
        return Ok(ExitReason::UserQuit);
    }
    let prepared = ProductBootstrap::new(layout).prepare()?;
    let prepared = workspace_setup::ensure_configured(prepared, repository.as_deref())?;
    let mut prepared = provider_setup::ensure_configured(prepared).await?;
    let discovery_cancellation = CancellationToken::new();
    let binaries = interruptible(
        &discovery_cancellation,
        SiblingBinaries::discover_cancellable(&discovery_cancellation),
    )
    .await?;
    let mut supervisor = DaemonSupervisor::without_deadline();
    if endpoint.as_deref().is_some_and(|endpoint| endpoint != prepared.endpoint_path()) {
        return Err(LauncherError::Interaction("The selected workspace uses a different daemon endpoint. Reconnect the browser to its configured daemon.".into()));
    }
    let product = match initial {
        InitialConversation::Run(run) => product_context(&prepared)?.with_run(run),
        InitialConversation::Latest => product_context(&prepared)?.with_latest_conversation(),
    };
    let report = diagnostics::launcher_report(&prepared, &binaries, product.workspace_id())?;
    let mut product = product.with_launcher_report(report).map_err(LauncherError::Tui)?;
    let mut tui_state = peritus_tui::TuiState::default();
    loop {
        let readiness_cancellation = CancellationToken::new();
        interruptible(
            &readiness_cancellation,
            supervisor.reconcile_ready_cancellable(
                &mut prepared,
                &product,
                &binaries,
                &readiness_cancellation,
            ),
        )
        .await?;
        let outcome = peritus_tui::run_with_state(
            TuiConfig::new(prepared.endpoint_path()).with_product(product.clone()),
            &mut tui_state,
        )
        .await
        .map_err(LauncherError::Tui)?;
        match outcome {
            ExitReason::UserQuit => return Ok(outcome),
            ExitReason::RecoverDaemon => {}
            ExitReason::OpenRun { run, workspace } => {
                match navigation::run_context(&prepared, &binaries, run, workspace) {
                    Ok(context) => product = context,
                    Err(error) => tui_state.conversation_open_failed(&error.to_string()),
                }
            }
            ExitReason::OpenConversation(query) => {
                match navigation::conversation_context(&prepared, &binaries, query) {
                    Ok(context) => product = context,
                    Err(error) => tui_state.conversation_open_failed(&error.to_string()),
                }
            }
        }
    }
}

async fn interruptible<T>(
    cancellation: &CancellationToken,
    operation: impl Future<Output = Result<T, LauncherError>>,
) -> Result<T, LauncherError> {
    tokio::pin!(operation);
    tokio::select! {
        result = &mut operation => result,
        signal = tokio::signal::ctrl_c() => {
            let _ = cancellation.cancel();
            let result = operation.await;
            match signal {
                Ok(()) => result,
                Err(error) => Err(LauncherError::Interaction(format!(
                    "cannot observe startup cancellation: {error}",
                ))),
            }
        }
    }
}

/// Checks for and installs the latest public release without requiring configuration exports.
///
/// # Errors
///
/// Returns an actionable update failure when release discovery, verification, or native package
/// installation cannot complete.
pub async fn update_interactive() -> Result<(), LauncherError> {
    let layout = AppLayout::discover()?.prepare()?;
    update::run_explicit(&layout).await
}

/// Persists whether ordinary interactive startup performs a cached release check.
///
/// # Errors
///
/// Returns an actionable filesystem or terminal failure when the setting cannot be saved or shown.
pub fn configure_update_checks(enabled: bool) -> Result<(), LauncherError> {
    let layout = AppLayout::discover()?.prepare()?;
    update::configure_checks(&layout, enabled)
}

fn product_context(
    prepared: &crate::PreparedProduct,
) -> Result<ProductLaunchContext, LauncherError> {
    let workspace = prepared.state().workspaces().active().ok_or_else(|| {
        LauncherError::WorkspaceSetup("no active workspace is available after setup".to_owned())
    })?;
    workspace_context(prepared, workspace)
}

fn workspace_context(
    prepared: &crate::PreparedProduct,
    workspace: &WorkspaceProfile,
) -> Result<ProductLaunchContext, LauncherError> {
    let workspace_id = WorkspaceId::new(decode_id(workspace.workspace_id())?).map_err(|error| {
        LauncherError::WorkspaceSetup(format!("active workspace identity is invalid: {error:?}"))
    })?;
    let routes = prepared.state().providers().routes();
    let providers = routes
        .iter()
        .map(|route| {
            let label = if routes.iter().filter(|other| other.kind() == route.kind()).count() > 1 {
                let identity = route.identity().to_string();
                let model = prepared
                    .state()
                    .providers()
                    .direct_profile_by_route(route.identity())
                    .map_or("account route", peritus_product_state::DirectProviderProfile::model);
                format!("{} / {model} ({})", route.kind().label(), &identity[..8])
            } else {
                route.kind().label().to_owned()
            };
            ProviderProfileId::new(*route.identity().as_bytes())
                .map(|profile| ProductProviderOption::new(profile, label))
                .map_err(|error| {
                    LauncherError::WorkspaceSetup(format!(
                        "provider profile identity is invalid: {error:?}"
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let default = prepared
        .state()
        .providers()
        .default_route()
        .and_then(|selected| routes.iter().position(|route| route.identity() == selected));
    ProductLaunchContext::new(
        workspace_id,
        workspace.managed_root().unwrap_or_else(|| workspace.repository_root()).to_owned(),
        providers,
        default,
    )
    .map(|context| {
        if workspace.is_direct_folder() {
            context.with_direct_folder(
                workspace.trust_level() == peritus_product_state::WorkspaceTrust::Trusted,
            )
        } else {
            context
        }
    })
    .map_err(LauncherError::Tui)
}

fn decode_id(value: &str) -> Result<[u8; 16], LauncherError> {
    if value.len() != 32 {
        return Err(LauncherError::WorkspaceSetup(
            "workspace identity must contain 32 hexadecimal digits".to_owned(),
        ));
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).map_err(|_| {
            LauncherError::WorkspaceSetup("workspace identity is not UTF-8 hexadecimal".to_owned())
        })?;
        bytes[index] = u8::from_str_radix(text, 16).map_err(|_| {
            LauncherError::WorkspaceSetup("workspace identity is not hexadecimal".to_owned())
        })?;
    }
    Ok(bytes)
}

/// Opens provider settings using platform-local state without requiring daemon details.
///
/// # Errors
///
/// Returns an actionable bootstrap, interaction, provider, or configuration failure.
pub async fn configure_providers_interactive() -> Result<(), LauncherError> {
    let _title = crate::terminal::product_title()?;
    let layout = AppLayout::discover()?.prepare()?;
    let prepared = ProductBootstrap::new(layout).prepare()?;
    let _configured = provider_setup::configure(&prepared).await?;
    Ok(())
}

/// Opens workspace settings without requiring endpoint paths or environment configuration.
///
/// # Errors
///
/// Returns an actionable bootstrap, interaction, Git, registration, or configuration failure.
pub fn configure_workspaces_interactive() -> Result<(), LauncherError> {
    let layout = AppLayout::discover()?.prepare()?;
    let prepared = ProductBootstrap::new(layout).prepare()?;
    let _configured = workspace_setup::configure(prepared)?;
    Ok(())
}
