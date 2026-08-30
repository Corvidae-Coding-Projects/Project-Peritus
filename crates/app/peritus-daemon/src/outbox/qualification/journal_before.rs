//! Real journal recovery when a staged append plan dies before durable commit.

use std::fs;

use peritus_journal::AppendPlan;

use crate::{DaemonConfig, DaemonError};

use super::{
    QualificationIdentity, acquire_instance, build_plan, digest_hex, journal_error, open_journal,
    qualification_error,
};

const EFFECT_DIRECTORY: &str = "outbox-crash-qualification-v1";

/// Checkpoint emitted after a production append plan exists but before it is submitted.
pub struct JournalBeforeCheckpoint {
    request_sha256: String,
    _unsubmitted: AppendPlan,
}

impl JournalBeforeCheckpoint {
    /// Returns the digest of the exact append request held only by the killed process.
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }
}

/// Direct facts from reopening the journal after the pre-commit process died.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalBeforeQualification {
    request_sha256: String,
    committed_events: u64,
    aggregate_heads: u64,
    external_effects: u64,
    pending_claims: u64,
}

impl JournalBeforeQualification {
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }

    pub(crate) const fn journal_verified(&self) -> bool {
        true
    }

    pub(crate) const fn committed_events(&self) -> u64 {
        self.committed_events
    }

    pub(crate) const fn aggregate_heads(&self) -> u64 {
        self.aggregate_heads
    }

    pub(crate) const fn external_effects(&self) -> u64 {
        self.external_effects
    }

    pub(crate) const fn pending_claims(&self) -> u64 {
        self.pending_claims
    }
}

/// Builds the exact production append plan and returns without submitting it.
///
/// The CLI publishes the returned checkpoint and waits to be killed. The plan has no durable
/// representation: it exists only in this process before the journal commit boundary.
pub fn stage_journal_before_crash(
    config: &DaemonConfig,
) -> Result<JournalBeforeCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let identity = QualificationIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let baseline = journal.integrity_scan().map_err(journal_error)?;
    if baseline.event_count() != 0
        || baseline.aggregate_count() != 0
        || journal.head(identity.aggregate).map_err(journal_error)?.is_some()
    {
        return Err(qualification_error("pre-commit qualification journal is not empty"));
    }
    let unsubmitted = build_plan(&journal, &identity)?;
    Ok(JournalBeforeCheckpoint {
        request_sha256: digest_hex(identity.request_digest),
        _unsubmitted: unsubmitted,
    })
}

/// Reopens the exact production journal and proves the killed plan created no durable mutation.
pub fn recover_journal_before_crash(
    config: &DaemonConfig,
) -> Result<JournalBeforeQualification, DaemonError> {
    let store_id = config.store_identity()?;
    let identity = QualificationIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    let pending_claims = u64::from(journal.claim_outbox(1, 2).map_err(journal_error)?.is_some());
    let effect_root = config.paths().state_root().join(EFFECT_DIRECTORY);
    let external_effects = match fs::symlink_metadata(&effect_root) {
        Ok(_) => fs::read_dir(&effect_root)
            .map_err(|_| qualification_error("inspect pre-commit effect directory"))?
            .count() as u64,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(_) => return Err(qualification_error("inspect pre-commit effect directory")),
    };
    if report.event_count() != 0
        || report.aggregate_count() != 0
        || report.last_position() != 0
        || journal.head(identity.aggregate).map_err(journal_error)?.is_some()
        || pending_claims != 0
        || external_effects != 0
    {
        return Err(qualification_error(
            "pre-commit append plan left durable journal or effect state",
        ));
    }
    Ok(JournalBeforeQualification {
        request_sha256: digest_hex(identity.request_digest),
        committed_events: report.event_count(),
        aggregate_heads: report.aggregate_count(),
        external_effects,
        pending_claims,
    })
}
