//! Controlled acceptance-evidence corruption and durable startup containment.

use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_codec::{CodecLimits, encode_message, sha256};
use peritus_evidence::{
    EvidenceDraft, EvidenceError, EvidenceErrorKind, EvidenceKind, EvidenceSource, EvidenceStore,
    EvidenceStoreOptions, revision_digest,
};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal,
};
use peritus_kernel::SessionPhase;
use peritus_protocol::LifecyclePhaseDto;
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, EventSequence, EvidenceId, Generation, HarnessId,
    PolicyId, ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};
use rusqlite::Connection;

use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{acquire_instance, journal_error, open_journal};

const CORRUPT_RECORD: &[u8] = b"peritus/h1/deliberately-corrupt-acceptance-evidence/v1";

/// Exact facts after changing one admitted acceptance record's canonical bytes.
pub struct EvidenceCorruptionCheckpoint {
    evidence_id: EvidenceId,
    record_digest: Sha256Digest,
    original_bytes_sha256: Sha256Digest,
    corrupt_bytes_sha256: Sha256Digest,
    record_bytes: u64,
}

impl EvidenceCorruptionCheckpoint {
    pub const fn evidence_id(&self) -> EvidenceId {
        self.evidence_id
    }
    pub const fn record_digest(&self) -> Sha256Digest {
        self.record_digest
    }
    pub const fn original_bytes_sha256(&self) -> Sha256Digest {
        self.original_bytes_sha256
    }
    pub const fn corrupt_bytes_sha256(&self) -> Sha256Digest {
        self.corrupt_bytes_sha256
    }
    pub const fn record_bytes(&self) -> u64 {
        self.record_bytes
    }
    pub const fn corruption_detected(&self) -> bool {
        true
    }
}

/// Fresh-process facts after the evidence catalog isolated the divergent record.
pub struct EvidenceCorruptionObservation {
    evidence_id: EvidenceId,
    corrupt_bytes_sha256: Sha256Digest,
    quarantine_digest: Sha256Digest,
    record_bytes: u64,
    committed_events: u64,
    aggregate_heads: u64,
}

impl EvidenceCorruptionObservation {
    pub const fn evidence_id(&self) -> EvidenceId {
        self.evidence_id
    }
    pub const fn corrupt_bytes_sha256(&self) -> Sha256Digest {
        self.corrupt_bytes_sha256
    }
    pub const fn quarantine_digest(&self) -> Sha256Digest {
        self.quarantine_digest
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
    pub const fn corruption_detected(&self) -> bool {
        true
    }
    pub const fn mutation_admitted(&self) -> bool {
        false
    }
}

/// Admits one real revision-bound evidence record and changes only its portable record bytes.
pub fn stage_corruption(
    config: &DaemonConfig,
) -> Result<EvidenceCorruptionCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    require_empty(&mut journal)?;
    let revision = revision()?;
    let position = append_evidence_event(&mut journal, &revision)?;
    let artifacts = ArtifactStore::open(artifact_config(config)?).map_err(artifact_error)?;
    let mut evidence =
        EvidenceStore::open(config.paths().database(), EvidenceStoreOptions::default())
            .map_err(evidence_error)?;
    let export = journal.integrity_export().map_err(journal_error)?;
    let id = evidence_id()?;
    let record = evidence
        .admit(
            EvidenceDraft::new(
                id,
                EvidenceKind::new("acceptance-decision").map_err(evidence_error)?,
                EvidenceSource::new("peritus-qualification").map_err(evidence_error)?,
                revision,
                position,
                sha256(b"peritus/h1/acceptance-evidence/payload/v1\0"),
                Vec::new(),
                Vec::new(),
            )
            .map_err(evidence_error)?,
            &export,
            &artifacts,
        )
        .map_err(evidence_error)?;
    let canonical = record.canonical_bytes();
    let original_bytes_sha256 = sha256(&canonical);
    let connection = Connection::open(config.paths().database()).map_err(sqlite_error)?;
    let values: [&[u8]; 2] = [CORRUPT_RECORD, id.as_bytes()];
    let changed = connection
        .execute(
            "UPDATE peritus_evidence_records SET record_bytes = ?1 WHERE evidence_id = ?2",
            values,
        )
        .map_err(sqlite_error)?;
    if changed != 1 || evidence.quarantine_count().map_err(evidence_error)? != 0 {
        return Err(qualification_error("evidence fault injection changed the wrong state"));
    }
    let error = evidence.load(id).expect_err("injected evidence must be corrupt");
    if error.kind() != EvidenceErrorKind::CorruptCatalog {
        return Err(qualification_error("evidence fault produced the wrong failure category"));
    }
    Ok(EvidenceCorruptionCheckpoint {
        evidence_id: id,
        record_digest: record.record_digest(),
        original_bytes_sha256,
        corrupt_bytes_sha256: sha256(CORRUPT_RECORD),
        record_bytes: u64::try_from(CORRUPT_RECORD.len())
            .map_err(|_| qualification_error("corrupt evidence size overflowed"))?,
    })
}

