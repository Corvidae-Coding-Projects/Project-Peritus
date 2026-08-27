//! Process-facing command-line composition for `peritusd`.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::process::ExitCode;

use peritus_app_protocol::ShutdownCompletionDisposition;

use crate::{DaemonConfig, DaemonError, DaemonRuntime, ShutdownOutcome};

/// Runs the production daemon command line and returns its truthful process status.
pub fn run_cli(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut arguments = arguments.into_iter();
    let executable = arguments.next().unwrap_or_default();
    let Some(configuration) = parse(&mut arguments) else {
        usage(&executable);
        return ExitCode::from(2);
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            write_error(&format!("failed to construct daemon runtime: {error}"));
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(serve(configuration)) {
        Ok(outcome) if outcome.disposition() == ShutdownCompletionDisposition::Clean => {
            ExitCode::SUCCESS
        }
        Ok(outcome) => {
            write_error(&format!(
                "daemon shutdown was unclean: remaining={:?}, failures={:?}",
                outcome.remaining(),
                outcome.failures(),
            ));
            ExitCode::FAILURE
        }
        Err(error) => {
            write_error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}

fn parse(arguments: &mut impl Iterator<Item = OsString>) -> Option<OsString> {
    let command = arguments.next()?;
    let flag = arguments.next()?;
    let configuration = arguments.next()?;
    (command == "serve" && flag == OsStr::new("--config") && arguments.next().is_none())
        .then_some(configuration)
}

async fn serve(configuration: OsString) -> Result<ShutdownOutcome, DaemonError> {
    let config = DaemonConfig::load(configuration)?;
    let mut runtime = DaemonRuntime::start(config).await?;
    runtime.wait_for_shutdown_signal().await?;
    runtime.shutdown().await
}

fn usage(executable: &OsStr) {
    write_error(&format!(
        "usage: {} serve --config <config.toml>",
        std::path::Path::new(executable).display(),
    ));
}

fn write_error(message: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(message.as_bytes()).and_then(|()| stderr.write_all(b"\n"));
}
