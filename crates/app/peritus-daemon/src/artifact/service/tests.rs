use super::*;
use peritus_app_protocol::{ArtifactOpenRequest, ConversationId, WorkbenchQuery};
use peritus_artifact_store::StoreConfig;
use peritus_journal::{SqliteJournalOptions, StoreId};
use peritus_types::{ArtifactId, WorkspaceId};

const CONTENT: &[u8] = b"exact private imported bytes";

#[test]
fn cancelling_owned_imports_releases_capacity_and_rejects_foreign_cancellation() {
    let directory = tempfile::tempdir().unwrap();
    let (mut journal, mut authority) = open(directory.path());
    for id in 10..20 {
        let metadata = metadata(id, id);
        authority
            .begin_scoped_upload(
                &mut journal,
                owner(),
                session(),
                metadata.clone(),
                64,
                scope(1, 5, 6),
            )
            .unwrap();
        let cancel = ArtifactCancellation::new(
            metadata.transfer_id(),
            metadata.artifact_id(),
            peritus_app_protocol::CorrelationId::new([id; 16]).unwrap(),
        );
        assert!(authority.cancel(ActorId::new([9; 16]).unwrap(), session(), cancel).is_err());
        assert!(authority.cancel(owner(), SessionId::new([9; 16]).unwrap(), cancel).is_err());
        assert_eq!(authority.transfers.len(), 1);
        authority.cancel(owner(), session(), cancel).unwrap();
        assert!(authority.transfers.is_empty());
        assert!(
            authority.read_scoped(&journal, scope(1, 5, 6), metadata.artifact_id(), 1024).is_err()
        );
    }
}

fn owner() -> ActorId {
    ActorId::new([1; 16]).expect("actor")
}
fn session() -> SessionId {
    SessionId::new([2; 16]).expect("session")
}
fn scope(actor: u8, conversation: u8, workspace: u8) -> ArtifactScope {
    ArtifactScope::new(
        ActorId::new([actor; 16]).expect("actor"),
        WorkbenchQuery::new(
            ConversationId::new([conversation; 16]).expect("conversation"),
            WorkspaceId::new([workspace; 16]).expect("workspace"),
        ),
    )
}
fn metadata(id: u8, transfer: u8) -> ArtifactMetadata {
    ArtifactMetadata::new(
        TransferId::new([transfer; 16]).expect("transfer"),
        ArtifactId::new([id; 16]).expect("artifact"),
        CONTENT.len() as u64,
        CanonicalMediaType::new("application/octet-stream".to_owned(), 255).expect("media"),
        peritus_codec::sha256(CONTENT),
        64,
        64,
    )
    .expect("metadata")
}
fn open(root: &std::path::Path) -> (SqliteJournal, ArtifactAuthority) {
    let journal = SqliteJournal::open(
        root.join("journal.sqlite3"),
        StoreId::new([7; 16]).expect("store ID"),
        SqliteJournalOptions::default(),
    )
    .expect("journal");
    let store =
        ArtifactStore::open(StoreConfig::new(root.join("artifacts"), 1024, 8192).expect("config"))
            .expect("store");
    (journal, ArtifactAuthority::new(store, 1024, 4).expect("authority"))
}
fn finish(
    authority: &mut ArtifactAuthority,
    journal: &mut SqliteJournal,
    metadata: &ArtifactMetadata,
) {
    authority
        .upload_chunk(
            owner(),
            session(),
            &ArtifactChunk::new(
                metadata.transfer_id(),
                metadata.artifact_id(),
                0,
                0,
                CONTENT.to_vec(),
                64,
            )
            .expect("chunk"),
        )
        .expect("chunk accepted");
    authority
        .complete_upload(
            journal,
            owner(),
            session(),
            ArtifactCompletion::new(
                metadata.transfer_id(),
                metadata.artifact_id(),
                metadata.byte_size(),
                metadata.digest(),
            ),
        )
        .expect("complete");
}

