//! CLI handlers for controlled projection corruption and startup repair qualification.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::process::ExitCode;

use peritus_types::Sha256Digest;

use crate::DaemonConfig;
use crate::qualification::projection::{recover_corruption, stage_corruption};

use super::{output_failure, qualification_failure, write_output};

pub(super) fn stage(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match stage_corruption(&config) {
        Ok(checkpoint) => {
            let line = format!(
                "peritus-qualification projection-corruption-stage projection={} generation={} original_payload_sha256={} corrupt_payload_sha256={} payload_bytes={} corrupted=true",
                checkpoint.projection_name(),
                checkpoint.generation(),
                digest_hex(checkpoint.original_payload_sha256()),
                digest_hex(checkpoint.corrupt_payload_sha256()),
                checkpoint.payload_bytes(),
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
    match recover_corruption(&config) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification projection-corruption-recover projection={} previous_generation={} repaired_generation={} corrupt_payload_sha256={} repaired_payload_sha256={} payload_bytes={} generation_count={} event_count={} aggregate_heads={} payload_valid={} reusable={}",
                observation.projection_name(),
                observation.previous_generation(),
                observation.repaired_generation(),
                digest_hex(observation.corrupt_payload_sha256()),
                digest_hex(observation.repaired_payload_sha256()),
                observation.payload_bytes(),
                observation.generation_count(),
                observation.event_count(),
                observation.aggregate_heads(),
                observation.payload_valid(),
                observation.reusable(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

fn digest_hex(digest: Sha256Digest) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.as_bytes() {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
