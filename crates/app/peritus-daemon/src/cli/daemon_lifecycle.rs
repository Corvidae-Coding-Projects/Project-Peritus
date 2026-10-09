//! Closed process-facing daemon lifecycle kill and replay commands.

use std::ffi::OsString;
use std::process::ExitCode;

use peritus_orchestrator::qualification::LifecyclePhase;

use crate::DaemonConfig;
use crate::qualification::daemon_lifecycle::{recover_daemon_lifecycle, stage_daemon_lifecycle};

use super::{
    QUALIFICATION_KILL_BOUND, output_failure, qualification_failure, write_error, write_output,
};

pub(super) fn stage(configuration: OsString, phase: LifecyclePhase) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_daemon_lifecycle(&config, phase) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification daemon-lifecycle-stage phase={} run_id={} state_sha256={} committed_events={} active_children={}",
        checkpoint.phase().code(),
        hex(checkpoint.run_id().as_bytes()),
        checkpoint.state_sha256(),
        checkpoint.committed_events(),
        checkpoint.active_children(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("daemon lifecycle qualifier was not killed at its durable phase checkpoint");
    ExitCode::FAILURE
}

pub(super) fn recover(configuration: OsString, phase: LifecyclePhase) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match recover_daemon_lifecycle(&config, phase) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification daemon-lifecycle-recover phase={} run_id={} state_sha256={} committed_events={} aggregate_heads={} active_children={} pending_directive={} open_handoff={} proposed_candidate={} acceptance_certificate={} replay_exact=true journal_verified=true ownership_reconciled=true",
                observation.phase().code(),
                hex(observation.run_id().as_bytes()),
                observation.state_sha256(),
                observation.committed_events(),
                observation.aggregate_heads(),
                observation.active_children(),
                observation.pending_directive(),
                observation.open_handoff(),
                observation.proposed_candidate(),
                observation.acceptance_certificate(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

fn hex(bytes: &[u8; 16]) -> String {
    let mut output = String::with_capacity(32);
    for byte in bytes {
        use core::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
