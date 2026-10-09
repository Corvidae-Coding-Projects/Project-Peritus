//! Fixed CLI handlers for H1 atomic promotion crash qualification.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::process::ExitCode;

use peritus_types::Sha256Digest;

use crate::DaemonConfig;
use crate::outbox::{
    recover_promotion_after_crash, recover_promotion_before_crash, stage_promotion_after_crash,
    stage_promotion_before_crash,
};

use super::{
    QUALIFICATION_KILL_BOUND, output_failure, qualification_failure, write_error, write_output,
};

pub(super) fn stage_before(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_promotion_before_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification promotion-before-stage proposal_sha256={} authorization_sha256={} campaign_before_sha256={} pointer_before_sha256={} campaign_after_sha256={} pointer_after_sha256={}",
        checkpoint.proposal_sha256(),
        checkpoint.authorization_sha256(),
        checkpoint.campaign_before_sha256(),
        checkpoint.pointer_before_sha256(),
        checkpoint.campaign_after_sha256(),
        checkpoint.pointer_after_sha256(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("promotion-before qualifier was not killed at its accepted activation checkpoint");
    ExitCode::FAILURE
}

pub(super) fn stage_after(configuration: OsString) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let checkpoint = match stage_promotion_after_crash(&config) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return qualification_failure(&error),
    };
    let line = format!(
        "peritus-qualification promotion-after-stage proposal_sha256={} authorization_sha256={} campaign_before_sha256={} pointer_before_sha256={} campaign_after_sha256={} pointer_after_sha256={} approval_revision={} first_position={} last_position={} committed=true",
        checkpoint.proposal_sha256(),
        checkpoint.authorization_sha256(),
        checkpoint.campaign_before_sha256(),
        checkpoint.pointer_before_sha256(),
        checkpoint.campaign_after_sha256(),
        checkpoint.pointer_after_sha256(),
        checkpoint.approval_revision(),
        checkpoint.first_position(),
        checkpoint.last_position(),
    );
    if let Err(error) = write_output(&line) {
        return output_failure(error);
    }
    std::thread::park_timeout(QUALIFICATION_KILL_BOUND);
    write_error("promotion-after qualifier was not killed at its committed checkpoint");
    ExitCode::FAILURE
}

pub(super) fn recover(configuration: OsString, after_commit: bool) -> ExitCode {
    let config = match DaemonConfig::load(configuration) {
        Ok(config) => config,
        Err(error) => return qualification_failure(&error),
    };
    let observation = if after_commit {
        recover_promotion_after_crash(&config)
    } else {
        recover_promotion_before_crash(&config)
    };
    match observation {
        Ok(observation) => {
            let timing = if after_commit { "after" } else { "before" };
            let authorization =
                observation.authorization_digest().map_or_else(|| "none".to_owned(), digest_hex);
            let line = format!(
                "peritus-qualification promotion-{timing}-recover proposal_sha256={} authorization_sha256={} campaign_sha256={} pointer_sha256={} approval_revision={} approval_position={} committed_events={} aggregate_heads={} committed={}",
                digest_hex(observation.proposal_digest()),
                authorization,
                digest_hex(observation.campaign_digest()),
                digest_hex(observation.pointer_digest()),
                observation.approval_revision(),
                observation.approval_position(),
                observation.event_count(),
                observation.aggregate_heads(),
                observation.committed(),
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
