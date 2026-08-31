//! CLI handlers for authoritative-journal corruption and fail-closed startup qualification.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::process::ExitCode;

use peritus_types::Sha256Digest;

use crate::DaemonConfig;
use crate::qualification::journal_corruption::{recover_corruption, stage_corruption};

use super::{output_failure, qualification_failure, write_output};

pub(super) fn stage(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match stage_corruption(&config) {
        Ok(checkpoint) => {
            let line = format!(
                "peritus-qualification journal-corruption-stage request_sha256={} original_frame_sha256={} corrupt_frame_sha256={} event_count={} corruption_detected={}",
                checkpoint.request_sha256(),
                digest_hex(checkpoint.original_frame_sha256()),
                digest_hex(checkpoint.corrupt_frame_sha256()),
                checkpoint.event_count(),
                checkpoint.corruption_detected(),
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
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => return qualification_failure(&error),
    };
    match runtime.block_on(recover_corruption(config)) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification journal-corruption-recover startup_error_code={} corrupt_frame_sha256={} event_count={} aggregate_heads={} state_records={} authority_epochs={} application_principals={} corruption_detected={} mutation_admitted={}",
                observation.startup_error_code(),
                digest_hex(observation.corrupt_frame_sha256()),
                observation.event_count(),
                observation.aggregate_heads(),
                observation.state_records(),
                observation.authority_epochs(),
                observation.application_principals(),
                observation.corruption_detected(),
                observation.mutation_admitted(),
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
