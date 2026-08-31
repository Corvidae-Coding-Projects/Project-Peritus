//! Controlled corruption of genuinely published F0 harness-activation evidence.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, StoreConfig};
use peritus_codec::sha256;
use peritus_evidence::{
    EvidenceError, EvidenceErrorKind, EvidenceRecord, EvidenceStore, EvidenceStoreOptions,
};
use peritus_evolution::qualification::{observe_promotion, prepare_promotion};
use peritus_evolution::{
    EvolutionPublicationClaim, EvolutionPublicationKind, finalize_evolution_artifact,
    publish_claimed_evolution, recover_pointer,
};
use peritus_journal::SqliteJournal;
use peritus_types::{EventId, EvidenceId, ProjectId, Sha256Digest};
use rusqlite::{Connection, OptionalExtension};

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{acquire_instance, journal_error, open_journal};

const CORRUPT_RECORD: &[u8] = b"peritus/h1/deliberately-corrupt-harness-activation/v1";
const EXPECTED_EVENTS: u64 = 16;
const EXPECTED_HEADS: u64 = 4;

/// Exact facts after a real F0 activation was published and its evidence bytes were changed.
pub struct PromotionEvidenceCorruptionCheckpoint {
    evidence_id: EvidenceId,
    record_digest: Sha256Digest,
    corrupt_bytes_sha256: Sha256Digest,
    pointer_digest: Sha256Digest,
    record_bytes: u64,
}

impl PromotionEvidenceCorruptionCheckpoint {
    pub const fn evidence_id(&self) -> EvidenceId {
        self.evidence_id
    }
    pub const fn record_digest(&self) -> Sha256Digest {
        self.record_digest
    }
    pub const fn corrupt_bytes_sha256(&self) -> Sha256Digest {
        self.corrupt_bytes_sha256
    }
    pub const fn pointer_digest(&self) -> Sha256Digest {
        self.pointer_digest
    }
    pub const fn record_bytes(&self) -> u64 {
        self.record_bytes
    }
    pub const fn corruption_detected(&self) -> bool {
        true
    }
}

/// Fresh-process facts after containing corrupt promotion evidence without changing F0 authority.
pub struct PromotionEvidenceCorruptionObservation {
    evidence_id: EvidenceId,
    corrupt_bytes_sha256: Sha256Digest,
    quarantine_digest: Sha256Digest,
    pointer_digest: Sha256Digest,
    record_bytes: u64,
    committed_events: u64,
    aggregate_heads: u64,
}

impl PromotionEvidenceCorruptionObservation {
    pub const fn evidence_id(&self) -> EvidenceId {
        self.evidence_id
    }
    pub const fn corrupt_bytes_sha256(&self) -> Sha256Digest {
        self.corrupt_bytes_sha256
    }
    pub const fn quarantine_digest(&self) -> Sha256Digest {
        self.quarantine_digest
    }
    pub const fn pointer_digest(&self) -> Sha256Digest {
        self.pointer_digest
    }
    pub const fn record_bytes(&self) -> u64 {
        self.record_bytes
    }
    pub const fn committed_events(&self) -> u64 {
        self.committed_events
    }
    pub const fn aggregate_heads(&self) -> u64 {
        self.aggregate_heads
    }
    pub const fn journal_verified(&self) -> bool {
        true
    }
    pub const fn promotion_verified(&self) -> bool {
        true
    }
    pub const fn corruption_detected(&self) -> bool {
        true
    }
    pub const fn mutation_admitted(&self) -> bool {
        false
    }
}