#[test]
fn scoped_upload_bytes_and_owner_conversation_workspace_binding_survive_restart() {
    let root = tempfile::tempdir().expect("root");
    let (mut journal, mut authority) = open(root.path());
    let metadata = metadata(3, 4);
    let selected = scope(1, 5, 6);
    authority
        .begin_scoped_upload(&mut journal, owner(), session(), metadata.clone(), 64, selected)
        .expect("begin");
    assert!(
        authority.read_scoped(&journal, selected, metadata.artifact_id(), 1024).is_err(),
        "pending bytes are not accepted"
    );
    finish(&mut authority, &mut journal, &metadata);
    let (_, bytes) =
        authority.read_scoped(&journal, selected, metadata.artifact_id(), 1024).expect("read");
    assert_eq!(bytes, CONTENT);
    drop(authority);
    drop(journal);
    let (journal, mut authority) = open(root.path());
    let (_, bytes) = authority
        .read_scoped(&journal, selected, metadata.artifact_id(), CONTENT.len() as u64)
        .expect("reopened exact read");
    assert_eq!(bytes, CONTENT);
    assert!(
        authority
            .read_scoped(&journal, selected, metadata.artifact_id(), CONTENT.len() as u64 - 1)
            .is_err()
    );
    for wrong in [scope(9, 5, 6), scope(1, 9, 6), scope(1, 5, 9)] {
        let error = authority
            .read_scoped(&journal, wrong, metadata.artifact_id(), 1024)
            .expect_err("scope rejection");
        assert_eq!(error.code_kind(), crate::DaemonErrorCode::Unauthorized);
    }
    let open = ArtifactOpenRequest::new(
        TransferId::new([10; 16]).expect("download"),
        metadata.artifact_id(),
    );
    assert!(
        authority
            .open_download(
                &journal,
                ActorId::new([9; 16]).expect("foreign actor"),
                session(),
                open.transfer_id(),
                open.artifact_id(),
                64
            )
            .is_err(),
        "legacy download cannot bypass actor scope"
    );
    assert!(
        authority
            .open_download(&journal, owner(), session(), open.transfer_id(), open.artifact_id(), 64)
            .is_ok()
    );
}

#[test]
fn interrupted_claim_cannot_be_adopted_by_legacy_or_foreign_upload_after_restart() {
    let root = tempfile::tempdir().expect("root");
    let (mut journal, mut authority) = open(root.path());
    let metadata = metadata(3, 4);
    let selected = scope(1, 5, 6);
    authority
        .begin_scoped_upload(&mut journal, owner(), session(), metadata.clone(), 64, selected)
        .expect("begin");
    drop(authority);
    drop(journal);
    let (mut journal, mut authority) = open(root.path());
    assert!(
        authority.begin_upload(&mut journal, owner(), session(), metadata.clone(), 64).is_err()
    );
    assert!(
        authority
            .begin_scoped_upload(
                &mut journal,
                owner(),
                session(),
                metadata.clone(),
                64,
                scope(1, 9, 6)
            )
            .is_err()
    );
    assert!(
        authority
            .begin_scoped_upload(
                &mut journal,
                ActorId::new([9; 16]).expect("actor"),
                session(),
                metadata.clone(),
                64,
                scope(9, 5, 6)
            )
            .is_err()
    );
    authority
        .begin_scoped_upload(&mut journal, owner(), session(), metadata.clone(), 64, selected)
        .expect("same scoped retry");
    finish(&mut authority, &mut journal, &metadata);
    assert_eq!(
        authority.read_scoped(&journal, selected, metadata.artifact_id(), 1024).expect("read").1,
        CONTENT
    );
}