/// Reopens the catalog twice and proves the corrupt record is isolated, audited, and denied.
pub fn recover_corruption(
    config: &DaemonConfig,
) -> Result<EvidenceCorruptionObservation, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let artifacts = ArtifactStore::open(artifact_config(config)?).map_err(artifact_error)?;
    drop(artifacts);
    let id = evidence_id()?;
    let evidence = EvidenceStore::open(config.paths().database(), EvidenceStoreOptions::default())
        .map_err(evidence_error)?;
    let quarantine = evidence
        .quarantined(id)
        .map_err(evidence_error)?
        .ok_or_else(|| qualification_error("corrupt acceptance evidence was not quarantined"))?;
    require_denied(&evidence, id)?;
    if evidence.quarantine_count().map_err(evidence_error)? != 1
        || quarantine.record_bytes_sha256() != sha256(CORRUPT_RECORD)
        || quarantine.record_bytes() != CORRUPT_RECORD.len() as u64
    {
        return Err(qualification_error("acceptance evidence quarantine differs from the fault"));
    }
    drop(evidence);
    let reopened = EvidenceStore::open(config.paths().database(), EvidenceStoreOptions::default())
        .map_err(evidence_error)?;
    if reopened.quarantined(id).map_err(evidence_error)? != Some(quarantine) {
        return Err(qualification_error("acceptance evidence quarantine is not idempotent"));
    }
    require_denied(&reopened, id)?;
    let mut journal = open_journal(config, store_id)?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    let connection = Connection::open(config.paths().database()).map_err(sqlite_error)?;
    if count(&connection, "authority_clock")? != 0 || count(&connection, "app_principals")? != 0 {
        return Err(qualification_error("evidence containment admitted authority mutation"));
    }
    Ok(EvidenceCorruptionObservation {
        evidence_id: id,
        corrupt_bytes_sha256: quarantine.record_bytes_sha256(),
        quarantine_digest: quarantine.quarantine_digest(),
        record_bytes: quarantine.record_bytes(),
        committed_events: report.event_count(),
        aggregate_heads: report.aggregate_count(),
    })
}

fn append_evidence_event(
    journal: &mut SqliteJournal,
    revision: &RevisionTuple,
) -> Result<u64, DaemonError> {
    let aggregate = AggregateKey::new(
        AggregateKind::Application,
        AggregateId::new([0x70; 16]).map_err(journal_error)?,
    );
    let frame =
        encode_message(&LifecyclePhaseDto::Session(SessionPhase::Open), CodecLimits::PRODUCTION)
            .map_err(|_| qualification_error("encode acceptance evidence event"))?;
    let event = EventDraft::new(
        aggregate,
        EventSequence::first(),
        EventId::new([0x72; 16]).map_err(|_| qualification_error("invalid evidence event ID"))?,
        None,
        ExactFrame::new(frame).map_err(journal_error)?,
        revision_digest(revision),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let plan = AppendRequest::new(
        journal.store_id(),
        CommandId::new([0x71; 16])
            .map_err(|_| qualification_error("invalid evidence command ID"))?,
        sha256(b"peritus/h1/acceptance-evidence/request/v1\0"),
        vec![HeadExpectation::Absent(aggregate)],
        vec![event],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .map_err(journal_error)?;
    Ok(journal.append(plan).map_err(journal_error)?.first_position())
}

fn require_empty(journal: &mut SqliteJournal) -> Result<(), DaemonError> {
    let report = journal.integrity_scan().map_err(journal_error)?;
    if report.event_count() == 0 && report.aggregate_count() == 0 {
        Ok(())
    } else {
        Err(qualification_error("acceptance evidence qualification journal is not empty"))
    }
}

fn require_denied(store: &EvidenceStore, id: EvidenceId) -> Result<(), DaemonError> {
    let denied =
        store.load(id).is_err_and(|error| error.kind() == EvidenceErrorKind::CorruptCatalog);
    if denied { Ok(()) } else { Err(qualification_error("quarantined evidence remained usable")) }
}

fn artifact_config(config: &DaemonConfig) -> Result<StoreConfig, DaemonError> {
    StoreConfig::new(
        config.paths().artifact_root(),
        config.limits().maximum_artifact_bytes(),
        config.limits().artifact_quota_bytes(),
    )
    .and_then(|value| value.with_database_path(config.paths().database()))
    .map_err(artifact_error)
}

fn revision() -> Result<RevisionTuple, DaemonError> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new([0x74; 16])
            .map_err(|_| qualification_error("invalid acceptance specification ID"))?,
        HarnessId::new([0x75; 16]).map_err(|_| qualification_error("invalid harness ID"))?,
        WorkspaceId::new([0x76; 16]).map_err(|_| qualification_error("invalid workspace ID"))?,
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([0x77; 16]).map_err(|_| qualification_error("invalid policy ID"))?,
        ProviderProfileId::new([0x78; 16])
            .map_err(|_| qualification_error("invalid provider profile ID"))?,
    ))
}

fn evidence_id() -> Result<EvidenceId, DaemonError> {
    EvidenceId::new([0x73; 16]).map_err(|_| qualification_error("invalid evidence ID"))
}

fn count(connection: &Connection, table: &'static str) -> Result<u64, DaemonError> {
    let value = connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get::<_, i64>(0))
        .map_err(sqlite_error)?;
    u64::try_from(value).map_err(|_| qualification_error("evidence row count is negative"))
}

fn artifact_error(error: peritus_artifact_store::ArtifactStoreError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify acceptance evidence corruption",
        error.to_string(),
        error,
    )
}

fn evidence_error(error: EvidenceError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify acceptance evidence corruption",
        error.to_string(),
        error,
    )
}

fn sqlite_error(error: rusqlite::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify acceptance evidence corruption",
        error.to_string(),
        error,
    )
}

fn qualification_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "qualify acceptance evidence corruption containment",
        detail,
    )
}
