//! Native checkpoint publication past narrow counts and an inline manifest's former size.

use super::*;

#[test]
fn paged_manifest_over_65535_paths_and_16_mib_reopens_with_its_original_receipt() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("conversation");
    let paths = (0..=u16::MAX)
        .map(|index| {
            CheckpointPath::new(
                format!("files/{index:05}-{}.txt", "x".repeat(192)),
                CheckpointFileVersion::Absent,
            )
            .expect("path")
        })
        .collect();
    let checkpoint = UserCheckpoint::new(
        CheckpointId::new(
            *operation(2, 1, ControlIntent::PinConversation { pinned: true }).id().as_bytes(),
        )
        .unwrap(),
        "complete manifest".to_owned(),
        CheckpointReferences::new(1, 0, 0, None),
        paths,
        Vec::new(),
        Vec::new(),
    )
    .expect("more than65535 paths");
    let change = operation(2, 1, ControlIntent::CreateCheckpoint(checkpoint.clone()));
    assert!(change.canonical_bytes().unwrap().len() > peritus_journal::MAX_STATE_BYTES);
    let bodies = (0..checkpoint.paths().len()).map(|_| None).collect::<Vec<_>>();
    let receipt =
        journal.accept_checkpoint_snapshots(&change, &bodies).expect("publish complete manifest");
    let row = journal.journal.state_record(3482, checkpoint.id().as_bytes()).unwrap().unwrap();
    let published: serde_json::Value = serde_json::from_slice(row.bytes()).unwrap();
    assert_eq!(published["schema"], 2);
    assert_eq!(published["entries"], 65_536);
    assert!(
        published["manifest"]["bytes"].as_u64().unwrap() > peritus_journal::MAX_STATE_BYTES as u64
    );
    assert!(row.bytes().len() < 1024, "the journal stores an exact root rather than all entries");
    drop(journal);
    let mut journal = store(root.path());
    let recovered =
        journal.load_checkpoint(create().conversation(), checkpoint.id()).unwrap().unwrap();
    assert_eq!(recovered, checkpoint);
    assert_eq!(
        journal
            .accept_checkpoint_snapshots(&change, &[])
            .expect("original receipt without recapture"),
        receipt
    );
    assert_eq!(journal.operation(create().conversation(), change.id()).unwrap(), Some(change));
    let current = journal.load(create().conversation()).unwrap().unwrap();
    assert_eq!(current.revision(), 2);
    assert_eq!(current.checkpoints(), &[checkpoint]);
}

#[test]
fn checkpoint_control_page_faults_preserve_predecessor_and_original_receipt() {
    use crate::product_control::storage::checkpoints::snapshot::{
        SnapshotFaultPoint, inject_snapshot_fault,
    };
    for point in [
        SnapshotFaultPoint::BeforeChunkFinalization,
        SnapshotFaultPoint::BeforeRootPublication,
        SnapshotFaultPoint::AfterRootPublication,
    ] {
        let root = tempfile::tempdir().expect("root");
        let mut journal = store(root.path());
        let predecessor = journal.accept(&create()).unwrap();
        let usage = journal.checkpoint_artifacts.quota_snapshot(0).unwrap();
        let checkpoint = UserCheckpoint::new(
            CheckpointId::new(
                *operation(2, 1, ControlIntent::PinConversation { pinned: true }).id().as_bytes(),
            )
            .unwrap(),
            "x".repeat(peritus_journal::MAX_STATE_BYTES + 1),
            CheckpointReferences::new(1, 0, 0, None),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let change = operation(2, 1, ControlIntent::CreateCheckpoint(checkpoint.clone()));
        inject_snapshot_fault(point);
        // No enclosing snapshot publication: these faults exercise the new control pages.
        journal.accept_checkpoint(&change, &[]).expect_err("control page interruption");
        drop(journal);
        let mut journal = store(root.path());
        assert_eq!(journal.accept(&create()).unwrap(), predecessor);
        let accepted = point == SnapshotFaultPoint::AfterRootPublication;
        assert_eq!(journal.resolve(&change).unwrap().is_some(), accepted);
        let current = journal.load(create().conversation()).unwrap().unwrap();
        assert_eq!(current.revision(), if accepted { 2 } else { 1 });
        if !accepted {
            assert!(current.checkpoints().is_empty());
            assert_eq!(journal.checkpoint_artifacts.quota_snapshot(0).unwrap(), usage);
        }
        let receipt =
            journal.accept_checkpoint(&change, &[]).expect("resume same checkpoint identity");
        assert_eq!(journal.accept_checkpoint(&change, &[]).unwrap(), receipt);
        assert_eq!(
            journal.load(create().conversation()).unwrap().unwrap().checkpoints(),
            &[checkpoint]
        );
        assert_eq!(
            std::fs::read_dir(root.path().join("checkpoint-artifacts/publishing")).unwrap().count(),
            0
        );
    }
}

#[test]
fn checkpoint_operation_and_projection_over_16_mib_reopen_and_continue_exactly() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("conversation");
    let checkpoint = UserCheckpoint::new(
        CheckpointId::new(
            *operation(2, 1, ControlIntent::PinConversation { pinned: true }).id().as_bytes(),
        )
        .unwrap(),
        "x".repeat(peritus_journal::MAX_STATE_BYTES + 1),
        CheckpointReferences::new(1, 0, 0, None),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("complete checkpoint name");
    let change = operation(2, 1, ControlIntent::CreateCheckpoint(checkpoint.clone()));
    let receipt = journal
        .accept_checkpoint_snapshots(&change, &[])
        .expect("publish complete checkpoint operation");
    assert!(change.canonical_bytes().unwrap().len() > peritus_journal::MAX_STATE_BYTES);
    let current = journal.load(create().conversation()).unwrap().unwrap();
    assert!(current.canonical_bytes().unwrap().len() > peritus_journal::MAX_STATE_BYTES);
    drop(journal);
    let mut journal = store(root.path());
    assert_eq!(journal.accept_checkpoint_snapshots(&change, &[]).unwrap(), receipt);
    assert_eq!(journal.operation(create().conversation(), change.id()).unwrap(), Some(change));
    assert_eq!(journal.load(create().conversation()).unwrap(), Some(current));
    assert!(journal.conversation_ids().unwrap().contains_key(&create().conversation()));
    journal
        .accept(&operation(3, 2, ControlIntent::PinConversation { pinned: true }))
        .expect("continue same conversation");
    let current = journal.load(create().conversation()).unwrap().unwrap();
    assert!(current.pinned());
    assert_eq!(current.revision(), 3);
    assert_eq!(current.checkpoints(), &[checkpoint]);
}
