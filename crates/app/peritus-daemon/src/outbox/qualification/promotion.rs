//! Real F0 atomic promotion recovery on both sides of its C0 transaction.

use peritus_artifact_store::{ArtifactStore, ArtifactStoreError, StoreConfig};
use peritus_evolution::qualification::{
    CommittedPromotion, PreparedPromotion, PromotionQualificationObservation, observe_promotion,
    prepare_promotion,
};

use crate::instance::InstanceGuard;
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{acquire_instance, digest_hex, open_journal};

/// Accepted activation retained only in the killed daemon process.
pub struct PromotionBeforeCheckpoint {
    prepared: PreparedPromotion,
    _instance: InstanceGuard,
}

impl PromotionBeforeCheckpoint {
    pub(crate) fn proposal_sha256(&self) -> String {
        digest_hex(self.prepared.proposal_digest())
    }
    pub(crate) fn authorization_sha256(&self) -> String {
        digest_hex(self.prepared.authorization_digest())
    }
    pub(crate) fn campaign_before_sha256(&self) -> String {
        digest_hex(self.prepared.campaign_before())
    }
    pub(crate) fn pointer_before_sha256(&self) -> String {
        digest_hex(self.prepared.pointer_before())
    }
    pub(crate) fn campaign_after_sha256(&self) -> String {
        digest_hex(self.prepared.campaign_after())
    }
    pub(crate) fn pointer_after_sha256(&self) -> String {
        digest_hex(self.prepared.pointer_after())
    }
}

/// Exact receipt retained after the atomic transaction and before caller acknowledgement.
pub struct PromotionAfterCheckpoint {
    committed: CommittedPromotion,
    _instance: InstanceGuard,
}

impl PromotionAfterCheckpoint {
    pub(crate) fn proposal_sha256(&self) -> String {
        digest_hex(self.committed.proposal_digest())
    }
    pub(crate) fn authorization_sha256(&self) -> String {
        digest_hex(self.committed.authorization_digest())
    }
    pub(crate) fn campaign_before_sha256(&self) -> String {
        digest_hex(self.committed.campaign_before())
    }
    pub(crate) fn pointer_before_sha256(&self) -> String {
        digest_hex(self.committed.pointer_before())
    }
    pub(crate) fn campaign_after_sha256(&self) -> String {
        digest_hex(self.committed.campaign_after())
    }
    pub(crate) fn pointer_after_sha256(&self) -> String {
        digest_hex(self.committed.pointer_after())
    }
    pub(crate) const fn approval_revision(&self) -> u64 {
        self.committed.committed().state_revision()
    }
    pub(crate) const fn first_position(&self) -> u64 {
        self.committed.committed().batch().first_position()
    }
    pub(crate) const fn last_position(&self) -> u64 {
        self.committed.committed().batch().last_position()
    }
}

/// Seeds every prerequisite but leaves the accepted activation out of C0.
pub fn stage_promotion_before_crash(
    config: &DaemonConfig,
) -> Result<PromotionBeforeCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let artifacts = open_artifacts(config)?;
    let prepared = prepare_promotion(&mut journal, &artifacts).map_err(evolution_error)?;
    Ok(PromotionBeforeCheckpoint { prepared, _instance: instance })
}

/// Commits the complete production activation before the process is killed.
pub fn stage_promotion_after_crash(
    config: &DaemonConfig,
) -> Result<PromotionAfterCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let artifacts = open_artifacts(config)?;
    let prepared = prepare_promotion(&mut journal, &artifacts).map_err(evolution_error)?;
    let committed = prepared.commit(&mut journal).map_err(evolution_error)?;
    if committed.committed().batch().records().len() != 2
        || committed.committed().state_revision() != 2
    {
        return Err(promotion_error("atomic promotion receipt is incomplete"));
    }
    Ok(PromotionAfterCheckpoint { committed, _instance: instance })
}

/// Replays the prepared-only side of the crash boundary through a fresh connection.
pub fn recover_promotion_before_crash(
    config: &DaemonConfig,
) -> Result<PromotionQualificationObservation, DaemonError> {
    recover(config, false)
}

/// Replays the fully committed side of the crash boundary through a fresh connection.
pub fn recover_promotion_after_crash(
    config: &DaemonConfig,
) -> Result<PromotionQualificationObservation, DaemonError> {
    recover(config, true)
}

fn recover(
    config: &DaemonConfig,
    committed: bool,
) -> Result<PromotionQualificationObservation, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    observe_promotion(&mut journal, committed).map_err(evolution_error)
}

fn open_artifacts(config: &DaemonConfig) -> Result<ArtifactStore, DaemonError> {
    let store_config = StoreConfig::new(
        config.paths().artifact_root(),
        config.limits().maximum_artifact_bytes(),
        config.limits().artifact_quota_bytes(),
    )
    .and_then(|value| value.with_database_path(config.paths().database()))
    .map_err(artifact_error)?;
    ArtifactStore::open(store_config).map_err(artifact_error)
}

fn artifact_error(error: ArtifactStoreError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "open promotion qualification artifact store",
        error.to_string(),
        error,
    )
}

fn evolution_error(error: peritus_evolution::EvolutionError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify atomic promotion recovery",
        error.to_string(),
        error,
    )
}

fn promotion_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify atomic promotion recovery",
        detail,
    )
}
