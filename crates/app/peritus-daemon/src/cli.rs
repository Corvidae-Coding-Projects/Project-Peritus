//! Process-facing command-line composition for `peritusd`.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use peritus_app_protocol::ShutdownCompletionDisposition;

use crate::outbox::{
    recover_blob_after_crash, recover_blob_before_crash, recover_journal_before_crash,
    recover_outbox_crash, stage_blob_after_crash, stage_blob_before_crash,
    stage_journal_before_crash, stage_outbox_crash,
};
use crate::terminal::qualify_pty_ordering;
use crate::{DaemonConfig, DaemonError, DaemonRuntime, ShutdownOutcome};

const QUALIFICATION_KILL_BOUND: Duration = Duration::from_secs(30);

/// Runs the production daemon command line and returns its truthful process status.
pub fn run_cli(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut arguments = arguments.into_iter();
    let executable = arguments.next().unwrap_or_default();
    let Some(command) = parse(&mut arguments) else {
        usage(&executable);
        return ExitCode::from(2);
    };
    match command {
        CommandLine::Version => write_output(&format!("peritusd {}", env!("CARGO_PKG_VERSION")))
            .map_or_else(output_failure, |()| ExitCode::SUCCESS),
        CommandLine::Serve(configuration) => run_server(configuration),
        CommandLine::QualifyPty => qualify_pty(),
        CommandLine::StageBlobBeforeCrash(configuration) => stage_blob_before(configuration),
        CommandLine::RecoverBlobBeforeCrash(configuration) => recover_blob_before(configuration),
        CommandLine::StageBlobAfterCrash(configuration) => stage_blob_after(configuration),
        CommandLine::RecoverBlobAfterCrash(configuration) => recover_blob_after(configuration),
        CommandLine::StageJournalBeforeCrash(configuration) => stage_journal_before(configuration),
        CommandLine::RecoverJournalBeforeCrash(configuration) => {
            recover_journal_before(configuration)
        }
        CommandLine::StageOutboxCrash(configuration) => stage_outbox(configuration),
        CommandLine::RecoverOutboxCrash(configuration) => recover_outbox(configuration),
    }
}

