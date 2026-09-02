//! CLI handlers for retained Git snapshot corruption containment qualification.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;
use crate::outbox::{recover_snapshot_corruption, stage_snapshot_corruption};

use super::{output_failure, qualification_failure, write_output};

pub(super) fn stage(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match stage_snapshot_corruption(&config) {
        Ok(checkpoint) => {
            let line = format!(
                "peritus-qualification snapshot-corruption-stage expected_commit={} divergent_commit={} reference={} manifest_sha256={} corruption_detected=true",
                checkpoint.expected_commit(),
                checkpoint.divergent_commit(),
                checkpoint.reference(),
                checkpoint.manifest_sha256(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

pub(super) fn recover(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match recover_snapshot_corruption(&config) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification snapshot-corruption-recover reference={} quarantine_reference={} quarantined_commit={} journal_verified={} corruption_detected={} mutation_admitted={}",
                observation.reference(),
                observation.quarantine_reference(),
                observation.quarantined_commit(),
                observation.journal_verified(),
                observation.corruption_detected(),
                observation.mutation_admitted(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}
