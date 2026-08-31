//! Process-facing command-line composition for `peritusd`.

mod arguments;
mod blob_corruption;
#[cfg(not(verus_only))]
mod daemon_lifecycle;
#[cfg(not(verus_only))]
mod dependency;
mod disk;
mod evidence_corruption;
mod gate;
mod journal_corruption;
mod lease;
mod patch;
mod projection;
mod promotion;
mod promotion_evidence_corruption;
mod reboot;
mod server;
mod snapshot;
mod snapshot_corruption;
mod usage;

use std::{ffi::OsString, io::Write, process::ExitCode, time::Duration};

use crate::DaemonConfig;
use crate::outbox::{
    recover_blob_after_crash, recover_blob_before_crash, recover_journal_before_crash,
    recover_outbox_crash, stage_blob_after_crash, stage_blob_before_crash,
    stage_journal_before_crash, stage_outbox_crash,
};
use crate::terminal::qualify_pty_ordering;
use arguments::{CommandLine, parse};

const QUALIFICATION_KILL_BOUND: Duration = Duration::from_secs(30);

/// Runs the production daemon command line and returns its truthful process status.
pub fn run_cli(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut arguments = arguments.into_iter();
    let executable = arguments.next().unwrap_or_default();
    let Some(command) = parse(&mut arguments) else {
        usage::write(&executable);
        return ExitCode::from(2);
    };
    match command {
        CommandLine::Version => write_output(&format!("peritusd {}", env!("CARGO_PKG_VERSION")))
            .map_or_else(output_failure, |()| ExitCode::SUCCESS),
        CommandLine::Serve(configuration) => server::run(configuration),
        CommandLine::QualifyPty => qualify_pty(),
        CommandLine::StageBlobBeforeCrash(configuration) => stage_blob_before(configuration),
        CommandLine::RecoverBlobBeforeCrash(configuration) => recover_blob_before(configuration),
        CommandLine::StageBlobAfterCrash(configuration) => stage_blob_after(configuration),
        CommandLine::RecoverBlobAfterCrash(configuration) => recover_blob_after(configuration),
        CommandLine::StageBlobCorruption(configuration) => blob_corruption::stage(configuration),
        CommandLine::RecoverBlobCorruption(configuration) => {
            blob_corruption::recover(configuration)
        }
        CommandLine::StageBlobFinalizeExhaustion(configuration) => {
            disk::stage_blob_finalize_exhaustion(configuration)
        }
        CommandLine::RecoverBlobFinalizeExhaustion(configuration) => {
            disk::recover_blob_finalize_exhaustion(configuration)
        }
        CommandLine::StageJournalAppendExhaustion(configuration) => {
            disk::stage_journal_append_exhaustion(configuration)
        }
        CommandLine::RecoverJournalAppendExhaustion(configuration) => {
            disk::recover_journal_append_exhaustion(configuration)
        }
        CommandLine::StageJournalBeforeCrash(configuration) => stage_journal_before(configuration),
        CommandLine::RecoverJournalBeforeCrash(configuration) => {
            recover_journal_before(configuration)
        }
        CommandLine::StageJournalCorruption(configuration) => {
            journal_corruption::stage(configuration)
        }
        CommandLine::RecoverJournalCorruption(configuration) => {
            journal_corruption::recover(configuration)
        }
        CommandLine::StageEvidenceCorruption(configuration) => {
            evidence_corruption::stage(configuration)
        }
        CommandLine::RecoverEvidenceCorruption(configuration) => {
            evidence_corruption::recover(configuration)
        }
        CommandLine::StagePromotionEvidenceCorruption(configuration) => {
            promotion_evidence_corruption::stage(configuration)
        }
        CommandLine::RecoverPromotionEvidenceCorruption(configuration) => {
            promotion_evidence_corruption::recover(configuration)
        }
        CommandLine::StageSnapshotBeforeCrash(configuration) => {
            snapshot::stage_before(configuration)
        }
        CommandLine::RecoverSnapshotBeforeCrash(configuration) => {
            snapshot::recover(configuration, false)
        }
        CommandLine::StageSnapshotAfterCrash(configuration) => snapshot::stage_after(configuration),
        CommandLine::RecoverSnapshotAfterCrash(configuration) => {
            snapshot::recover(configuration, true)
        }
        CommandLine::StageSnapshotCorruption(configuration) => {
            snapshot_corruption::stage(configuration)
        }
        CommandLine::RecoverSnapshotCorruption(configuration) => {
            snapshot_corruption::recover(configuration)
        }
        CommandLine::StageSnapshotQuotaExhaustion(configuration) => {
            disk::stage_snapshot(configuration)
        }
        CommandLine::RecoverSnapshotQuotaExhaustion(configuration) => {
            disk::recover_snapshot(configuration)
        }
        CommandLine::StageLeaseBeforeCrash(configuration) => lease::stage_before(configuration),
        CommandLine::RecoverLeaseBeforeCrash(configuration) => lease::recover(configuration, false),
        CommandLine::StageLeaseAfterCrash(configuration) => lease::stage_after(configuration),
        CommandLine::RecoverLeaseAfterCrash(configuration) => lease::recover(configuration, true),
        CommandLine::StagePatchBeforeCrash(configuration) => patch::stage_before(configuration),
        CommandLine::RecoverPatchBeforeCrash(configuration) => patch::recover(configuration, false),
        CommandLine::StagePatchAfterCrash(configuration) => patch::stage_after(configuration),
        CommandLine::RecoverPatchAfterCrash(configuration) => patch::recover(configuration, true),
        CommandLine::StageGateBeforeCrash(configuration) => gate::stage_before(configuration),
        CommandLine::RecoverGateBeforeCrash(configuration) => gate::recover(configuration, false),
        CommandLine::StageGateAfterCrash(configuration) => gate::stage_after(configuration),
        CommandLine::RecoverGateAfterCrash(configuration) => gate::recover(configuration, true),
        CommandLine::StagePromotionBeforeCrash(configuration) => {
            promotion::stage_before(configuration)
        }
        CommandLine::RecoverPromotionBeforeCrash(configuration) => {
            promotion::recover(configuration, false)
        }
        CommandLine::StagePromotionAfterCrash(configuration) => {
            promotion::stage_after(configuration)
        }
        CommandLine::RecoverPromotionAfterCrash(configuration) => {
            promotion::recover(configuration, true)
        }
        CommandLine::StageProjectionCorruption(configuration) => projection::stage(configuration),
        CommandLine::RecoverProjectionCorruption(configuration) => {
            projection::recover(configuration)
        }
        #[cfg(not(verus_only))]
        CommandLine::StageDependencyFault { dependency, fault, retry_limit, configuration } => {
            dependency::stage(configuration, dependency, fault, retry_limit)
        }
        #[cfg(not(verus_only))]
        CommandLine::RecoverDependencyFault { dependency, fault, retry_limit, configuration } => {
            dependency::recover(configuration, dependency, fault, retry_limit)
        }
        #[cfg(not(verus_only))]
        CommandLine::DependencyChild(dependency) => dependency::child(dependency),
        #[cfg(not(verus_only))]
        CommandLine::StageDaemonLifecycle { phase, configuration } => {
            daemon_lifecycle::stage(configuration, phase)
        }
        #[cfg(not(verus_only))]
        CommandLine::RecoverDaemonLifecycle { phase, configuration } => {
            daemon_lifecycle::recover(configuration, phase)
        }
        CommandLine::StageOutboxCrash(configuration) => stage_outbox(configuration),
        CommandLine::RecoverOutboxCrash(configuration) => recover_outbox(configuration),
        CommandLine::StageHostReboot { phase, reconciliation, configuration } => {
            reboot::stage(configuration, phase, reconciliation)
        }
        CommandLine::RecoverHostReboot { phase, configuration } => {
            reboot::recover(configuration, phase)
        }
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