/// Executes the real F0 activation and publication path, then changes only its evidence bytes.
pub fn stage_corruption(
    config: &DaemonConfig,
) -> Result<PromotionEvidenceCorruptionCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let artifacts = open_artifacts(config)?;
    let prepared = prepare_promotion(&mut journal, &artifacts).map_err(evolution_error)?;
    let committed = prepared.commit(&mut journal).map_err(evolution_error)?;
    let identity = committed.identity();
    let pointer_digest = current_pointer_digest(&journal, identity.project_id())?;
    let mut evidence =
        EvidenceStore::open(config.paths().database(), EvidenceStoreOptions::default())
            .map_err(evidence_error)?;
    let activation = publish_both(&mut journal, &mut evidence, &artifacts, config)?;
    let connection = Connection::open(config.paths().database()).map_err(sqlite_error)?;
    let activation_id = activation.id();
    let values: [&[u8]; 2] = [CORRUPT_RECORD, activation_id.as_bytes()];
    let changed = connection
        .execute(
            "UPDATE peritus_evidence_records SET record_bytes = ?1 WHERE evidence_id = ?2",
            values,
        )
        .map_err(sqlite_error)?;
    if changed != 1 || evidence.quarantine_count().map_err(evidence_error)? != 0 {
        return Err(qualification_error("promotion evidence fault changed the wrong state"));
    }
    let error = evidence
        .load(activation.id())
        .expect_err("injected harness-activation evidence must be corrupt");
    if error.kind() != EvidenceErrorKind::CorruptCatalog {
        return Err(qualification_error("promotion evidence fault has the wrong category"));
    }
    Ok(PromotionEvidenceCorruptionCheckpoint {
        evidence_id: activation.id(),
        record_digest: activation.record_digest(),
        corrupt_bytes_sha256: sha256(CORRUPT_RECORD),
        pointer_digest,
        record_bytes: CORRUPT_RECORD.len() as u64,
    })
}

/// Reopens production stores and proves containment did not alter the promoted pointer.
pub fn recover_corruption(
    config: &DaemonConfig,
) -> Result<PromotionEvidenceCorruptionObservation, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let id = faulted_evidence_id(&config.paths().database())?;
    let evidence = EvidenceStore::open(config.paths().database(), EvidenceStoreOptions::default())
        .map_err(evidence_error)?;
    let quarantine = evidence
        .quarantined(id)
        .map_err(evidence_error)?
        .ok_or_else(|| qualification_error("promotion evidence was not quarantined"))?;
    require_denied(&evidence, id)?;
    if evidence.quarantine_count().map_err(evidence_error)? != 1
        || quarantine.record_bytes_sha256() != sha256(CORRUPT_RECORD)
        || quarantine.record_bytes() != CORRUPT_RECORD.len() as u64
    {
        return Err(qualification_error("promotion evidence quarantine differs from the fault"));
    }
    drop(evidence);
    let reopened = EvidenceStore::open(config.paths().database(), EvidenceStoreOptions::default())
        .map_err(evidence_error)?;
    if reopened.quarantined(id).map_err(evidence_error)? != Some(quarantine) {
        return Err(qualification_error("promotion evidence quarantine is not idempotent"));
    }
    require_denied(&reopened, id)?;
    let mut journal = open_journal(config, store_id)?;
    let promotion = observe_promotion(&mut journal, true).map_err(evolution_error)?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    if report.event_count() != EXPECTED_EVENTS || report.aggregate_count() != EXPECTED_HEADS {
        return Err(qualification_error("promotion authority changed during containment"));
    }
    Ok(PromotionEvidenceCorruptionObservation {
        evidence_id: id,
        corrupt_bytes_sha256: quarantine.record_bytes_sha256(),
        quarantine_digest: quarantine.quarantine_digest(),
        pointer_digest: promotion.pointer_digest(),
        record_bytes: quarantine.record_bytes(),
        committed_events: report.event_count(),
        aggregate_heads: report.aggregate_count(),
    })
}

