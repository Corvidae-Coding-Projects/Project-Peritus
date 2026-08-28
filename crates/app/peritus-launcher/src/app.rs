//! End-to-end interactive product launch composition.

use std::{path::PathBuf, time::Duration};

use peritus_product_state::ProviderKind;
use peritus_tui::{ExitReason, ProductLaunchContext, ProductProviderOption, TuiConfig};
use peritus_types::{ProviderProfileId, WorkspaceId};

use crate::{
    AppLayout, DaemonSupervisor, LauncherError, ProductBootstrap, SiblingBinaries, provider_setup,
    workspace_setup,
};

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
    let layout = AppLayout::discover()?.prepare()?;
    let prepared = ProductBootstrap::new(layout).prepare()?;
    let prepared = workspace_setup::ensure_configured(prepared, repository.as_deref())?;
    let prepared = provider_setup::ensure_configured(prepared)?;
    let binaries = SiblingBinaries::discover()?;
    let supervisor = DaemonSupervisor::new(Duration::from_secs(30));
    supervisor.ensure_ready(&prepared, &binaries).await?;
    let product = product_context(&prepared)?;
    peritus_tui::run(TuiConfig::new(prepared.endpoint_path()).with_product(product))
        .await
        .map_err(LauncherError::Tui)
}

fn product_context(
    prepared: &crate::PreparedProduct,
) -> Result<ProductLaunchContext, LauncherError> {
    let workspace = prepared.state().workspaces().active().ok_or_else(|| {
        LauncherError::WorkspaceSetup("no active workspace is available after setup".to_owned())
    })?;
    let workspace_id = WorkspaceId::new(decode_id(workspace.workspace_id())?).map_err(|error| {
        LauncherError::WorkspaceSetup(format!("active workspace identity is invalid: {error:?}"))
    })?;
    let providers = prepared
        .state()
        .providers()
        .enabled()
        .iter()
        .map(|kind| {
            ProviderProfileId::new(provider_id(*kind))
                .map(|profile| ProductProviderOption::new(profile, kind.label()))
                .map_err(|error| {
                    LauncherError::WorkspaceSetup(format!(
                        "provider profile identity is invalid: {error:?}"
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let default = prepared.state().providers().default().and_then(|selected| {
        prepared.state().providers().enabled().iter().position(|kind| *kind == selected)
    });
    ProductLaunchContext::new(
        workspace_id,
        workspace.managed_root().unwrap_or_else(|| workspace.repository_root()).to_owned(),
        providers,
        default,
    )
    .map_err(LauncherError::Tui)
}

const fn provider_id(kind: ProviderKind) -> [u8; 16] {
    match kind {
        ProviderKind::CodexAccount => [0xa1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        ProviderKind::ClaudeAccount => [0xa2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
        ProviderKind::OpenAiApi => [0xa3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3],
        ProviderKind::AnthropicApi => [0xa4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4],
        ProviderKind::GoogleGeminiApi => [0xa5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5],
        ProviderKind::CompatibleEndpoint => [0xa6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6],
    }
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
pub fn configure_providers_interactive() -> Result<(), LauncherError> {
    let layout = AppLayout::discover()?.prepare()?;
    let prepared = ProductBootstrap::new(layout).prepare()?;
    let _configured = provider_setup::configure(&prepared)?;
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
