//! Artifact quota qualification command rendering.

use std::ffi::OsString;
use std::process::ExitCode;

use crate::DaemonConfig;

pub(super) fn stage_blob_finalize_exhaustion(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return super::qualification_failure(&error),
    };
    match crate::qualification::disk::stage_blob_finalize_exhaustion(&config) {
        Ok(checkpoint) => {
            let line = format!(
                "peritus-qualification disk-blob-finalize-stage filler_sha256={} rejected_sha256={} quota_bytes={} temporary_files={} object_files={}",
                checkpoint.filler_digest(),
                checkpoint.rejected_digest(),
                checkpoint.quota_bytes(),
                checkpoint.temporary_files(),
                checkpoint.object_files(),
            );
            super::write_output(&line).map_or_else(super::output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => super::qualification_failure(&error),
    }
}

pub(super) fn recover_blob_finalize_exhaustion(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return super::qualification_failure(&error),
    };
    match crate::qualification::disk::recover_blob_finalize_exhaustion(&config) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification disk-blob-finalize-recover filler_sha256={} rejected_sha256={} quota_bytes={} used_bytes={} journal_verified={} temporary_files={} object_files={}",
                observation.filler_digest(),
                observation.rejected_digest(),
                observation.quota_bytes(),
                observation.used_bytes(),
                observation.journal_verified(),
                observation.temporary_files(),
                observation.object_files(),
            );
            super::write_output(&line).map_or_else(super::output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => super::qualification_failure(&error),
    }
}

pub(super) fn stage_journal_append_exhaustion(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return super::qualification_failure(&error),
    };
    match crate::qualification::disk_journal::stage_journal_append_exhaustion(&config) {
        Ok(checkpoint) => {
            let line = format!(
                "peritus-qualification disk-journal-append-stage request_sha256={} page_count={} page_size={} maximum_bytes={} storage_exhausted=true append_absent=true",
                checkpoint.request_sha256(),
                checkpoint.page_count(),
                checkpoint.page_size(),
                checkpoint.maximum_bytes(),
            );
            super::write_output(&line).map_or_else(super::output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => super::qualification_failure(&error),
    }
}

pub(super) fn recover_journal_append_exhaustion(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return super::qualification_failure(&error),
    };
    match crate::qualification::disk_journal::recover_journal_append_exhaustion(&config) {
        Ok(observation) => {
            let line = format!(
                "peritus-qualification disk-journal-append-recover request_sha256={} page_count={} page_size={} maximum_bytes={} committed_events={} aggregate_heads={} journal_verified={} append_absent=true",
                observation.request_sha256(),
                observation.page_count(),
                observation.page_size(),
                observation.maximum_bytes(),
                observation.committed_events(),
                observation.aggregate_heads(),
                observation.journal_verified(),
            );
            super::write_output(&line).map_or_else(super::output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => super::qualification_failure(&error),
    }
}
