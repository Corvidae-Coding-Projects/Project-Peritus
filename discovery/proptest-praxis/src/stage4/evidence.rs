//! Native evidence admissions and portable causal bundles against input-derived bytes.

use super::journal::{bump, bytes, error, hash};
use crate::Metrics;
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, Publication, StoreConfig,
    WriteRequest,
};
use peritus_evidence::{
    BundleExportCursor, BundleExportOperation, BundleLimits, BundlePlan,
    BundleVerificationOperation, EvidenceAdmission, EvidenceCancellation, EvidenceDraft,
    EvidenceError, EvidenceErrorKind, EvidenceId, EvidenceInvalidation, EvidenceKind,
    EvidenceRecord, EvidenceSource, EvidenceStore, EvidenceStoreOptions, Freshness, RevisionDrift,
    assemble_bundle, resume_bundle, revision_digest, verify_bundle,
};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, ArtifactDependency, EventDraft,
    ExactFrame, HeadExpectation, IntegrityExport, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, EventSequence, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Cursor, Write},
    num::NonZeroUsize,
    path::PathBuf,
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    salt: u8,
    steps: Vec<Op>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum BundleMode {
    Normal,
    ShortWrite,
    Cancel,
    CorruptSource,
    Malformed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum Op {
    Admit { slot: u8, parents: u8 },
    SeedGraph,
    Retry { slot: u8, conflict: bool, broken_dependencies: bool },
    BadBinding { slot: u8, defect: u8 },
    Freshness { slot: u8, axis: u8 },
    Invalidate { slot: u8, defect: u8 },
    Bundle { authority: u8, axis: u8, mode: BundleMode },
    Reopen,
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let mode = proptest::sample::select(vec![
        BundleMode::Normal,
        BundleMode::ShortWrite,
        BundleMode::Cancel,
        BundleMode::CorruptSource,
        BundleMode::Malformed,
    ]);
    let operation = prop_oneof![
        5=>(0_u8..4,0_u8..16).prop_map(|(slot,parents)|Op::Admit{slot,parents}),
        3=>Just(Op::SeedGraph),
        4=>(0_u8..4,any::<bool>(),any::<bool>()).prop_map(|(slot,conflict,broken_dependencies)|Op::Retry{slot,conflict,broken_dependencies}),
        4=>(0_u8..4,0_u8..4).prop_map(|(slot,defect)|Op::BadBinding{slot,defect}),
        5=>(0_u8..4,0_u8..8).prop_map(|(slot,axis)|Op::Freshness{slot,axis}),
        3=>(0_u8..4,0_u8..4).prop_map(|(slot,defect)|Op::Invalidate{slot,defect}),
        7=>(0_u8..16,0_u8..8,mode.clone()).prop_map(|(authority,axis,mode)|Op::Bundle{authority,axis,mode}),
        2=>Just(Op::Reopen),
    ];
    let ordinary = (any::<u8>(), prop::collection::vec(operation, 0..=max_steps.min(48)))
        .prop_map(|(salt, steps)| Case { salt, steps });
    let constructive = (any::<u8>(), mode, 1_u8..8).prop_map(move |(salt, mode, axis)| {
        let mut steps = vec![
            Op::Admit { slot: 1, parents: 0 },
            Op::Admit { slot: 0, parents: 2 },
            Op::SeedGraph,
            Op::Freshness { slot: 3, axis },
            Op::Bundle { authority: 8, axis: 0, mode },
            Op::Invalidate { slot: 0, defect: 0 },
            Op::Bundle { authority: 8, axis: 0, mode: BundleMode::Normal },
            Op::Retry { slot: 3, conflict: false, broken_dependencies: true },
            Op::Reopen,
        ];
        steps.truncate(max_steps.min(48));
        Case { salt, steps }
    });
    prop_oneof![4=>ordinary,2=>constructive].boxed()
}

fn identity(slot: u8) -> Result<EvidenceId, String> {
    EvidenceId::new(bytes(6000 + u64::from(slot))).map_err(error)
}
fn version(slot: u8) -> u8 {
    u8::from(slot >= 2)
}
fn tuple(version: u8, axis: u8) -> Result<RevisionTuple, String> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new([if axis == 1 { 41 } else { 11 }; 16]).map_err(error)?,
        HarnessId::new([if axis == 2 { 42 } else { 12 }; 16]).map_err(error)?,
        WorkspaceId::new([if axis == 3 { 43 } else { 13 }; 16]).map_err(error)?,
        Generation::new(if axis == 4 { 2 } else { 1 }).map_err(error)?,
        RevisionNumber::new(if axis == 5 { 9 } else { 1 + u64::from(version) }).map_err(error)?,
        PolicyId::new([if axis == 6 { 46 } else { 14 }; 16]).map_err(error)?,
        ProviderProfileId::new([if axis == 7 { 47 } else { 15 }; 16]).map_err(error)?,
    ))
}
fn revision_bytes(version: u8, axis: u8) -> Vec<u8> {
    [
        vec![if axis == 1 { 41 } else { 11 }; 16],
        vec![if axis == 2 { 42 } else { 12 }; 16],
        vec![if axis == 3 { 43 } else { 13 }; 16],
        (if axis == 4 { 2_u64 } else { 1 }).to_be_bytes().to_vec(),
        (if axis == 5 { 9 } else { 1 + u64::from(version) }).to_be_bytes().to_vec(),
        vec![if axis == 6 { 46 } else { 14 }; 16],
        vec![if axis == 7 { 47 } else { 15 }; 16],
    ]
    .concat()
}
fn content(salt: u8, slot: u8) -> Vec<u8> {
    (0..64 + slot * 7).map(|n| salt.wrapping_add(n).wrapping_add(slot)).collect()
}
fn artifact(salt: u8, slot: u8) -> ArtifactDigest {
    ArtifactDigest::from_sha256(hash(&content(salt, slot)))
}
fn raw_frame(salt: u8, index: u8) -> Vec<u8> {
    [b"PRTS".as_slice(), &[0, 1, 0, 1, 0, 1, 0, 0], &2_u32.to_be_bytes(), &[salt, index]].concat()
}
fn event_hash(salt: u8, index: u8) -> Sha256Digest {
    hash(
        &[
            b"peritus.journal.event.v1\0".as_slice(),
            &8_u16.to_be_bytes(),
            &bytes(5000 + u64::from(index)),
            &1_u64.to_be_bytes(),
            &bytes(5200 + u64::from(index)),
            &[0],
            &[0; 16],
            &[0; 32],
            &bytes(5100 + u64::from(index)),
            hash(&raw_frame(salt, index)).as_bytes(),
            hash(&revision_bytes(if index < 4 { version(index) } else { 0 }, 0)).as_bytes(),
            &0_u32.to_be_bytes(),
        ]
        .concat(),
    )
}
fn batch_hash(salt: u8, index: u8) -> Sha256Digest {
    let mut input = [
        b"peritus.journal.batch.v1\0".as_slice(),
        &[51; 16],
        &bytes(5100 + u64::from(index)),
        hash(&[salt, index, 99]).as_bytes(),
        &1_u32.to_be_bytes(),
        event_hash(salt, index).as_bytes(),
        &u32::from(index < 4).to_be_bytes(),
    ]
    .concat();
    if index < 4 {
        input.extend(artifact(salt, index).as_bytes());
    }
    hash(&input)
}
fn journal_head(salt: u8) -> Sha256Digest {
    let mut input = [
        b"peritus.journal.head.v1\0".as_slice(),
        &[51; 16],
        &8_u64.to_be_bytes(),
        &8_u32.to_be_bytes(),
    ]
    .concat();
    for index in 0_u8..8 {
        input.extend(8_u16.to_be_bytes());
        input.extend(bytes(5000 + u64::from(index)));
        input.extend(1_u64.to_be_bytes());
        input.extend(event_hash(salt, index).as_bytes());
    }
    hash(&input)
}
fn schema_digest() -> Sha256Digest {
    hash(
        &[b"peritus-evidence-frame-schema-v1\0".as_slice(), &[0, 1, 0, 1], b"kernel-command"]
            .concat(),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Record {
    slot: u8,
    parents: u8,
    payload: u8,
}
impl Record {
    fn draft(&self, salt: u8) -> Result<Result<EvidenceDraft, EvidenceError>, String> {
        Ok(EvidenceDraft::new(
            identity(self.slot)?,
            EvidenceKind::new("native-evidence").map_err(error)?,
            EvidenceSource::new("praxis").map_err(error)?,
            tuple(version(self.slot), 0)?,
            u64::from(self.slot) + 1,
            hash(&[salt, self.slot, self.payload]),
            vec![artifact(salt, self.slot)],
            (0_u8..4)
                .filter(|slot| self.parents & (1 << slot) != 0)
                .map(identity)
                .collect::<Result<_, _>>()?,
        ))
    }
    fn canonical(&self, salt: u8) -> Result<(Vec<u8>, Sha256Digest), String> {
        let mut body = b"peritus-evidence-record-v1\0".to_vec();
        body.extend(identity(self.slot)?.as_bytes());
        put_bytes(&mut body, b"native-evidence")?;
        put_bytes(&mut body, b"praxis")?;
        body.extend(revision_bytes(version(self.slot), 0));
        body.extend((u64::from(self.slot) + 1).to_be_bytes());
        body.extend(bytes(5200 + u64::from(self.slot)));
        body.extend(event_hash(salt, self.slot).as_bytes());
        body.extend(batch_hash(salt, self.slot).as_bytes());
        body.extend(journal_head(salt).as_bytes());
        body.extend([0, 1, 0, 1]);
        body.extend(hash(&raw_frame(salt, self.slot)).as_bytes());
        body.extend(schema_digest().as_bytes());
        body.extend(hash(&revision_bytes(version(self.slot), 0)).as_bytes());
        body.extend(hash(&[salt, self.slot, self.payload]).as_bytes());
        body.extend(1_u64.to_be_bytes());
        body.extend(artifact(salt, self.slot).as_bytes());
        body.extend(u64::from(self.parents.count_ones()).to_be_bytes());
        for parent in 0..4 {
            if self.parents & (1 << parent) != 0 {
                body.extend(identity(parent)?.as_bytes());
            }
        }
        let digest = hash(&body);
        let mut record = Vec::new();
        put_bytes(&mut record, &body)?;
        record.extend(digest.as_bytes());
        Ok((record, digest))
    }
}
fn put_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), String> {
    output.extend(u64::try_from(bytes.len()).map_err(error)?.to_be_bytes());
    output.extend(bytes);
    Ok(())
}
fn require<T: std::fmt::Debug>(
    result: Result<T, EvidenceError>,
    kind: EvidenceErrorKind,
) -> Result<(), String> {
    match result {
        Err(failure) if failure.kind() == kind => Ok(()),
        other => Err(format!("expected evidence {kind:?}, got {other:?}")),
    }
}

struct Fixture {
    evidence: Option<EvidenceStore>,
    artifacts: Option<ArtifactStore>,
    journal: Option<SqliteJournal>,
    export: IntegrityExport,
    empty: IntegrityExport,
    config: StoreConfig,
    database: PathBuf,
    records: BTreeMap<u8, Record>,
    invalid: BTreeSet<u8>,
    salt: u8,
    metrics: Metrics,
    next: u64,
    _directory: tempfile::TempDir,
}
impl Fixture {
    fn new(salt: u8) -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(error)?;
        let database = directory.path().join("shared.sqlite");
        let mut journal = SqliteJournal::open(
            &database,
            StoreId::new([51; 16]).map_err(error)?,
            SqliteJournalOptions { busy_timeout: Duration::ZERO },
        )
        .map_err(error)?;
        let empty = journal.integrity_export().map_err(error)?;
        let config = StoreConfig::new(directory.path().join("artifacts"), 128, 4096)
            .map_err(error)?
            .with_database_path(&database)
            .map_err(error)?;
        let mut artifacts = ArtifactStore::open(config.clone()).map_err(error)?;
        for index in 0_u8..4 {
            write_artifact(&mut artifacts, salt, index)?;
        }
        for index in 0_u8..8 {
            append(&mut journal, salt, index)?;
        }
        let export = journal.integrity_export().map_err(error)?;
        if export.report().journal_head_digest() != journal_head(salt) {
            return Err("independent seeded journal head differs".into());
        }
        let evidence = EvidenceStore::open(&database, EvidenceStoreOptions::new(Duration::ZERO))
            .map_err(error)?;
        Ok(Self {
            evidence: Some(evidence),
            artifacts: Some(artifacts),
            journal: Some(journal),
            export,
            empty,
            config,
            database,
            records: BTreeMap::new(),
            invalid: BTreeSet::new(),
            salt,
            metrics: Metrics::new(),
            next: 0,
            _directory: directory,
        })
    }
    fn store(&self) -> Result<&EvidenceStore, String> {
        self.evidence.as_ref().ok_or_else(|| "evidence owner closed".into())
    }
    fn artifact_store(&self) -> Result<&ArtifactStore, String> {
        self.artifacts.as_ref().ok_or_else(|| "artifact owner closed".into())
    }
    fn admit(&mut self, record: Record) -> Result<(), String> {
        let draft = record.draft(self.salt)?;
        let expected = if record.parents & (1 << record.slot) != 0 {
            Some(EvidenceErrorKind::InvalidInput)
        } else if let Some(existing) = self.records.get(&record.slot) {
            (existing != &record).then_some(EvidenceErrorKind::IdentityConflict)
        } else if (0_u8..4).any(|p| {
            record.parents & (1 << p) != 0 && (p >= record.slot || !self.records.contains_key(&p))
        }) {
            Some(EvidenceErrorKind::InvalidCause)
        } else {
            None
        };
        let result = match draft {
            Ok(draft) if !self.records.contains_key(&record.slot) => {
                let result = incremental_admit(
                    self.evidence.as_mut().ok_or("evidence closed")?,
                    draft,
                    &self.export,
                    self.artifacts.as_ref().ok_or("artifacts closed")?,
                )?;
                bump(&mut self.metrics, "admission_cancelled_hash_resumes");
                result
            }
            Ok(draft) => self.evidence.as_mut().ok_or("evidence closed")?.admit(
                draft,
                &self.export,
                self.artifacts.as_ref().ok_or("artifacts closed")?,
            ),
            Err(failure) => Err(failure),
        };
        if let Some(kind) = expected {
            require(result, kind)?;
            bump(&mut self.metrics, "admission_rejections");
            let partition = if record.parents & (1 << record.slot) != 0 {
                "self_cause_constructor_rejections"
            } else if self.records.contains_key(&record.slot) {
                "admission_identity_conflicts"
            } else if (0_u8..4).any(|parent| {
                record.parents & (1 << parent) != 0 && !self.records.contains_key(&parent)
            }) {
                "missing_parent_admission_rejections"
            } else {
                "newer_parent_admission_rejections"
            };
            bump(&mut self.metrics, partition);
        } else {
            let actual = result.map_err(error)?;
            check_record(&actual, &record, self.salt)?;
            self.records.insert(record.slot, record);
            bump(&mut self.metrics, "admissions_or_exact_retries");
        }
        self.verify()
    }
    fn seed(&mut self) -> Result<(), String> {
        for (slot, parents) in [(0, 0), (1, 1), (2, 3), (3, 6)] {
            if !self.records.contains_key(&slot) {
                self.admit(Record { slot, parents, payload: 0 })?;
            }
        }
        bump(&mut self.metrics, "causal_graph_waves");
        Ok(())
    }
    fn verify(&self) -> Result<(), String> {
        for slot in 0_u8..4 {
            let actual = self.store()?.load(identity(slot)?).map_err(error)?;
            match (actual, self.records.get(&slot)) {
                (Some(actual), Some(expected)) => check_record(&actual, expected, self.salt)?,
                (None, None) => {}
                other => return Err(format!("durable evidence map differs: {other:?}")),
            }
        }
        Ok(())
    }
    fn object(&self, slot: u8) -> Result<PathBuf, String> {
        let hex = artifact(self.salt, slot).to_hex();
        Ok(self.artifact_store()?.root().join("objects/sha256").join(&hex[..2]).join(hex))
    }
    fn retry(&mut self, slot: u8, conflict: bool, broken: bool) -> Result<(), String> {
        self.seed()?;
        let mut record = self.records.get(&slot).ok_or("seeded record absent")?.clone();
        if conflict {
            record.payload = record.payload.wrapping_add(1);
        }
        let path = self.object(slot)?;
        if broken {
            fs::write(&path, vec![0; content(self.salt, slot).len()]).map_err(error)?;
        }
        let result = self.evidence.as_mut().ok_or("evidence closed")?.admit(
            record.draft(self.salt)?.map_err(error)?,
            if broken { &self.empty } else { &self.export },
            self.artifacts.as_ref().ok_or("artifacts closed")?,
        );
        if broken {
            fs::write(path, content(self.salt, slot)).map_err(error)?;
        }
        if conflict {
            require(result, EvidenceErrorKind::IdentityConflict)?;
            bump(&mut self.metrics, "exact_identity_conflicts");
        } else {
            check_record(&result.map_err(error)?, &record, self.salt)?;
            bump(&mut self.metrics, "exact_retries");
        }
        if broken {
            bump(&mut self.metrics, "receipt_before_dependency_checks");
        }
        self.verify()
    }
    fn bad_binding(&mut self, slot: u8, defect: u8) -> Result<(), String> {
        self.next += 1;
        let id = EvidenceId::new(bytes(9000 + self.next)).map_err(error)?;
        let draft = EvidenceDraft::new(
            id,
            EvidenceKind::new("native-evidence").map_err(error)?,
            EvidenceSource::new("praxis").map_err(error)?,
            tuple(version(slot), u8::from(defect == 0)).map_err(error)?,
            if defect == 2 { 99 } else { u64::from(slot) + 1 },
            hash(b"bad binding"),
            if defect == 1 { vec![] } else { vec![artifact(self.salt, slot)] },
            vec![],
        )
        .map_err(error)?;
        let result = self.evidence.as_mut().ok_or("evidence closed")?.admit(
            draft,
            if defect == 3 { &self.empty } else { &self.export },
            self.artifacts.as_ref().ok_or("artifacts closed")?,
        );
        require(
            result,
            match defect {
                0 => EvidenceErrorKind::RevisionMismatch,
                1 => EvidenceErrorKind::JournalMismatch,
                _ => EvidenceErrorKind::MissingJournalRecord,
            },
        )?;
        if self.store()?.load(id).map_err(error)?.is_some() {
            return Err("rejected binding retained evidence".into());
        }
        bump(&mut self.metrics, "binding_rejections");
        bump(
            &mut self.metrics,
            match defect {
                0 => "revision_binding_rejections",
                1 => "artifact_set_binding_rejections",
                2 => "missing_journal_binding_rejections",
                _ => "missing_export_binding_rejections",
            },
        );
        self.verify()
    }
    fn freshness(&mut self, slot: u8, axis: u8) -> Result<(), String> {
        self.seed()?;
        let actual = self
            .store()?
            .freshness(identity(slot)?, &tuple(version(slot), axis)?)
            .map_err(error)?;
        let good = if self.invalid.contains(&slot) {
            matches!(actual,Freshness::Invalidated(value) if invalidation_matches(value, slot, self.salt)?)
        } else {
            actual
                == if axis == 0 {
                    Freshness::Current
                } else {
                    Freshness::RevisionStale(drift(axis)?)
                }
        };
        if !good {
            return Err(format!(
                "independent freshness axis {axis} invalidated={} disagrees: {actual:?}",
                self.invalid.contains(&slot)
            ));
        }
        bump(&mut self.metrics, &format!("freshness_axis_{axis}"));
        if self.invalid.contains(&slot) {
            bump(&mut self.metrics, "durable_invalidation_dominates_revision");
        }
        Ok(())
    }
    fn invalidate(&mut self, slot: u8, defect: u8) -> Result<(), String> {
        self.seed()?;
        let index = if defect == 1 { slot } else { slot + 4 };
        let target = if defect == 3 {
            EvidenceId::new(bytes(8999)).map_err(error)?
        } else {
            identity(slot)?
        };
        let invalidation = EvidenceInvalidation::new(
            target,
            u64::from(index) + 1,
            EventId::new(bytes(5200 + u64::from(index))).map_err(error)?,
            if defect == 2 { hash(b"wrong event") } else { event_hash(self.salt, index) },
            hash(b"retire evidence"),
        )
        .map_err(error)?;
        let result =
            self.evidence.as_mut().ok_or("evidence closed")?.invalidate(invalidation, &self.export);
        if defect == 0 {
            result.map_err(error)?;
            self.invalid.insert(slot);
            bump(&mut self.metrics, "durable_invalidations");
        } else {
            require(result, EvidenceErrorKind::InvalidInput)?;
            bump(&mut self.metrics, "invalidation_rejections");
            bump(
                &mut self.metrics,
                match defect {
                    1 => "nonlater_invalidation_rejections",
                    2 => "wrong_event_invalidation_rejections",
                    _ => "missing_target_invalidation_rejections",
                },
            );
        }
        self.freshness(slot, 0)?;
        self.verify()
    }
    fn reopen(&mut self) -> Result<(), String> {
        drop(self.evidence.take());
        drop(self.artifacts.take());
        drop(self.journal.take());
        self.journal = Some(
            SqliteJournal::open(
                &self.database,
                StoreId::new([51; 16]).map_err(error)?,
                SqliteJournalOptions { busy_timeout: Duration::ZERO },
            )
            .map_err(error)?,
        );
        self.artifacts = Some(ArtifactStore::open(self.config.clone()).map_err(error)?);
        self.evidence = Some(
            EvidenceStore::open(&self.database, EvidenceStoreOptions::new(Duration::ZERO))
                .map_err(error)?,
        );
        self.verify()?;
        for slot in self.records.keys().copied().collect::<Vec<_>>() {
            self.freshness(slot, 0)?;
        }
        bump(&mut self.metrics, "actual_shared_store_reopens");
        Ok(())
    }
}

