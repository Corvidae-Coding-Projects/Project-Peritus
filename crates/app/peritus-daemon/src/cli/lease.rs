//! Fixed CLI handlers for H1 lease commit crash qualification.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;
use crate::outbox::{
    recover_lease_after_crash, recover_lease_before_crash, stage_lease_after_crash,
    stage_lease_before_crash,
};

use super::{
    QUALIFICATION_KILL_BOUND, output_failure, qualification_failure, write_error, write_output,
};

pub(super) fn stage_before(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_lease_before_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification lease-before-stage request_sha256={}",
        checkpoint.request_sha256(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("lease-before qualifier was not killed at its unsubmitted commit checkpoint");
    ExitCode::FAILURE
}

pub(super) fn stage_after(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_lease_after_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification lease-after-stage request_sha256={} state_revision={} state_sha256={} producing_position={} committed=true",
        checkpoint.request_sha256(),
        checkpoint.state_revision(),
        checkpoint.state_sha256(),
        checkpoint.producing_position(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("lease-after qualifier was not killed at its committed projection checkpoint");
    ExitCode::FAILURE
}

pub(super) fn recover(configuration: OsString, after_commit: bool) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let observation = if after_commit {
        recover_lease_after_crash(&config)
    } else {
        recover_lease_before_crash(&config)
    };
    match observation {
        Ok(observation) => {
            let timing = if after_commit { "after" } else { "before" };
            let state_revision = observation
                .state_revision()
                .map_or_else(|| "none".to_owned(), |value| value.to_string());
            let producing_position = observation
                .producing_position()
                .map_or_else(|| "none".to_owned(), |value| value.to_string());
            let line = format!(
                "peritus-qualification lease-{timing}-recover request_sha256={} journal_verified={} committed_events={} aggregate_heads={} state_revision={} state_sha256={} producing_position={}",
                observation.request_sha256(),
                observation.journal_verified(),
                observation.committed_events(),
                observation.aggregate_heads(),
                state_revision,
                observation.state_sha256().unwrap_or("none"),
                producing_position,
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}
