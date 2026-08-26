//! C0 atomic checkpoint, artifact dependency, idempotency, and restart coverage.

mod support;

use std::time::Duration;

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_eval::{
    EvaluationCommand, EvaluationCommandKind, EvaluationPhase, commit_evaluation_transition,
    decide, evaluation_aggregate_key, load_evaluation_replay,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_types::{CommandId, EventId};

use support::{bytes, campaign_id, digest, frozen_profile, revision};

#[test]
fn genesis_commit_is_idempotent_and_restart_replay_matches_checkpoint() {
    let stores = Stores::open();
    let Stores { _temporary: temporary, mut journal, artifacts } = stores;
    let dataset_artifact = finalize(&artifacts, b"dataset", 110, 111);
    let profile_artifact = finalize(&artifacts, b"profile", 112, 113);
    let profile = frozen_profile();
    let command = EvaluationCommand::new(
        CommandId::new(bytes(114)).expect("command"),
        EventId::new(bytes(115)).expect("event"),
        campaign_id(),
        0,
        None,
        digest(0),
        profile.digest(),
        EvaluationCommandKind::CreateCampaign {
            revision: revision(),
            dataset_digest: profile.dataset().digest(),
            dataset_artifact,
            profile_artifact,
        },
    )
    .expect("command");
    let transition = decide(None, &command).expect("transition");
    let first = commit_evaluation_transition(&mut journal, &command, &transition).expect("commit");
    let retry = commit_evaluation_transition(&mut journal, &command, &transition).expect("retry");
    assert_eq!(first.batch_hash(), retry.batch_hash());
    assert_eq!(first.first_position(), retry.first_position());
    drop(journal);

    let journal = SqliteJournal::open(
        temporary.path().join("shared.sqlite3"),
        StoreId::new(bytes(120)).expect("store"),
        SqliteJournalOptions { busy_timeout: Duration::from_millis(500) },
    )
    .expect("reopen journal");
    let replay = load_evaluation_replay(&journal, campaign_id()).expect("load replay");
    let rebuilt = replay.rebuild().expect("rebuild").expect("state");
    assert_eq!(rebuilt, transition.state().clone());
    assert_eq!(rebuilt.phase(), EvaluationPhase::Created);
    assert_eq!(
        evaluation_aggregate_key(campaign_id()).expect("aggregate").kind(),
        peritus_journal::AggregateKind::Evaluation,
    );
}

struct Stores {
    _temporary: tempfile::TempDir,
    journal: SqliteJournal,
    artifacts: ArtifactStore,
}

impl Stores {
    fn open() -> Self {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let database = temporary.path().join("shared.sqlite3");
        let journal = SqliteJournal::open(
            &database,
            StoreId::new(bytes(120)).expect("store"),
            SqliteJournalOptions { busy_timeout: Duration::from_millis(500) },
        )
        .expect("journal");
        let artifacts = ArtifactStore::open(
            StoreConfig::new(temporary.path().join("artifacts"), 1_048_576, 8 * 1_048_576)
                .expect("config")
                .with_database_path(database)
                .expect("shared database"),
        )
        .expect("artifact store");
        Self { _temporary: temporary, journal, artifacts }
    }
}

fn finalize(store: &ArtifactStore, bytes: &[u8], event_seed: u8, media_seed: u8) -> ArtifactDigest {
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
    let size = u64::try_from(bytes.len()).expect("size");
    let request = WriteRequest::new(
        digest,
        size,
        size,
        MediaType::new(format!("application/x-peritus-eval-{media_seed}")).expect("media type"),
        EncryptionMetadata::unencrypted(),
        EventId::new(bytes::bytes(event_seed)).expect("creating event"),
    );
    let mut writer = store.begin_write(request).expect("writer");
    writer.write_chunk(bytes).expect("write");
    writer.finalize().expect("finalize").digest()
}

mod bytes {
    pub const fn bytes(value: u8) -> [u8; 16] {
        [value; 16]
    }
}
