//! Acceptance-evidence corruption qualification command rendering.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;

pub(super) fn stage(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return super::qualification_failure(&error),
    };
    match crate::qualification::evidence_corruption::stage_corruption(&config) {
        Ok(checkpoint) => {
            let line = format!(
                "peritus-qualification evidence-corruption-stage evidence_id={} record_sha256={} original_bytes_sha256={} corrupt_bytes_sha256={} bytes={} corruption_detected={}",
                hex(checkpoint.evidence_id().as_bytes()),
                hex(checkpoint.record_digest().as_bytes()),
                hex(checkpoint.original_bytes_sha256().as_bytes()),
                hex(checkpoint.corrupt_bytes_sha256().as_bytes()),
                checkpoint.record_bytes(),
                checkpoint.corruption_detected(),
            );
            super::write_output(&line).map_or_else(super::output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => super::qualification_failure(&error),
    }
}

pub(super) fn recover(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return super::qualification_failure(&error),
    };
    match crate::qualification::evidence_corruption::recover_corruption(&config) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification evidence-corruption-recover evidence_id={} corrupt_bytes_sha256={} quarantine_sha256={} bytes={} committed_events={} aggregate_heads={} journal_verified={} corruption_detected={} mutation_admitted={}",
                hex(observation.evidence_id().as_bytes()),
                hex(observation.corrupt_bytes_sha256().as_bytes()),
                hex(observation.quarantine_digest().as_bytes()),
                observation.record_bytes(),
                observation.committed_events(),
                observation.aggregate_heads(),
                observation.journal_verified(),
                observation.corruption_detected(),
                observation.mutation_admitted(),
            );
            super::write_output(&line).map_or_else(super::output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => super::qualification_failure(&error),
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}
