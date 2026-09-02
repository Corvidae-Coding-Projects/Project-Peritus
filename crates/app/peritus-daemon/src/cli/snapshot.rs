//! Fixed CLI handlers for H1 retained Git snapshot crash qualification.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;
use crate::outbox::{
    recover_snapshot_after_crash, recover_snapshot_before_crash, stage_snapshot_after_crash,
    stage_snapshot_before_crash,
};

use super::{
    QUALIFICATION_KILL_BOUND, output_failure, qualification_failure, write_error, write_output,
};

pub(super) fn stage_before(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_snapshot_before_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification snapshot-before-stage tree={} reference={}",
        checkpoint.tree(),
        checkpoint.reference(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("snapshot-before qualifier was not killed at its prepared candidate checkpoint");
    ExitCode::FAILURE
}

pub(super) fn stage_after(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_snapshot_after_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification snapshot-after-stage commit={} tree={} reference={} manifest_sha256={} retained=true",
        checkpoint.commit(),
        checkpoint.tree(),
        checkpoint.reference(),
        checkpoint.manifest_sha256(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("snapshot-after qualifier was not killed at its retained-reference checkpoint");
    ExitCode::FAILURE
}

pub(super) fn recover(configuration: OsString, after_commit: bool) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let observation = if after_commit {
        recover_snapshot_after_crash(&config)
    } else {
        recover_snapshot_before_crash(&config)
    };
    match observation {
        Ok(observation) => {
            let timing = if after_commit { "after" } else { "before" };
            let line = format!(
                "peritus-qualification snapshot-{timing}-recover commit={} tree={} reference={} manifest_sha256={} journal_verified={} retained={} snapshot_refs={}",
                observation.commit().unwrap_or("none"),
                observation.tree(),
                observation.reference(),
                observation.manifest_sha256().unwrap_or("none"),
                observation.journal_verified(),
                observation.retained(),
                observation.snapshot_refs(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}
