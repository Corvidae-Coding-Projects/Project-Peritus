//! Process entry and command dispatch.

use std::{
    ffi::OsString,
    io::{IsTerminal as _, Write as _},
    process::ExitCode,
};

use crate::{
    args::{Cli, Command},
    artifact, completion,
    error::CliError,
    events, operation, output, prompt, terminal,
};

/// Parses the process arguments, executes one command, and returns its stable exit category.
#[must_use]
pub fn run_env() -> ExitCode {
    run(std::env::args_os())
}

fn run(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments.len() == 1 {
        return run_interactive();
    }
    let requested_json = arguments.iter().any(|argument| argument == "--json");
    let cli = match Cli::parse(arguments) {
        Ok(cli) => cli,
        Err(error) => return report_error(&error, requested_json),
    };
    if let Command::Help { text } = &cli.command {
        return write_stdout(text.as_bytes()).map_or_else(
            |error| report_error(&CliError::output(error), cli.json),
            |()| ExitCode::SUCCESS,
        );
    }
    if matches!(&cli.command, Command::Version) {
        let line = format!("peritus {}\n", env!("CARGO_PKG_VERSION"));
        return write_stdout(line.as_bytes()).map_or_else(
            |error| report_error(&CliError::output(error), cli.json),
            |()| ExitCode::SUCCESS,
        );
    }
    if let Command::Completions(shell) = &cli.command {
        let script = completion::generate(*shell);
        return write_stdout(script.as_bytes()).map_or_else(
            |error| report_error(&CliError::output(error), cli.json),
            |()| ExitCode::SUCCESS,
        );
    }
    if matches!(&cli.command, Command::Providers) {
        return run_provider_settings();
    }
    if matches!(&cli.command, Command::Workspaces) {
        return run_workspace_settings();
    }
    if let Command::Open { path } = &cli.command {
        return run_interactive_at(path.clone());
    }

    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            return report_error(
                &CliError::runtime("construct async runtime", error.to_string()),
                cli.json,
            );
        }
    };
    let json = cli.json;
    match runtime.block_on(execute(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report_error(&error, json),
    }
}

fn run_provider_settings() -> ExitCode {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return report_error(
            &CliError::usage("provider settings require an interactive terminal"),
            false,
        );
    }
    match peritus_launcher::configure_providers_interactive() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report_error(&CliError::runtime("configure providers", error.to_string()), false)
        }
    }
}

fn run_workspace_settings() -> ExitCode {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return report_error(
            &CliError::usage("workspace settings require an interactive terminal"),
            false,
        );
    }
    match peritus_launcher::configure_workspaces_interactive() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report_error(&CliError::runtime("configure workspaces", error.to_string()), false)
        }
    }
}

fn run_interactive() -> ExitCode {
    run_interactive_at(None)
}

fn run_interactive_at(repository: Option<std::path::PathBuf>) -> ExitCode {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return report_error(
            &CliError::usage(
                "interactive launch requires a terminal; use an explicit command for automation",
            ),
            false,
        );
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            return report_error(
                &CliError::runtime("construct interactive runtime", error.to_string()),
                false,
            );
        }
    };
    match runtime.block_on(peritus_launcher::launch_interactive_at(repository)) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            report_error(&CliError::runtime("launch interactive product", error.to_string()), false)
        }
    }
}

async fn execute(cli: Cli) -> Result<(), CliError> {
    let output = output::Output::new(cli.json);
    let endpoint = cli.endpoint.ok_or_else(|| {
        CliError::usage("--endpoint <path-or-pipe> is required for daemon commands")
    })?;
    match cli.command {
        Command::Status => operation::status(&endpoint, cli.session, cli.timeout, &output).await,
        Command::Shutdown { wait } => {
            operation::shutdown(&endpoint, cli.session, cli.timeout, wait, &output).await
        }
        Command::Submit(arguments) => {
            operation::submit(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::Events(arguments) => {
            events::watch(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::ArtifactGet(arguments) => {
            artifact::get(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::ArtifactPut(arguments) => {
            artifact::put(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::ArtifactCancel(arguments) => {
            artifact::cancel(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::PromptAnswer(arguments) => {
            prompt::answer(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::PromptCancel(arguments) => {
            prompt::cancel(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::TerminalAttach(arguments) => {
            terminal::attach(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::TerminalInput(arguments) => {
            terminal::input(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::TerminalResize(arguments) => {
            terminal::resize(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::TerminalDetach(arguments) => {
            terminal::detach(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::TerminalCancel(arguments) => {
            terminal::cancel(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::Help { .. }
        | Command::Version
        | Command::Completions(_)
        | Command::Providers
        | Command::Workspaces
        | Command::Open { .. } => Ok(()),
    }
}

fn report_error(error: &CliError, json: bool) -> ExitCode {
    let payload = if json {
        serde_json::json!({
            "ok": false,
            "error": {
                "category": error.category().as_str(),
                "message": error.to_string(),
            }
        })
        .to_string()
    } else {
        format!("peritus: {}: {}", error.category().as_str(), error)
    };
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(payload.as_bytes()).and_then(|()| stderr.write_all(b"\n"));
    ExitCode::from(error.category().code())
}

fn write_stdout(bytes: &[u8]) -> Result<(), std::io::Error> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(bytes)?;
    stdout.flush()
}
