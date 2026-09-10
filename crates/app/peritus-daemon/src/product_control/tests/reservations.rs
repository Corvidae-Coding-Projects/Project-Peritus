//! A prepared combined restore owns its child identity through crash and settlement.

use super::*;
use peritus_product_runner::control::{
    CheckpointId, CheckpointReferences, ConversationBranch, ConversationBranchMode, RestoreId,
    RestoreOperation, RestoreStatus, UserCheckpoint,
};

#[test]
fn prepared_restore_reserves_child_until_exact_publication_even_after_reopen() {
    let root = tempfile::tempdir().unwrap();
    let mut journal = store(root.path());
    journal.accept(&create()).unwrap();
    let checkpoint = make_checkpoint(2, 1);
    journal
        .accept_checkpoint(
            &operation(2, 1, ControlIntent::CreateCheckpoint(checkpoint.clone())),
            &[],
        )
        .unwrap();
    let branch = make_branch(4, checkpoint.id());
    let recovery = make_checkpoint(3, 2);
    let preparation = prepare(3, 2, &checkpoint, recovery, branch.clone());
    journal.accept_restore_preparation(&preparation, &[]).unwrap();
    assert!(journal.load(branch.child()).unwrap().is_none(), "reservation is not publication");
    drop(journal);
    let mut journal = store(root.path());
    let collision = ControlOperation::new(
        OperationId::new([9; 16]).unwrap(),
        branch.child(),
        ActorId::new([3; 16]).unwrap(),
        WorkspaceId::new([4; 16]).unwrap(),
        0,
        ControlIntent::CreateConversation {
            title: ControlText::new("collision".to_owned()).unwrap(),
        },
    );
    assert!(matches!(
        journal.accept(&collision),
        Err(Error::Control(ControlError::IdempotencyConflict))
    ));
    assert!(journal.resolve(&collision).unwrap().is_none());
    let rival = make_branch(5, checkpoint.id());
    let rival_child = child(&rival);
    let rival_source =
        operation(5, 3, ControlIntent::ReserveFork { branch: rival.clone(), now_unix_millis: 0 });
    assert!(matches!(
        journal.accept_fork(&rival_source, &rival_child, &rival),
        Err(Error::Control(ControlError::IdempotencyConflict))
    ));
    let restore = RestoreId::new(*preparation.id().as_bytes()).unwrap();
    journal
        .accept_restore_settlement(
            &operation(
                6,
                3,
                ControlIntent::SettleRestore {
                    restore,
                    status: RestoreStatus::Applied,
                    conflicts: Vec::new(),
                    transaction_manifest_digest: Some(sha256(b"applied").into_bytes()),
                },
            ),
            Some(b"applied".to_vec()),
        )
        .unwrap();
    assert!(matches!(
        journal.accept(&collision),
        Err(Error::Control(ControlError::IdempotencyConflict))
    ));
    let publication =
        operation(4, 4, ControlIntent::PublishRestoreBranch { restore, branch: branch.clone() });
    let receipt = journal.accept_fork(&publication, &child(&branch), &branch).unwrap();
    assert_eq!(journal.accept_fork(&publication, &child(&branch), &branch).unwrap(), receipt);
    assert!(journal.load(branch.child()).unwrap().is_some());
}

fn make_checkpoint(index: u32, revision: u64) -> UserCheckpoint {
    UserCheckpoint::new(
        CheckpointId::new(
            *operation(index, revision, ControlIntent::PinConversation { pinned: true })
                .id()
                .as_bytes(),
        )
        .unwrap(),
        "exact checkpoint".to_owned(),
        CheckpointReferences::new(revision, 0, 0, None),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap()
}

fn make_branch(index: u32, checkpoint: CheckpointId) -> ConversationBranch {
    ConversationBranch::new(
        operation(index, 0, ControlIntent::PinConversation { pinned: true }).id(),
        create().conversation(),
        WorkspaceId::new([4; 16]).unwrap(),
        1,
        OperationId::new(*checkpoint.as_bytes()).unwrap(),
        0,
        0,
        0,
        ConversationId::new([8; 16]).unwrap(),
        WorkspaceId::new([4; 16]).unwrap(),
        ConversationBranchMode::ReadOnlyCurrentWorkspace,
        "reserved child".to_owned(),
        None,
        Vec::new(),
        None,
    )
    .unwrap()
}

fn prepare(
    index: u32,
    revision: u64,
    source: &UserCheckpoint,
    recovery: UserCheckpoint,
    branch: ConversationBranch,
) -> ControlOperation {
    let operation = operation(index, revision, ControlIntent::PinConversation { pinned: true });
    let restore = RestoreOperation::prepared(
        RestoreId::new(*operation.id().as_bytes()).unwrap(),
        source.id(),
        peritus_types::Sha256Digest::new([1; 32]),
        peritus_types::Sha256Digest::new([2; 32]),
        recovery.id(),
    )
    .unwrap()
    .with_branch(branch)
    .unwrap();
    super::operation(index, revision, ControlIntent::PrepareRestore { restore, recovery })
}

fn child(branch: &ConversationBranch) -> ControlOperation {
    ControlOperation::new(
        branch.operation(),
        branch.child(),
        ActorId::new([3; 16]).unwrap(),
        WorkspaceId::new([4; 16]).unwrap(),
        0,
        ControlIntent::CreateFork { branch: branch.clone() },
    )
}