fn run_server(configuration: OsString) -> ExitCode {
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

enum CommandLine {
    Version,
    Serve(OsString),
    QualifyPty,
    StageBlobBeforeCrash(OsString),
    RecoverBlobBeforeCrash(OsString),
    StageBlobAfterCrash(OsString),
    RecoverBlobAfterCrash(OsString),
    StageJournalBeforeCrash(OsString),
    RecoverJournalBeforeCrash(OsString),
    StageOutboxCrash(OsString),
    RecoverOutboxCrash(OsString),
}

fn parse(arguments: &mut impl Iterator<Item = OsString>) -> Option<CommandLine> {
    let command = arguments.next()?;
    match command.to_str()? {
        "--version" if arguments.next().is_none() => Some(CommandLine::Version),
        "serve" => configuration_argument(arguments).map(CommandLine::Serve),
        "qualify-pty" if arguments.next().is_none() => Some(CommandLine::QualifyPty),
        "qualify-blob-before-stage" => {
            configuration_argument(arguments).map(CommandLine::StageBlobBeforeCrash)
        }
        "qualify-blob-before-recover" => {
            configuration_argument(arguments).map(CommandLine::RecoverBlobBeforeCrash)
        }
        "qualify-blob-after-stage" => {
            configuration_argument(arguments).map(CommandLine::StageBlobAfterCrash)
        }
        "qualify-blob-after-recover" => {
            configuration_argument(arguments).map(CommandLine::RecoverBlobAfterCrash)
        }
        "qualify-journal-before-stage" => {
            configuration_argument(arguments).map(CommandLine::StageJournalBeforeCrash)
        }
        "qualify-journal-before-recover" => {
            configuration_argument(arguments).map(CommandLine::RecoverJournalBeforeCrash)
        }
        "qualify-outbox-stage" => {
            configuration_argument(arguments).map(CommandLine::StageOutboxCrash)
        }
        "qualify-outbox-recover" => {
            configuration_argument(arguments).map(CommandLine::RecoverOutboxCrash)
        }
        _ => None,
    }
}

fn stage_blob_before(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_blob_before_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification blob-before-stage digest={} bytes={} temporary_files={}",
        checkpoint.digest(),
        checkpoint.bytes(),
        checkpoint.temporary_files(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error(&format!(
        "blob-before qualifier was not killed for digest {}",
        checkpoint.digest(),
    ));
    ExitCode::FAILURE
}

fn recover_blob_before(configuration: OsString) -> ExitCode {
    recover_blob(configuration, false)
}

fn stage_blob_after(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_blob_after_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification blob-after-stage digest={} bytes={} finalized=true referenced=true",
        checkpoint.digest(),
        checkpoint.bytes(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error(&format!("blob-after qualifier was not killed for digest {}", checkpoint.digest()));
    ExitCode::FAILURE
}

fn recover_blob_after(configuration: OsString) -> ExitCode {
    recover_blob(configuration, true)
}

fn recover_blob(configuration: OsString, after_commit: bool) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let observation = if after_commit {
        recover_blob_after_crash(&config)
    } else {
        recover_blob_before_crash(&config)
    };
    match observation {
        Ok(observation) => {
            let timing = if after_commit { "after" } else { "before" };
            let line = format!(
                "peritus-qualification blob-{timing}-recover digest={} bytes={} journal_verified={} finalized={} referenced={} temporary_files={} object_files={}",
                observation.digest(),
                observation.bytes(),
                observation.journal_verified(),
                observation.finalized(),
                observation.referenced(),
                observation.temporary_files(),
                observation.object_files(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

fn stage_journal_before(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_journal_before_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification journal-before-stage request_sha256={}",
        checkpoint.request_sha256(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error(&format!(
        "journal-before crash qualifier was not killed at request {}",
        checkpoint.request_sha256(),
    ));
    ExitCode::FAILURE
}

fn recover_journal_before(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match recover_journal_before_crash(&config) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification journal-before-recover request_sha256={} journal_verified={} committed_events={} aggregate_heads={} external_effects={} pending_claims={}",
                observation.request_sha256(),
                observation.journal_verified(),
                observation.committed_events(),
                observation.aggregate_heads(),
                observation.external_effects(),
                observation.pending_claims(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

fn configuration_argument(arguments: &mut impl Iterator<Item = OsString>) -> Option<OsString> {
    let flag = arguments.next()?;
    let configuration = arguments.next()?;
    (flag == OsStr::new("--config") && arguments.next().is_none()).then_some(configuration)
}

fn qualify_pty() -> ExitCode {
    match qualify_pty_ordering() {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification pty output_bytes={} sequence_strictly_increasing={} offsets_conserved={} combined_stream_only={} exit_records={} peak_buffered_bytes={} configured_buffer_limit={}",
                observation.output_bytes(),
                observation.sequence_strictly_increasing(),
                observation.offsets_conserved(),
                observation.combined_stream_only(),
                observation.exit_records(),
                observation.peak_buffered_bytes(),
                observation.configured_buffer_limit(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

fn stage_outbox(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_outbox_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification outbox-stage effect_path={} claim_fence={}",
        checkpoint.effect_path().display(),
        checkpoint.claim_fence(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("outbox crash qualifier was not killed at its published checkpoint");
    ExitCode::FAILURE
}

fn recover_outbox(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match recover_outbox_crash(&config) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification outbox-recover destination_reconciled={} external_effects={} duplicate_effects={} exact_fence_acknowledged={} pending_claims={}",
                observation.destination_reconciled(),
                observation.external_effects(),
                observation.duplicate_effects(),
                observation.exact_fence_acknowledged(),
                observation.pending_claims(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

fn qualification_failure(error: &impl std::fmt::Display) -> ExitCode {
    write_error(&error.to_string());
    ExitCode::FAILURE
}

fn output_failure(error: std::io::Error) -> ExitCode {
    write_error(&format!("failed to write qualification observation: {error}"));
    ExitCode::FAILURE
}

async fn serve(configuration: OsString) -> Result<ShutdownOutcome, DaemonError> {
    let config = DaemonConfig::load(configuration)?;
    let mut runtime = DaemonRuntime::start(config).await?;
    runtime.wait_for_shutdown_signal().await?;
    runtime.shutdown().await
}

fn usage(executable: &OsStr) {
    write_error(&format!(
        "usage: {} --version | serve --config <config.toml> | qualify-pty | qualify-blob-before-stage --config <config.toml> | qualify-blob-before-recover --config <config.toml> | qualify-blob-after-stage --config <config.toml> | qualify-blob-after-recover --config <config.toml> | qualify-journal-before-stage --config <config.toml> | qualify-journal-before-recover --config <config.toml> | qualify-outbox-stage --config <config.toml> | qualify-outbox-recover --config <config.toml>",
        std::path::Path::new(executable).display(),
    ));
}

fn write_output(message: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(message.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

fn write_error(message: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(message.as_bytes()).and_then(|()| stderr.write_all(b"\n"));
}
