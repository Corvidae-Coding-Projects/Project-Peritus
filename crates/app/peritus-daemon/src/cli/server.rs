//! Tokio runtime ownership for the long-lived daemon command.

use std::ffi::OsString;
use std::process::ExitCode;

use peritus_app_protocol::ShutdownCompletionDisposition;

use crate::{DaemonConfig, DaemonError, DaemonRuntime, ShutdownOutcome};

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
    let config = DaemonConfig::load(configuration)?;
    let mut runtime = DaemonRuntime::start(config).await?;
    runtime.wait_for_shutdown_signal().await?;
    runtime.shutdown().await
}