fn publish_both(
    journal: &mut SqliteJournal,
    evidence: &mut EvidenceStore,
    artifacts: &ArtifactStore,
    config: &DaemonConfig,
) -> Result<EvidenceRecord, DaemonError> {
    let mut activation = None;
    let mut campaign_seen = false;
    for index in 0_u8..2 {
        let now = u64::from(index) + 1;
        let message = journal
            .claim_outbox(now, now + 100)
            .map_err(journal_error)?
            .ok_or_else(|| qualification_error("promotion publication directive is absent"))?;
        let claim = EvolutionPublicationClaim::from_message(&message).map_err(evolution_error)?;
        let directive = claim.directive();
        let digest = ArtifactDigest::from_sha256(directive.artifact_digest());
        let bytes = artifacts
            .read(digest, config.limits().maximum_artifact_bytes())
            .map_err(artifact_error)?;
        let event_id = EventId::new([0x90 + index; 16])
            .map_err(|_| qualification_error("promotion publication event ID is invalid"))?;
        let finalized =
            finalize_evolution_artifact(artifacts, &bytes, directive.evidence_digest(), event_id)
                .map_err(evolution_error)?;
        let published = publish_claimed_evolution(journal, evidence, artifacts, &claim, finalized)
            .map_err(evolution_error)?
            .into_evidence();
        match directive.kind() {
            EvolutionPublicationKind::CampaignDecision => campaign_seen = true,
            EvolutionPublicationKind::HarnessActivation => activation = Some(published),
        }
    }
    if !campaign_seen {
        return Err(qualification_error("campaign publication was not settled"));
    }
    activation.ok_or_else(|| qualification_error("harness activation was not published"))
}

fn current_pointer_digest(
    journal: &SqliteJournal,
    project: ProjectId,
) -> Result<Sha256Digest, DaemonError> {
    recover_pointer(journal, project)
        .map_err(evolution_error)?
        .state()
        .map(peritus_evolution::ProductionHarnessState::state_digest)
        .ok_or_else(|| qualification_error("promoted production pointer is absent"))
}

fn faulted_evidence_id(path: &std::path::Path) -> Result<EvidenceId, DaemonError> {
    let connection = Connection::open(path).map_err(sqlite_error)?;
    let id = connection
        .query_row(
            "SELECT evidence_id FROM peritus_evidence_records WHERE record_bytes = ?1",
            [CORRUPT_RECORD],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(sqlite_error)?
        .ok_or_else(|| qualification_error("faulted promotion evidence is absent"))?;
    EvidenceId::new(
        id.as_slice()
            .try_into()
            .map_err(|_| qualification_error("faulted promotion evidence ID is malformed"))?,
    )
    .map_err(|_| qualification_error("faulted promotion evidence ID is reserved"))
}

fn require_denied(store: &EvidenceStore, id: EvidenceId) -> Result<(), DaemonError> {
    if store.load(id).is_err_and(|error| error.kind() == EvidenceErrorKind::CorruptCatalog) {
        Ok(())
    } else {
        Err(qualification_error("quarantined promotion evidence remained usable"))
    }
}

fn open_artifacts(config: &DaemonConfig) -> Result<ArtifactStore, DaemonError> {
    let store = StoreConfig::new(
        config.paths().artifact_root(),
        config.limits().maximum_artifact_bytes(),
        config.limits().artifact_quota_bytes(),
    )
    .and_then(|value| value.with_database_path(config.paths().database()))
    .map_err(artifact_error)?;
    ArtifactStore::open(store).map_err(artifact_error)
}

fn artifact_error(error: peritus_artifact_store::ArtifactStoreError) -> DaemonError {
    source_error("qualify promotion evidence artifact", error)
}
fn evidence_error(error: EvidenceError) -> DaemonError {
    source_error("qualify promotion evidence catalog", error)
}
fn evolution_error(error: peritus_evolution::EvolutionError) -> DaemonError {
    source_error("qualify promotion evidence publication", error)
}
fn sqlite_error(error: rusqlite::Error) -> DaemonError {
    source_error("qualify promotion evidence storage", error)
}
fn source_error(
    operation: &'static str,
    error: impl std::error::Error + Send + Sync + 'static,
) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        operation,
        error.to_string(),
        error,
    )
}
fn qualification_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify harness-promotion evidence corruption",
        detail,
    )
}
