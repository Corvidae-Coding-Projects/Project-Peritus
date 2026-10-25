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
    events, operation, output, product_run, prompt, terminal,
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
    if let Some(exit) = run_informational_command(&cli) {
        return exit;
    }
    if let Some(exit) = run_local_command(&cli) {
        return exit;
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

fn run_informational_command(cli: &Cli) -> Option<ExitCode> {
    let result = match &cli.command {
        Command::Help { text } => {
            write_stdout(text.as_bytes()).map_err(CliError::output).map(|()| ExitCode::SUCCESS)
        }
        Command::Version => {
            let line = format!("peritus {}\n", env!("CARGO_PKG_VERSION"));
            write_stdout(line.as_bytes()).map_err(CliError::output).map(|()| ExitCode::SUCCESS)
        }
        Command::Completions(shell) => {
            let script = completion::generate(*shell);
            write_stdout(script.as_bytes()).map_err(CliError::output).map(|()| ExitCode::SUCCESS)
        }
        Command::Modules { arguments } => crate::modules::execute(arguments).and_then(|value| {
            write_stdout(format!("{value}\n").as_bytes())
                .map_err(CliError::output)
                .map(|()| ExitCode::SUCCESS)
        }),
        Command::Web { arguments } => crate::web::execute(arguments).and_then(|value| {
            write_stdout(format!("{value}\n").as_bytes())
                .map_err(CliError::output)
                .map(|()| ExitCode::SUCCESS)
        }),
        Command::ProductRunTrace { run_id, extensions_only } => {
            let output = output::Output::new(cli.json);
            product_run::trace(*run_id, *extensions_only, &output).map(|()| ExitCode::SUCCESS)
        }
        _ => return None,
    };
    Some(result.unwrap_or_else(|error| report_error(&error, cli.json)))
}

fn run_local_command(cli: &Cli) -> Option<ExitCode> {
    match &cli.command {
        Command::Automation { setup } => {
            let result = if *setup {
                peritus_tools_computer::setup()
            } else {
                peritus_tools_computer::status()
            };
            Some(match result {
                Ok(value) => write_stdout(format!("{value}\n").as_bytes()).map_or_else(
                    |error| report_error(&CliError::output(error), cli.json),
                    |()| ExitCode::SUCCESS,
                ),
                Err(error) => report_error(
                    &CliError::runtime("configure computer automation", error),
                    cli.json,
                ),
            })
        }
        Command::Update(arguments) => Some(run_update(arguments.automatic_checks)),
        Command::Providers => Some(run_provider_settings()),
        Command::Workspaces => Some(run_workspace_settings()),
        Command::Open { path, run, conversation } => Some(conversation.as_ref().map_or_else(
            || run_interactive_at(path.clone(), *run, cli.endpoint.clone()),
            |conversation| {
                run_interactive_target(
                    InteractiveLaunch::Conversation {
                        repository: path.clone(),
                        conversation: *conversation,
                    },
                    cli.endpoint.clone(),
                )
            },
        )),
        Command::Resume => Some(run_interactive_resume(cli.endpoint.clone())),
        _ => None,
    }
}

fn run_update(automatic_checks: Option<bool>) -> ExitCode {
    if !std::io::stdout().is_terminal() {
        return report_error(
            &CliError::usage("product update requires an interactive terminal"),
            false,
        );
    }
    if let Some(enabled) = automatic_checks {
        return match peritus_launcher::configure_update_checks(enabled) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => report_error(
                &CliError::runtime("configure update checks", error.to_string()),
                false,
            ),
        };
    }
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            return report_error(
                &CliError::runtime("construct update runtime", error.to_string()),
                false,
            );
        }
    };
    match runtime.block_on(peritus_launcher::update_interactive()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report_error(&CliError::runtime("update product", error.to_string()), false),
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
    run_interactive_at(None, None, None)
}

enum InteractiveLaunch {
    Conversation {
        repository: Option<std::path::PathBuf>,
        conversation: peritus_app_protocol::ConversationId,
    },
    Open {
        repository: Option<std::path::PathBuf>,
        run: Option<peritus_types::RunId>,
    },
    Resume,
}

fn run_interactive_at(
    repository: Option<std::path::PathBuf>,
    run: Option<peritus_types::RunId>,
    endpoint: Option<OsString>,
) -> ExitCode {
    run_interactive_target(InteractiveLaunch::Open { repository, run }, endpoint)
}

fn run_interactive_resume(endpoint: Option<OsString>) -> ExitCode {
    run_interactive_target(InteractiveLaunch::Resume, endpoint)
}

fn run_interactive_target(target: InteractiveLaunch, endpoint: Option<OsString>) -> ExitCode {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return report_error(
            &CliError::usage(
                "interactive launch requires a terminal; use an explicit command for automation",
            ),
            false,
        );
    }
    // Portable/source and system-package installs have no per-user installer hook.
    // Provision once here; a transient download failure must not disable offline coding.
    if let Err(error) = peritus_tools_computer::ensure_setup() {
        eprintln!("Peritus browser automation setup is incomplete: {error}");
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
    let (operation, result) = match target {
        InteractiveLaunch::Conversation { repository, conversation } => (
            "open durable conversation",
            runtime.block_on(peritus_launcher::launch_interactive_conversation(
                repository,
                conversation,
                endpoint,
            )),
        ),
        InteractiveLaunch::Open { repository, run } => (
            "launch interactive product",
            runtime.block_on(peritus_launcher::launch_interactive_run(repository, run, endpoint)),
        ),
        InteractiveLaunch::Resume => (
            "resume interactive product",
            runtime.block_on(peritus_launcher::launch_interactive_resume(endpoint)),
        ),
    };
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => report_error(&CliError::runtime(operation, error.to_string()), false),
    }
}

async fn execute(cli: Cli) -> Result<(), CliError> {
    let output = output::Output::new(cli.json);
    if cli.endpoint.is_none() && matches!(&cli.command, Command::Shutdown { .. }) {
        return operation::shutdown_local(cli.session, cli.timeout, &output).await;
    }
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
        Command::Improvements(request) => {
            crate::improvements::execute(&endpoint, cli.session, cli.timeout, request, &output)
                .await
        }
        Command::ProductRuns(arguments) => {
            product_run::execute(&endpoint, cli.session, cli.timeout, arguments, &output).await
        }
        Command::Help { .. }
        | Command::Version
        | Command::Completions(_)
        | Command::Update(_)
        | Command::Providers
        | Command::Modules { .. }
        | Command::Web { .. }
        | Command::Automation { .. }
        | Command::Workspaces
        | Command::Open { .. }
        | Command::ProductRunTrace { .. }
        | Command::Resume => Ok(()),
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
