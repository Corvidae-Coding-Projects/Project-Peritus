//! Compatibility and integrity fixtures for immutable checkpoint manifest publication.

use super::*;
use peritus_codec::sha256;
use peritus_journal::StoreId;
use peritus_product_runner::control::{
    CheckpointPath, CheckpointReferences, ControlText, ConversationId, OperationId,
};
use peritus_types::{ActorId, WorkspaceId};
use std::io::Write as _;
use std::{fs, path::Path};

fn operation(id: u8, revision: u64, intent: ControlIntent) -> ControlOperation {
    ControlOperation::new(
        OperationId::new([id; 16]).unwrap(),
        ConversationId::new([2; 16]).unwrap(),
        ActorId::new([3; 16]).unwrap(),
        WorkspaceId::new([4; 16]).unwrap(),
        revision,
        intent,
    )
}

fn fixture(path: &Path) -> (ControlStore, UserCheckpoint, ControlOperation) {
    let mut store = ControlStore::open(path, StoreId::new([1; 16]).unwrap()).unwrap();
    store
        .accept(&operation(
            1,
            0,
            ControlIntent::CreateConversation {
                title: ControlText::new("retained".to_owned()).unwrap(),
            },
        ))
        .unwrap();
    let version = CheckpointFileVersion::present(
        sha256(b"retained"),
        8,
        peritus_product_runner::control::CheckpointFileMode::Regular,
    );
    let checkpoint = UserCheckpoint::new(
        CheckpointId::new([5; 16]).unwrap(),
        "original".to_owned(),
        CheckpointReferences::new(1, 0, 0, None),
        vec![CheckpointPath::new("note".to_owned(), version).unwrap()],
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let change = operation(5, 1, ControlIntent::CreateCheckpoint(checkpoint.clone()));
    (store, checkpoint, change)
}

#[test]
fn legacy_schema1_body_roots_reopen_restore_exact_bytes_and_original_receipt() {
    let root = tempfile::tempdir().unwrap();
    let (mut store, checkpoint, change) = fixture(root.path());
    let mut pending = PendingPublication::new(
        store.checkpoint_config.root(),
        SNAPSHOT_NAMESPACE,
        checkpoint.id().as_bytes(),
    )
    .unwrap();
    let body = store
        .publish_stream(
            &change,
            reference_owner(SNAPSHOT_NAMESPACE, checkpoint.id().as_bytes()),
            &mut pending,
            &mut io::Cursor::new(b"retained"),
            checkpoint.paths()[0].checkpoint(),
        )
        .unwrap();
    let receipt = store
        .accept_installs(
            &change,
            vec![
                StateInstall::new(
                    SNAPSHOT_NAMESPACE,
                    checkpoint.id().as_bytes().to_vec(),
                    None,
                    1,
                    serde_json::to_vec(&BodyRoots { schema: 1, bodies: vec![Some(body)] }).unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap();
    pending.finish().unwrap();
    drop(store);
    let mut store = ControlStore::open(root.path(), StoreId::new([1; 16]).unwrap()).unwrap();
    assert_eq!(store.accept(&change).unwrap(), receipt);
    assert_eq!(
        store.load_checkpoint(change.conversation(), checkpoint.id()).unwrap(),
        Some(checkpoint.clone())
    );
    let snapshots = store.checkpoint_snapshots(checkpoint.id()).unwrap();
    let snapshot = snapshots.snapshot(0, checkpoint.paths()[0].checkpoint()).unwrap().unwrap();
    let patch = peritus_patch::PatchSet::from_snapshot(
        WorkspaceId::new([4; 16]).unwrap(),
        peritus_types::Generation::first(),
        peritus_types::RevisionNumber::first(),
        vec![peritus_patch::PatchOperation::create_snapshot(
            peritus_patch::WorkspacePath::new("note").unwrap(),
            snapshot,
        )],
    )
    .unwrap();
    let plan = patch
        .plan(
            WorkspaceId::new([4; 16]).unwrap(),
            peritus_types::Generation::first(),
            peritus_types::RevisionNumber::first(),
        )
        .unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let transactions = tempfile::tempdir().unwrap();
    peritus_patch::apply_patch(workspace.path(), transactions.path(), &plan).unwrap();
    assert_eq!(fs::read(workspace.path().join("note")).unwrap(), b"retained");
}

#[test]
fn manifest_pages_reject_wrong_path_identity_presence_count_and_digest() {
    let root = tempfile::tempdir().unwrap();
    let (mut store, checkpoint, change) = fixture(root.path());
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"retained").unwrap();
    store.accept_checkpoint_snapshots(&change, &[Some(file.into_temp_path())]).unwrap();
    let published = store
        .journal
        .state_record(SNAPSHOT_NAMESPACE, checkpoint.id().as_bytes())
        .unwrap()
        .unwrap();
    let root: serde_json::Value = serde_json::from_slice(published.bytes()).unwrap();
    let stream: EvidenceRoot = serde_json::from_value(root["manifest"].clone()).unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(&store.manifest_bytes(&stream).unwrap()).unwrap();
    for missing in [false, true] {
        let mut manifest = original.clone();
        if missing {
            manifest["bodies"][0]["root"] = serde_json::Value::Null;
        } else {
            let first = manifest["bodies"][0]["path_id"][0].as_u64().unwrap();
            manifest["bodies"][0]["path_id"][0] = serde_json::json!(first ^ 1);
        }
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let id = [if missing { 8 } else { 9 }; 16];
        let mut pending =
            PendingPublication::new(store.checkpoint_config.root(), SNAPSHOT_NAMESPACE, &id)
                .unwrap();
        let digest = sha256(&bytes);
        let version = CheckpointFileVersion::present(
            digest,
            bytes.len() as u64,
            peritus_product_runner::control::CheckpointFileMode::Regular,
        );
        let body = store
            .publish_stream(
                &change,
                reference_owner(SNAPSHOT_NAMESPACE, &id),
                &mut pending,
                &mut io::Cursor::new(&bytes),
                version,
            )
            .unwrap();
        let mut malformed = root.clone();
        malformed["manifest"] = serde_json::to_value(EvidenceRoot {
            schema: 1,
            root: body,
            digest: digest.into_bytes(),
            bytes: bytes.len() as u64,
        })
        .unwrap();
        assert!(matches!(
            store.decode_body_roots(checkpoint.id(), &serde_json::to_vec(&malformed).unwrap()),
            Err(Error::Corrupt(_))
        ));
        drop(pending);
        store.recover_snapshot_publications().unwrap();
    }
    for field in ["entries", "digest"] {
        let mut malformed = root.clone();
        if field == "entries" {
            malformed["entries"] = serde_json::json!(2);
        } else {
            let first = malformed["manifest"]["digest"][0].as_u64().unwrap();
            malformed["manifest"]["digest"][0] = serde_json::json!(first ^ 1);
        }
        assert!(matches!(
            store.decode_body_roots(checkpoint.id(), &serde_json::to_vec(&malformed).unwrap()),
            Err(Error::Corrupt(_))
        ));
    }
    assert_eq!(
        store.load_checkpoint(change.conversation(), checkpoint.id()).unwrap(),
        Some(checkpoint)
    );
}