#[test]
fn repeating_a_scoped_upload_keeps_the_original_publication_and_exact_bytes() {
    let root = tempfile::tempdir().expect("root");
    let selected = scope(1, 5, 6);
    let mut original = None;
    for transfer in [4, 8] {
        let (mut journal, mut authority) = open(root.path());
        let metadata = metadata(3, transfer);
        authority
            .begin_scoped_upload(&mut journal, owner(), session(), metadata.clone(), 64, selected)
            .expect("repeatable begin");
        finish(&mut authority, &mut journal, &metadata);
        let entry = journal.application_artifact(metadata.artifact_id()).unwrap().unwrap();
        if let Some(original) = original.as_ref() {
            assert_eq!(original, &entry);
        } else {
            original = Some(entry);
        }
        assert_eq!(
            authority.read_scoped(&journal, selected, metadata.artifact_id(), 1024).unwrap().1,
            CONTENT
        );
    }
}

#[test]
fn identical_bytes_can_be_imported_as_independent_scoped_artifacts() {
    let root = tempfile::tempdir().expect("root");
    for id in [3, 8] {
        let (mut journal, mut authority) = open(root.path());
        let selected = scope(1, id, 6);
        let base = metadata(id, id);
        let metadata = ArtifactMetadata::new(
            base.transfer_id(),
            base.artifact_id(),
            base.byte_size(),
            CanonicalMediaType::new(
                if id == 3 { "application/octet-stream" } else { "text/plain" }.to_owned(),
                255,
            )
            .unwrap(),
            base.digest(),
            64,
            64,
        )
        .unwrap();
        authority
            .begin_scoped_upload(&mut journal, owner(), session(), metadata.clone(), 64, selected)
            .expect("independent begin");
        finish(&mut authority, &mut journal, &metadata);
        assert_eq!(
            authority.read_scoped(&journal, selected, metadata.artifact_id(), 1024).unwrap().1,
            CONTENT
        );
    }
}

#[test]
fn legacy_artifact_cannot_be_promoted_to_a_scoped_import_by_identifier() {
    let root = tempfile::tempdir().expect("root");
    let (mut journal, mut authority) = open(root.path());
    let metadata = metadata(3, 4);
    authority
        .begin_upload(&mut journal, owner(), session(), metadata.clone(), 64)
        .expect("legacy upload");
    finish(&mut authority, &mut journal, &metadata);
    assert!(authority.read_scoped(&journal, scope(1, 5, 6), metadata.artifact_id(), 1024).is_err());
    assert!(
        authority
            .begin_scoped_upload(&mut journal, owner(), session(), metadata, 64, scope(1, 5, 6))
            .is_err()
    );
}

#[test]
fn foreign_completion_and_duplicate_object_upload_do_not_destroy_the_owned_transfer() {
    let root = tempfile::tempdir().expect("root");
    let (mut journal, mut authority) = open(root.path());
    let metadata = metadata(3, 4);
    authority
        .begin_scoped_upload(&mut journal, owner(), session(), metadata.clone(), 64, scope(1, 5, 6))
        .expect("begin");
    let completion = ArtifactCompletion::new(
        metadata.transfer_id(),
        metadata.artifact_id(),
        metadata.byte_size(),
        metadata.digest(),
    );
    assert!(
        authority
            .complete_upload(
                &mut journal,
                ActorId::new([9; 16]).expect("foreign actor"),
                session(),
                completion
            )
            .is_err()
    );
    assert!(
        authority
            .complete_upload(
                &mut journal,
                owner(),
                SessionId::new([9; 16]).expect("foreign session"),
                completion
            )
            .is_err()
    );
    let duplicate = self::metadata(3, 8);
    assert!(
        authority
            .begin_scoped_upload(&mut journal, owner(), session(), duplicate, 64, scope(1, 5, 6))
            .is_err()
    );
    finish(&mut authority, &mut journal, &metadata);
    assert_eq!(
        authority
            .read_scoped(&journal, scope(1, 5, 6), metadata.artifact_id(), 1024)
            .expect("read")
            .1,
        CONTENT
    );
}
