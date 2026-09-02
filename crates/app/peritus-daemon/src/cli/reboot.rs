//! Disposable-host reboot qualification command rendering.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;
use crate::outbox::{
    HostRebootPhase, recover_host_reboot, stage_host_reboot, stage_startup_reconciliation,
};

pub(super) fn stage(
    configuration: OsString,
    phase: HostRebootPhase,
    reconciliation: bool,
) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return super::qualification_failure(&error),
    };
    let checkpoint = if reconciliation {
        stage_startup_reconciliation(&config)
    } else {
        stage_host_reboot(&config, phase)
    };
    let checkpoint = match checkpoint {
        Ok(checkpoint) => checkpoint,
        Err(error) => return super::qualification_failure(&error),
    };
    let prefix = if reconciliation {
        "peritus-qualification reboot-reconciliation-stage"
    } else {
        "peritus-qualification reboot-stage"
    };
    let line = format!(
        "{prefix} phase={} effect_path={} claim_fence={} external_effects={}",
        checkpoint.phase().code(),
        checkpoint.effect_path().display(),
        checkpoint.claim_fence(),
        checkpoint.external_effects(),
    );
    if let Err(error) = super::write_output(&line) {
        return super::output_failure(error);
    }
    std::thread::park_timeout(super::QUALIFICATION_KILL_BOUND);
    super::write_error("host reboot qualifier was not interrupted at its checkpoint");
    ExitCode::FAILURE
}

pub(super) fn recover(configuration: OsString, phase: HostRebootPhase) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return super::qualification_failure(&error),
    };
    match recover_host_reboot(&config, phase) {
        Ok(value) => {
            let line = format!(
                "peritus-qualification reboot-recover phase={} destination_reconciled={} external_effects={} duplicate_effects={} exact_fence_acknowledged={} pending_claims={}",
                value.phase().code(),
                value.destination_reconciled(),
                value.external_effects(),
                value.duplicate_effects(),
                value.exact_fence_acknowledged(),
                value.pending_claims(),
            );
            super::write_output(&line).map_or_else(super::output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => super::qualification_failure(&error),
    }
}
