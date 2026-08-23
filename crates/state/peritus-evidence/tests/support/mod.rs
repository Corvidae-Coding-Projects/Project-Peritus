#![allow(dead_code, reason = "fixtures are shared across focused integration suites")]

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_codec::{CodecLimits, encode_message};
use peritus_evidence::{
    EvidenceDraft, EvidenceId, EvidenceKind, EvidenceSource, EvidenceStore, EvidenceStoreOptions,
    revision_digest,
};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, ArtifactDependency, EventDraft,
    ExactFrame, HeadExpectation, IntegrityExport, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_kernel::SessionPhase;
use peritus_protocol::LifecyclePhaseDto;
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, EventSequence, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct Fixture {
    pub temp: TempDir,
    pub path: PathBuf,
    pub journal: SqliteJournal,
    pub artifacts: ArtifactStore,
    next: u8,
}

impl Fixture {
    pub fn new() -> Self {
        let temp = TempDir::new().expect("temporary evidence directory");
        let path = temp.path().join("shared.sqlite3");
        let journal = SqliteJournal::open(&path, store_id(), SqliteJournalOptions::default())
            .expect("journal opens");
        let config = StoreConfig::new(temp.path().join("artifacts"), 1024 * 1024, 8 * 1024 * 1024)
            .expect("artifact config")
            .with_database_path(&path)
            .expect("shared database path");
        let artifacts = ArtifactStore::open(config).expect("artifact store opens");
        Self { temp, path, journal, artifacts, next: 20 }
    }

    pub fn evidence_store(&self) -> EvidenceStore {
        EvidenceStore::open(&self.path, EvidenceStoreOptions::default())
            .expect("evidence catalog opens")
    }

    pub fn finalize(&self, bytes: &[u8]) -> ArtifactDigest {
        let digest = ArtifactDigest::new(Sha256::digest(bytes).into());
        let request = WriteRequest::new(
            digest,
            u64::try_from(bytes.len()).expect("fixture artifact size"),
            1024 * 1024,
            MediaType::new("application/octet-stream").expect("media type"),
            EncryptionMetadata::unencrypted(),
            event_id(10),
        );
        let mut writer = self.artifacts.begin_write(request).expect("begin artifact write");
        writer.write_chunk(bytes).expect("write artifact");
        writer.finalize().expect("finalize artifact");
        digest
    }

    pub fn append(&mut self, revision: &RevisionTuple, artifact: Option<ArtifactDigest>) -> u64 {
        let aggregate = aggregate_key();
        let head = self.journal.head(aggregate).expect("read aggregate head");
        let sequence = head.map_or(1, |value| value.sequence().get() + 1);
        let event = event_id(self.next);
        let command = command_id(self.next);
        self.next = self.next.checked_add(1).expect("fixture identity space");
        let frame = encode_message(
            &LifecyclePhaseDto::Session(SessionPhase::Open),
            CodecLimits::PRODUCTION,
        )
        .expect("canonical lifecycle frame");
        let draft = EventDraft::new(
            aggregate,
            EventSequence::new(sequence).expect("event sequence"),
            event,
            head.map(peritus_journal::AggregateHead::event_id),
            ExactFrame::new(frame).expect("exact frame"),
            revision_digest(revision),
            Vec::new(),
        )
        .expect("event draft");
        let expected = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
        let dependencies = artifact
            .map(|digest| vec![ArtifactDependency::new(digest.sha256())])
            .unwrap_or_default();
        let plan = AppendRequest::new(
            store_id(),
            command,
            Sha256Digest::new([self.next; 32]),
            vec![expected],
            vec![draft],
            Vec::new(),
            dependencies,
            None,
            None,
            Vec::new(),
        )
        .plan()
        .expect("append plan");
        self.journal.append(plan).expect("journal append").first_position()
    }

    pub fn export(&mut self) -> IntegrityExport {
        self.journal.integrity_export().expect("journal integrity export")
    }

    pub fn draft(
        byte: u8,
        revision: RevisionTuple,
        position: u64,
        artifacts: Vec<ArtifactDigest>,
        causes: Vec<EvidenceId>,
    ) -> EvidenceDraft {
        EvidenceDraft::new(
            evidence_id(byte),
            EvidenceKind::new("execution-result").expect("kind"),
            EvidenceSource::new("local-runner").expect("source"),
            revision,
            position,
            Sha256Digest::new([byte; 32]),
            artifacts,
            causes,
        )
        .expect("evidence draft")
    }

    pub fn object_path(&self, digest: ArtifactDigest) -> PathBuf {
        let hex = digest.to_hex();
        self.artifacts.root().join("objects").join("sha256").join(&hex[..2]).join(hex)
    }
}

pub fn revision() -> RevisionTuple {
    make_revision([2, 3, 4, 1, 1, 5, 6])
}

pub fn make_revision(parts: [u8; 7]) -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([parts[0]; 16]).expect("acceptance id"),
        HarnessId::new([parts[1]; 16]).expect("harness id"),
        WorkspaceId::new([parts[2]; 16]).expect("workspace id"),
        Generation::new(u64::from(parts[3])).expect("generation"),
        RevisionNumber::new(u64::from(parts[4])).expect("revision"),
        PolicyId::new([parts[5]; 16]).expect("policy id"),
        ProviderProfileId::new([parts[6]; 16]).expect("provider profile id"),
    )
}

pub fn evidence_id(byte: u8) -> EvidenceId {
    EvidenceId::new([byte; 16]).expect("evidence id")
}

pub fn event_id(byte: u8) -> EventId {
    EventId::new([byte; 16]).expect("event id")
}

fn command_id(byte: u8) -> CommandId {
    CommandId::new([byte; 16]).expect("command id")
}

fn store_id() -> StoreId {
    StoreId::new([1; 16]).expect("store id")
}

fn aggregate_key() -> AggregateKey {
    AggregateKey::new(AggregateKind::Kernel, AggregateId::new([9; 16]).expect("aggregate id"))
}

pub fn open_evidence(path: &Path) -> EvidenceStore {
    EvidenceStore::open(path, EvidenceStoreOptions::default()).expect("reopen evidence catalog")
}
