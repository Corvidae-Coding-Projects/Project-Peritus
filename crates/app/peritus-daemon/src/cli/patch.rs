//! Fixed CLI handlers for H1 patch commit crash qualification.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;
use crate::outbox::{
    recover_patch_after_crash, recover_patch_before_crash, stage_patch_after_crash,
    stage_patch_before_crash,
};

use super::{
    QUALIFICATION_KILL_BOUND, output_failure, qualification_failure, write_error, write_output,
};

pub(super) fn stage_before(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_patch_before_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification patch-before-stage patch_sha256={} target_sha256={}",
        checkpoint.patch_sha256(),
        checkpoint.target_sha256(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("patch-before qualifier was not killed at its checked-plan checkpoint");
    ExitCode::FAILURE
}

pub(super) fn stage_after(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_patch_after_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification patch-after-stage patch_sha256={} target_sha256={} manifest_sha256={} applied=true",
        checkpoint.patch_sha256(),
        checkpoint.target_sha256(),
        checkpoint.manifest_sha256(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("patch-after qualifier was not killed at its applied-patch checkpoint");
    ExitCode::FAILURE
}

pub(super) fn recover(configuration: OsString, after_commit: bool) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let observation = if after_commit {
        recover_patch_after_crash(&config)
    } else {
        recover_patch_before_crash(&config)
    };
    match observation {
        Ok(observation) => {
            let timing = if after_commit { "after" } else { "before" };
            let line = format!(
                "peritus-qualification patch-{timing}-recover patch_sha256={} target_sha256={} journal_verified={} target_files={} pending_transactions={}",
                observation.patch_sha256(),
                observation.target_sha256().unwrap_or("none"),
                observation.journal_verified(),
                observation.target_files(),
                observation.pending_transactions(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}