fn invalidation_matches(value: EvidenceInvalidation, slot: u8, salt: u8) -> Result<bool, String> {
    let position = u64::from(slot) + 5;
    let event = bytes(5204 + u64::from(slot));
    let event_hash = event_hash(salt, slot + 4);
    let reason = hash(b"retire evidence");
    let digest = hash(
        &[
            b"peritus-evidence-invalidation-v1\0".as_slice(),
            identity(slot)?.as_bytes(),
            &position.to_be_bytes(),
            &event,
            event_hash.as_bytes(),
            reason.as_bytes(),
        ]
        .concat(),
    );
    Ok(value.target() == identity(slot)?
        && value.global_position() == position
        && value.event_id().as_bytes() == &event
        && value.event_hash() == event_hash
        && value.reason_digest() == reason
        && value.invalidation_digest() == digest)
}

fn drift(axis: u8) -> Result<RevisionDrift, String> {
    Ok(match axis {
        1 => RevisionDrift::AcceptanceSpec,
        2 => RevisionDrift::Harness,
        3 => RevisionDrift::Workspace,
        4 => RevisionDrift::WorkspaceGeneration,
        5 => RevisionDrift::WorkspaceRevision,
        6 => RevisionDrift::Policy,
        7 => RevisionDrift::ProviderProfile,
        _ => return Err("setup-incomplete: unknown freshness axis".into()),
    })
}
fn write_artifact(store: &mut ArtifactStore, salt: u8, slot: u8) -> Result<(), String> {
    let content = content(salt, slot);
    let request = WriteRequest::new(
        artifact(salt, slot),
        u64::try_from(content.len()).map_err(error)?,
        128,
        MediaType::new("application/octet-stream").map_err(error)?,
        EncryptionMetadata::unencrypted(),
        EventId::new(bytes(5200 + u64::from(slot))).map_err(error)?,
    );
    let mut writer = store.begin_write(request).map_err(error)?;
    writer.write_chunk(&content).map_err(error)?;
    let finalized = writer.finalize().map_err(error)?;
    if finalized.digest() != artifact(salt, slot)
        || finalized.size() != u64::try_from(content.len()).map_err(error)?
        || finalized.publication() != Publication::New
    {
        return Err(format!("artifact did not finalize exact new bytes: {finalized:?}"));
    }
    Ok(())
}
fn append(journal: &mut SqliteJournal, salt: u8, index: u8) -> Result<(), String> {
    let aggregate = AggregateKey::new(
        AggregateKind::Trace,
        AggregateId::new(bytes(5000 + u64::from(index))).map_err(error)?,
    );
    let event = EventDraft::new(
        aggregate,
        EventSequence::new(1).map_err(error)?,
        EventId::new(bytes(5200 + u64::from(index))).map_err(error)?,
        None,
        ExactFrame::new(raw_frame(salt, index)).map_err(error)?,
        revision_digest(&tuple(if index < 4 { version(index) } else { 0 }, 0)?),
        vec![],
    )
    .map_err(error)?;
    let plan = AppendRequest::new(
        journal.store_id(),
        CommandId::new(bytes(5100 + u64::from(index))).map_err(error)?,
        hash(&[salt, index, 99]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![event],
        vec![],
        if index < 4 { vec![ArtifactDependency::new(hash(&content(salt, index)))] } else { vec![] },
        None,
        None,
        vec![],
    )
    .plan()
    .map_err(error)?;
    let batch = journal.append(plan).map_err(error)?;
    if batch.last_position() != u64::from(index) + 1
        || batch.batch_hash() != batch_hash(salt, index)
    {
        return Err("independent producer event/batch differs".into());
    }
    Ok(())
}
fn check_record(actual: &EvidenceRecord, expected: &Record, salt: u8) -> Result<(), String> {
    let (bytes, digest) = expected.canonical(salt)?;
    if actual.canonical_bytes() != bytes
        || actual.record_digest() != digest
        || actual.id() != identity(expected.slot)?
    {
        return Err(
            "durable evidence record differs from independent complete canonical input bytes"
                .into(),
        );
    }
    Ok(())
}

fn incremental_admit(
    store: &mut EvidenceStore,
    draft: EvidenceDraft,
    export: &IntegrityExport,
    artifacts: &ArtifactStore,
) -> Result<Result<EvidenceRecord, EvidenceError>, String> {
    let live = EvidenceCancellation::new();
    let mut operation = EvidenceAdmission::new(store, draft, export, artifacts);
    if operation.advance(nonzero(3)?, &live).map_err(error)?.is_some()
        || operation.artifact_offset() != 3
        || operation.verified_artifacts() != 0
    {
        return Err("new admission did not retain exact first hash prefix".into());
    }
    let cancelled = EvidenceCancellation::new();
    cancelled.cancel();
    require(operation.advance(nonzero(5)?, &cancelled).map(|_| ()), EvidenceErrorKind::Cancelled)?;
    if operation.artifact_offset() != 3 || operation.verified_artifacts() != 0 {
        return Err(
            "admission cancellation discarded/advanced retained artifact hash cursor".into()
        );
    }
    Ok(operation.finish(&live))
}

struct ExpectedBundle {
    slots: Vec<u8>,
    manifest: Vec<u8>,
    stream: Vec<u8>,
    root: Sha256Digest,
}
impl ExpectedBundle {
    fn new(
        fixture: &Fixture,
        authority: &[u8],
        current_version: u8,
        axis: u8,
    ) -> Result<Self, String> {
        // Deliberately list/stack based, rather than the owner's BTreeSet work queue.
        let mut pending = authority.to_vec();
        let mut closure = Vec::new();
        while let Some(slot) = pending.pop() {
            if closure.contains(&slot) {
                continue;
            }
            closure.push(slot);
            let record = fixture.records.get(&slot).ok_or("model closure record absent")?;
            for parent in 0_u8..4 {
                if record.parents & (1 << parent) != 0 {
                    pending.push(parent);
                }
            }
        }
        closure.sort_unstable();
        let mut body = if closure == authority {
            b"peritus-evidence-manifest-v1\0".to_vec()
        } else {
            b"peritus-evidence-manifest-v3\0".to_vec()
        };
        body.extend(revision_bytes(current_version, axis));
        body.extend(journal_head(fixture.salt).as_bytes());
        let count = u64::try_from(closure.len()).map_err(error)?;
        body.extend(count.to_be_bytes());
        let records = closure
            .iter()
            .map(|slot| {
                fixture.records.get(slot).ok_or("model record absent")?.canonical(fixture.salt)
            })
            .collect::<Result<Vec<_>, String>>()?;
        for (slot, (_, digest)) in closure.iter().zip(&records) {
            body.extend(identity(*slot)?.as_bytes());
            body.extend(digest.as_bytes());
        }
        body.extend(count.to_be_bytes());
        for slot in &closure {
            body.extend((u64::from(*slot) + 1).to_be_bytes());
            body.extend(bytes(5200 + u64::from(*slot)));
            body.extend(event_hash(fixture.salt, *slot).as_bytes());
            body.extend(hash(&raw_frame(fixture.salt, *slot)).as_bytes());
            body.extend(schema_digest().as_bytes());
            body.extend(
                u64::try_from(raw_frame(fixture.salt, *slot).len()).map_err(error)?.to_be_bytes(),
            );
        }
        let artifacts = closure
            .iter()
            .map(|slot| (artifact(fixture.salt, *slot), content(fixture.salt, *slot)))
            .collect::<BTreeMap<_, _>>();
        body.extend(u64::try_from(artifacts.len()).map_err(error)?.to_be_bytes());
        for (digest, content) in &artifacts {
            body.extend(digest.as_bytes());
            body.extend(u64::try_from(content.len()).map_err(error)?.to_be_bytes());
        }
        if closure != authority {
            body.extend(u64::try_from(authority.len()).map_err(error)?.to_be_bytes());
            for slot in authority {
                body.extend(identity(*slot)?.as_bytes());
            }
        }
        let digest = hash(&body);
        let root =
            hash(&[b"peritus-evidence-bundle-root-v1\0".as_slice(), digest.as_bytes()].concat());
        body.extend(digest.as_bytes());
        body.extend(root.as_bytes());
        let mut stream = b"PEREVB1\0".to_vec();
        put_bytes(&mut stream, &body)?;
        stream.extend(count.to_be_bytes());
        for (record, _) in records {
            put_bytes(&mut stream, &record)?;
        }
        stream.extend(count.to_be_bytes());
        for slot in &closure {
            stream.extend((u64::from(*slot) + 1).to_be_bytes());
            put_bytes(&mut stream, &raw_frame(fixture.salt, *slot))?;
        }
        stream.extend(u64::try_from(artifacts.len()).map_err(error)?.to_be_bytes());
        for (digest, content) in artifacts {
            stream.extend(digest.as_bytes());
            put_bytes(&mut stream, &content)?;
        }
        stream.extend(root.as_bytes());
        Ok(Self { slots: closure, manifest: body, stream, root })
    }
    fn check_plan(&self, plan: &BundlePlan) -> Result<(), String> {
        let count = u64::try_from(self.stream.len()).map_err(error)?;
        let export_id = hash(
            &[
                b"peritus-evidence-export-v1\0".as_slice(),
                self.root.as_bytes(),
                &count.to_be_bytes(),
            ]
            .concat(),
        );
        if plan.manifest().canonical_bytes() != self.manifest
            || plan.byte_count() != count
            || plan.export_id() != export_id
            || plan.records().iter().map(|r| *r.id().as_bytes()).collect::<Vec<_>>()
                != self
                    .slots
                    .iter()
                    .map(|s| identity(*s).map(|id| *id.as_bytes()))
                    .collect::<Result<Vec<_>, _>>()?
        {
            return Err("bundle plan manifest/closure/size/identity differs from independent graph and bytes".into());
        }
        Ok(())
    }
}
impl Fixture {
    fn bundle(&mut self, mask: u8, axis: u8, mode: BundleMode) -> Result<(), String> {
        self.seed()?;
        let authority = (0_u8..4).filter(|slot| mask & (1 << slot) != 0).collect::<Vec<_>>();
        let selected_version = authority.first().map_or(0, |slot| version(*slot));
        let current = tuple(selected_version, axis)?;
        let ids = authority.iter().copied().map(identity).collect::<Result<Vec<_>, _>>()?;
        let expected = ExpectedBundle::new(self, &authority, selected_version, axis)?;
        let rejected = if ids.is_empty() {
            Some(EvidenceErrorKind::InvalidBundle)
        } else if axis != 0
            || authority.iter().any(|slot| version(*slot) != selected_version)
            || expected.slots.iter().any(|s| self.invalid.contains(s))
        {
            Some(EvidenceErrorKind::StaleEvidence)
        } else {
            None
        };
        let plan = self.store()?.plan_bundle(
            &ids,
            &current,
            &self.export,
            self.artifact_store()?,
            BundleLimits::default(),
        );
        if let Some(kind) = rejected {
            require(plan, kind)?;
            bump(&mut self.metrics, "bundle_authority_or_invalidation_rejections");
            return Ok(());
        }
        let plan = plan.map_err(error)?;
        expected.check_plan(&plan)?;
        for record in plan.records() {
            let slot = expected
                .slots
                .iter()
                .copied()
                .find(|s| identity(*s).is_ok_and(|id| id == record.id()))
                .ok_or("unexpected plan record")?;
            check_record(record, self.records.get(&slot).ok_or("model missing")?, self.salt)?;
        }
        let second = self
            .store()?
            .plan_bundle(
                &ids,
                &current,
                &self.export,
                self.artifact_store()?,
                BundleLimits::default(),
            )
            .map_err(error)?;
        if second != plan {
            return Err("identical graph planning was nondeterministic".into());
        }
        run_bundle(self, &plan, &expected, mode)?;
        bump(&mut self.metrics, "exact_causal_bundle_plans");
        if expected.slots.iter().any(|slot| version(*slot) != selected_version) {
            bump(&mut self.metrics, "historical_parent_revision_drift_allowed");
        }
        *self.metrics.entry("bundle_closure_records".into()).or_default() +=
            u64::try_from(expected.slots.len()).map_err(error)?;
        self.verify()
    }
}
struct FailingWriter {
    accepted: Vec<u8>,
    limit: usize,
}
impl Write for FailingWriter {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let count = input.len().min(self.limit.saturating_sub(self.accepted.len()));
        if count == 0 {
            return Err(io::Error::other("owned output failure after accepted prefix"));
        }
        self.accepted.extend_from_slice(&input[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn nonzero(value: usize) -> Result<NonZeroUsize, String> {
    NonZeroUsize::new(value).ok_or_else(|| "zero local I/O budget".into())
}
fn short_write(
    fixture: &Fixture,
    plan: &BundlePlan,
    expected: &ExpectedBundle,
) -> Result<(), String> {
    let limit = 1 + usize::from(fixture.salt) % expected.stream.len().saturating_sub(1);
    let mut writer = FailingWriter { accepted: vec![], limit };
    let mut cursor = BundleExportCursor::start(plan);
    require(
        resume_bundle(
            plan,
            fixture.artifact_store()?,
            &mut writer,
            BundleLimits::default(),
            &mut cursor,
        ),
        EvidenceErrorKind::Io,
    )?;
    if cursor.byte_offset() != u64::try_from(writer.accepted.len()).map_err(error)?
        || writer.accepted != expected.stream[..limit]
    {
        return Err("failed output cursor differs from exact accepted bytes".into());
    }
    let mut suffix = Vec::new();
    let receipt = resume_bundle(
        plan,
        fixture.artifact_store()?,
        &mut suffix,
        BundleLimits::default(),
        &mut cursor,
    )
    .map_err(error)?;
    writer.accepted.extend(suffix);
    if writer.accepted != expected.stream
        || cursor.byte_offset() != plan.byte_count()
        || receipt.bundle_digest() != hash(&expected.stream)
    {
        return Err("fresh output resume duplicated/omitted canonical bytes".into());
    }
    Ok(())
}
fn cancelled_export(
    fixture: &Fixture,
    plan: &BundlePlan,
    expected: &ExpectedBundle,
) -> Result<(), String> {
    let live = EvidenceCancellation::new();
    let mut owner = BundleExportOperation::prepare(
        plan,
        fixture.artifact_store()?,
        BundleLimits::default(),
        &live,
    )
    .map_err(error)?;
    let mut output = Vec::new();
    owner.advance(&mut output, nonzero(17)?, &live).map_err(error)?;
    let before = owner.cursor();
    let accepted = output.len();
    if accepted == 0
        || accepted >= expected.stream.len()
        || output != expected.stream[..accepted]
        || before.byte_offset() != u64::try_from(accepted).map_err(error)?
    {
        return Err("export cancellation fixture did not reach a real exact partial prefix".into());
    }
    let cancelled = EvidenceCancellation::new();
    cancelled.cancel();
    require(owner.advance(&mut output, nonzero(31)?, &cancelled), EvidenceErrorKind::Cancelled)?;
    if owner.cursor() != before
        || output.len() != accepted
        || output != expected.stream[..accepted]
        || before.byte_offset() != u64::try_from(accepted).map_err(error)?
    {
        return Err("export cancellation lost/advanced accepted progress".into());
    }
    drop(owner);
    let mut owner = BundleExportOperation::restore(
        plan,
        fixture.artifact_store()?,
        BundleLimits::default(),
        before,
        &live,
    )
    .map_err(error)?;
    owner.resume(&mut output, &live).map_err(error)?;
    if output != expected.stream || owner.receipt().is_none() {
        return Err("restored cancelled output differs from canonical stream".into());
    }
    Ok(())
}
fn verify_stream(expected: &ExpectedBundle) -> Result<(), String> {
    let verified =
        verify_bundle(Cursor::new(&expected.stream), BundleLimits::default()).map_err(error)?;
    if verified.manifest().canonical_bytes() != expected.manifest
        || verified.bundle_digest() != hash(&expected.stream)
        || verified.byte_count() != u64::try_from(expected.stream.len()).map_err(error)?
    {
        return Err("offline verification differs from independent expected bytes".into());
    }
    let mut framed = expected.stream.clone();
    framed.extend(b"NEXT");
    let mut owner = BundleVerificationOperation::new(Cursor::new(framed), BundleLimits::default());
    let live = EvidenceCancellation::new();
    owner.advance(nonzero(19)?, &live).map_err(error)?;
    let before = owner.cursor();
    if before.input_bytes() == 0
        || before.input_bytes() >= u64::try_from(expected.stream.len()).map_err(error)?
    {
        return Err("verification cancellation fixture did not reach a real partial input".into());
    }
    let cancelled = EvidenceCancellation::new();
    cancelled.cancel();
    require(owner.advance(nonzero(31)?, &cancelled).map(|_| ()), EvidenceErrorKind::Cancelled)?;
    if owner.cursor() != before {
        return Err("verification cancellation lost input progress".into());
    }
    let result = owner.resume(&live).map_err(error)?;
    if result != verified
        || owner.cursor().input_bytes() != u64::try_from(expected.stream.len()).map_err(error)?
    {
        return Err("framed verifier consumed wrong boundary or changed digest".into());
    }
    let input = owner.into_input();
    let offset = usize::try_from(input.position()).map_err(error)?;
    if &input.get_ref()[offset..] != b"NEXT" {
        return Err("framed verification consumed following transport frame".into());
    }
    Ok(())
}
fn malformed(expected: &ExpectedBundle) -> Result<(), String> {
    let mut flipped = expected.stream.clone();
    let index = flipped.len() / 2;
    flipped[index] ^= 1;
    let mut trailing = expected.stream.clone();
    trailing.push(0);
    let mut wrong_magic = expected.stream.clone();
    wrong_magic[0] ^= 1;
    for bytes in [
        &expected.stream[..expected.stream.len() - 1],
        flipped.as_slice(),
        trailing.as_slice(),
        wrong_magic.as_slice(),
    ] {
        require(
            verify_bundle(Cursor::new(bytes), BundleLimits::default()),
            EvidenceErrorKind::InvalidBundle,
        )?;
    }
    Ok(())
}
fn run_bundle(
    fixture: &mut Fixture,
    plan: &BundlePlan,
    expected: &ExpectedBundle,
    mode: BundleMode,
) -> Result<(), String> {
    if matches!(mode, BundleMode::CorruptSource) {
        let slot = *expected.slots.first().ok_or("empty closure")?;
        let path = fixture.object(slot)?;
        let mut corrupt = content(fixture.salt, slot);
        corrupt[0] ^= 1;
        fs::write(&path, corrupt).map_err(error)?;
        let mut output = vec![];
        let result =
            assemble_bundle(plan, fixture.artifact_store()?, &mut output, BundleLimits::default());
        fs::write(path, content(fixture.salt, slot)).map_err(error)?;
        require(result, EvidenceErrorKind::CorruptArtifact)?;
        if !output.is_empty() {
            return Err(
                "corrupt artifact touched public bundle output before authentication".into()
            );
        }
        bump(&mut fixture.metrics, "corrupt_source_preflight_rejections");
    }
    let mut output = Vec::new();
    let receipt =
        assemble_bundle(plan, fixture.artifact_store()?, &mut output, BundleLimits::default())
            .map_err(error)?;
    if output != expected.stream
        || receipt.byte_count() != u64::try_from(output.len()).map_err(error)?
        || receipt.bundle_digest() != hash(&expected.stream)
        || receipt.root_digest() != expected.root
    {
        return Err("assembled bundle differs from complete independent canonical bytes".into());
    }
    match mode {
        BundleMode::ShortWrite => {
            short_write(fixture, plan, expected)?;
            bump(&mut fixture.metrics, "partial_output_exact_cursor_resumes");
        }
        BundleMode::Cancel => {
            cancelled_export(fixture, plan, expected)?;
            bump(&mut fixture.metrics, "cancelled_export_exact_restore_resumes");
        }
        BundleMode::Malformed => {
            malformed(expected)?;
            *fixture.metrics.entry("malformed_bundle_rejections".into()).or_default() += 4;
        }
        BundleMode::Normal | BundleMode::CorruptSource => {}
    }
    verify_stream(expected)?;
    bump(&mut fixture.metrics, "offline_and_framed_verifications");
    bump(&mut fixture.metrics, "cancelled_verification_resumes");
    Ok(())
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.steps.len() > 48 {
        return Err("setup-incomplete: evidence history exceeds local bound".into());
    }
    let mut fixture = Fixture::new(case.salt)?;
    for (index, step) in case.steps.iter().enumerate() {
        let result = match *step {
            Op::Admit { slot, parents } => {
                fixture.admit(Record { slot: slot % 4, parents: parents & 15, payload: 0 })
            }
            Op::SeedGraph => fixture.seed(),
            Op::Retry { slot, conflict, broken_dependencies } => {
                fixture.retry(slot % 4, conflict, broken_dependencies)
            }
            Op::BadBinding { slot, defect } => fixture.bad_binding(slot % 4, defect % 4),
            Op::Freshness { slot, axis } => fixture.freshness(slot % 4, axis % 8),
            Op::Invalidate { slot, defect } => fixture.invalidate(slot % 4, defect % 4),
            Op::Bundle { authority, axis, mode } => fixture.bundle(authority & 15, axis % 8, mode),
            Op::Reopen => fixture.reopen(),
        };
        result.map_err(|failure| format!("{failure}; step {index} {step:?}"))?;
        fixture.verify()?;
        bump(&mut fixture.metrics, "operations");
    }
    Ok(fixture.metrics)
}
pub fn calibration() -> Case {
    Case { salt: 7, steps: vec![Op::SeedGraph, Op::Freshness { slot: 3, axis: 7 }] }
}
pub fn examples() -> Vec<Case> {
    let mut examples = vec![
        calibration(),
        Case {
            salt: 1,
            steps: vec![
                Op::Admit { slot: 1, parents: 1 },
                Op::Admit { slot: 1, parents: 0 },
                Op::Admit { slot: 0, parents: 2 },
                Op::Admit { slot: 0, parents: 1 },
                Op::SeedGraph,
                Op::Retry { slot: 3, conflict: true, broken_dependencies: true },
                Op::Retry { slot: 3, conflict: false, broken_dependencies: true },
                Op::Reopen,
            ],
        },
    ];
    for mode in [
        BundleMode::Normal,
        BundleMode::ShortWrite,
        BundleMode::Cancel,
        BundleMode::CorruptSource,
        BundleMode::Malformed,
    ] {
        examples.push(Case {
            salt: 17,
            steps: vec![
                Op::SeedGraph,
                Op::Bundle { authority: 8, axis: 0, mode },
                Op::Invalidate { slot: 0, defect: 0 },
                Op::Bundle { authority: 8, axis: 0, mode: BundleMode::Normal },
                Op::Reopen,
            ],
        });
    }
    examples.push(Case {
        salt: 31,
        steps: (0_u8..8)
            .map(|axis| Op::Freshness { slot: 3, axis })
            .chain((0_u8..4).map(|defect| Op::BadBinding { slot: 1, defect }))
            .chain((1_u8..4).map(|defect| Op::Invalidate { slot: 2, defect }))
            .collect(),
    });
    examples
}
