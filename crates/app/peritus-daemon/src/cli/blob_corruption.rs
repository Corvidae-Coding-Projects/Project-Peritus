//! CLI handlers for artifact-corruption containment qualification.

use std::ffi::OsString;
use std::process::ExitCode;

use peritus_artifact_store::ArtifactDigest;
use peritus_types::Sha256Digest;

use crate::DaemonConfig;
use crate::qualification::blob_corruption::{recover_corruption, stage_corruption};

use super::{output_failure, qualification_failure, write_output};

pub(super) fn stage(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    match stage_corruption(&config) {
        Ok(checkpoint) => {
            let line = format!(
                "peritus-qualification blob-corruption-stage digest={} original_sha256={} corrupt_sha256={} bytes={} corruption_detected=true",
                checkpoint.digest().to_hex(),
                digest_hex(checkpoint.original_sha256()),
                digest_hex(checkpoint.corrupt_sha256()),
                checkpoint.bytes(),
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
                "peritus-qualification blob-corruption-recover digest={} quarantined_sha256={} bytes={} journal_verified={} reference_retained={} corruption_detected={} mutation_admitted={}",
                observation.digest().to_hex(),
                digest_hex(observation.quarantined_sha256()),
                observation.bytes(),
                observation.journal_verified(),
                observation.reference_retained(),
                observation.corruption_detected(),
                observation.mutation_admitted(),
            );
            write_output(&line).map_or_else(output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => qualification_failure(&error),
    }
}

fn digest_hex(digest: Sha256Digest) -> String {
    ArtifactDigest::from_sha256(digest).to_hex()
}
