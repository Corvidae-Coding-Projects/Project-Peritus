//! Closed process-facing dependency failure qualification commands.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;
use crate::qualification::dependency::{
    DependencyFault, DependencyKind, recover_dependency_fault, stage_dependency_fault,
};

use super::{output_failure, qualification_failure, write_output};

pub(super) fn stage(
    configuration: OsString,
    dependency: DependencyKind,
    fault: DependencyFault,
    retry_limit: u16,
) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match stage_dependency_fault(&config, dependency, fault, retry_limit) {
        Ok(observation) => {
            let child_exit = observation
                .child_exit_code()
                .map_or_else(|| "none".to_owned(), |code| code.to_string());
            let line = format!(
                "peritus-qualification dependency-stage dependency={} fault={} state_sha256={} effect_sha256={} attempts={} committed_events={} receipt_bytes={} child_exit={child_exit}",
                dependency.code(),
                fault.code(),
                observation.state_sha256(),
                observation.effect_sha256(),
                observation.attempts(),
                observation.committed_events(),
                observation.receipt_bytes(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

pub(super) fn recover(
    configuration: OsString,
    dependency: DependencyKind,
    fault: DependencyFault,
    retry_limit: u16,
) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match recover_dependency_fault(&config, dependency, fault, retry_limit) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification dependency-recover dependency={} fault={} state_sha256={} attempts={} committed_events={} aggregate_heads={} retry_pending={} exhausted={} ownership_reconciled={} journal_verified={}",
                dependency.code(),
                fault.code(),
                observation.state_sha256(),
                observation.attempts(),
                observation.committed_events(),
                observation.aggregate_heads(),
                observation.retry_pending(),
                observation.exhausted(),
                observation.ownership_reconciled(),
                observation.journal_verified(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

pub(super) fn child(dependency: DependencyKind) -> ExitCode {
    let line = format!(
        "peritus-qualification dependency-child dependency={} reached=true",
        dependency.code(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    ExitCode::from(17)
}
