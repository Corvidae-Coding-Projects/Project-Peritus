//! Tokio runtime ownership for the long-lived daemon command.

use std::ffi::OsString;
use std::process::ExitCode;

use peritus_app_protocol::ShutdownCompletionDisposition;

use crate::{
    DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery, DaemonRuntime, ShutdownOutcome,
};

pub(super) fn run(configuration: OsString) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            super::write_error(&format!("failed to construct daemon runtime: {error}"));
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(serve(configuration)) {
        Ok(outcome) if outcome.disposition() == ShutdownCompletionDisposition::Clean => {
            ExitCode::SUCCESS
        }
        Ok(outcome) => {
            super::write_error(&format!(
                "daemon shutdown was unclean: remaining={:?}, failures={:?}",
                outcome.remaining(),
                outcome.failures(),
            ));
            ExitCode::FAILURE
        }
        Err(error) => {
            super::write_error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}

async fn serve(configuration: OsString) -> Result<ShutdownOutcome, DaemonError> {
    let config = with_installed_process_watchdog(DaemonConfig::load(configuration)?)?;
    let mut runtime = DaemonRuntime::start(config).await?;
    runtime.wait_for_shutdown_signal().await?;
    runtime.shutdown().await
}

fn with_installed_process_watchdog(config: DaemonConfig) -> Result<DaemonConfig, DaemonError> {
    #[cfg(target_os = "linux")]
    {
        let daemon = std::env::current_exe().map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::RecoveryRequired,
                DaemonRecovery::Operator,
                "resolve process crash watchdog",
                "installed daemon executable cannot be resolved",
                error,
            )
        })?;
        Ok(config.with_process_crash_watchdog(daemon))
    }
    #[cfg(not(target_os = "linux"))]
    Ok(config)
}
