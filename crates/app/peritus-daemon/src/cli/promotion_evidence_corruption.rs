//! Harness-promotion evidence corruption qualification command rendering.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;

pub(super) fn stage(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return super::qualification_failure(&error),
    };
    match crate::qualification::promotion_evidence_corruption::stage_corruption(&config) {
        Ok(value) => {
            let line = format!(
                "peritus-qualification promotion-evidence-corruption-stage evidence_id={} record_sha256={} corrupt_bytes_sha256={} pointer_sha256={} bytes={} corruption_detected={}",
                hex(value.evidence_id().as_bytes()),
                hex(value.record_digest().as_bytes()),
                hex(value.corrupt_bytes_sha256().as_bytes()),
                hex(value.pointer_digest().as_bytes()),
                value.record_bytes(),
                value.corruption_detected(),
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
    match crate::qualification::promotion_evidence_corruption::recover_corruption(&config) {
        Ok(value) => {
            let line = format!(
                "peritus-qualification promotion-evidence-corruption-recover evidence_id={} corrupt_bytes_sha256={} quarantine_sha256={} pointer_sha256={} bytes={} committed_events={} aggregate_heads={} journal_verified={} promotion_verified={} corruption_detected={} mutation_admitted={}",
                hex(value.evidence_id().as_bytes()),
                hex(value.corrupt_bytes_sha256().as_bytes()),
                hex(value.quarantine_digest().as_bytes()),
                hex(value.pointer_digest().as_bytes()),
                value.record_bytes(),
                value.committed_events(),
                value.aggregate_heads(),
                value.journal_verified(),
                value.promotion_verified(),
                value.corruption_detected(),
                value.mutation_admitted(),
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
