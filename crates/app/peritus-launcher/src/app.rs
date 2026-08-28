//! End-to-end interactive product launch composition.

use std::time::Duration;

use peritus_tui::{ExitReason, TuiConfig};

use crate::{
    AppLayout, DaemonSupervisor, LauncherError, ProductBootstrap, SiblingBinaries, provider_setup,
};

/// Prepares local state, starts or reuses the daemon, and runs the interactive application.
///
/// # Errors
///
/// Returns an actionable product-boundary failure when platform setup, daemon readiness, or the
/// terminal application cannot complete.
pub async fn launch_interactive() -> Result<ExitReason, LauncherError> {
    let layout = AppLayout::discover()?.prepare()?;
    let prepared = ProductBootstrap::new(layout).prepare()?;
    let prepared = provider_setup::ensure_configured(prepared)?;
    let binaries = SiblingBinaries::discover()?;
    let supervisor = DaemonSupervisor::new(Duration::from_secs(30));
    supervisor.ensure_ready(&prepared, &binaries).await?;
    peritus_tui::run(TuiConfig::new(prepared.endpoint_path())).await.map_err(LauncherError::Tui)
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
