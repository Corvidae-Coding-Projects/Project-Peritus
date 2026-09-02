//! Fixed CLI handlers for H1 gate commit crash qualification.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;
use crate::outbox::{
    recover_gate_after_crash, recover_gate_before_crash, stage_gate_after_crash,
    stage_gate_before_crash,
};

use super::{
    QUALIFICATION_KILL_BOUND, output_failure, qualification_failure, write_error, write_output,
};

pub(super) fn stage_before(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_gate_before_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification gate-before-stage request_sha256={} plan_sha256={} successor_sha256={}",
        checkpoint.request_sha256(),
        checkpoint.plan_sha256(),
        checkpoint.successor_sha256(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("gate-before qualifier was not killed at its accepted transition checkpoint");
    ExitCode::FAILURE
}

pub(super) fn stage_after(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_gate_after_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification gate-after-stage request_sha256={} plan_sha256={} successor_sha256={} checkpoint_sha256={} state_revision={} producing_position={} committed=true",
        checkpoint.request_sha256(),
        checkpoint.plan_sha256(),
        checkpoint.successor_sha256(),
        checkpoint.checkpoint_sha256(),
        checkpoint.state_revision(),
        checkpoint.producing_position(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("gate-after qualifier was not killed at its committed checkpoint");
    ExitCode::FAILURE
}

pub(super) fn recover(configuration: OsString, after_commit: bool) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let observation = if after_commit {
        recover_gate_after_crash(&config)
    } else {
        recover_gate_before_crash(&config)
    };
    match observation {
        Ok(observation) => {
            let timing = if after_commit { "after" } else { "before" };
            let state_revision = optional_number(observation.state_revision());
            let producing_position = optional_number(observation.producing_position());
            let line = format!(
                "peritus-qualification gate-{timing}-recover request_sha256={} plan_sha256={} journal_verified={} committed_events={} aggregate_heads={} state_revision={} successor_sha256={} checkpoint_sha256={} producing_position={}",
                observation.request_sha256(),
                observation.plan_sha256(),
                observation.journal_verified(),
                observation.committed_events(),
                observation.aggregate_heads(),
                state_revision,
                observation.successor_sha256().unwrap_or("none"),
                observation.checkpoint_sha256().unwrap_or("none"),
                producing_position,
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

fn optional_number(value: Option<u64>) -> String {
    value.map_or_else(|| "none".to_owned(), |number| number.to_string())
}
