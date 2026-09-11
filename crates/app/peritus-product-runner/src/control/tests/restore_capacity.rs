//! Restore settlement must not depend on free message capacity after a filesystem effect.

use super::*;

#[test]
fn applied_restore_invalidates_context_with_a_full_input_ledger() {
    let (mut record, _) = ConversationRecord::apply(None, &create()).expect("create");
    for byte in 10..74 {
        record = apply(
            &record,
            byte,
            ControlIntent::Queue(QueueIntent::Enqueue {
                id: InputId::new([byte; 16]).expect("input"),
                text: ControlText::new("x".repeat(8192)).expect("text"),
                dependencies: Vec::new(),
            }),
        )
        .expect("fill ledger");
    }
    assert_eq!(
        apply(
            &record,
            74,
            ControlIntent::Queue(QueueIntent::Enqueue {
                id: InputId::new([74; 16]).expect("input"),
                text: ControlText::new("one more".to_owned()).expect("text"),
                dependencies: Vec::new(),
            })
        ),
        Err(ControlError::Capacity)
    );
    let source = checkpoint(80, &record);
    record = apply(&record, 80, ControlIntent::CreateCheckpoint(source.clone())).expect("capture");
    let recovery = checkpoint(81, &record);
    let restore_id = RestoreId::new([82; 16]).expect("restore");
    let restore = RestoreOperation::prepared(
        restore_id,
        source.id(),
        peritus_types::Sha256Digest::new([1; 32]),
        peritus_types::Sha256Digest::new([2; 32]),
        recovery.id(),
    )
    .expect("prepare");
    record =
        apply(&record, 82, ControlIntent::PrepareRestore { restore, recovery }).expect("retain");
    let generation = record.inputs().generation();
    let next = apply(
        &record,
        83,
        ControlIntent::SettleRestore {
            restore: restore_id,
            status: RestoreStatus::Applied,
            conflicts: Vec::new(),
            transaction_manifest_digest: Some([3; 32]),
        },
    )
    .expect("settle despite full message capacity");
    assert_eq!(next.inputs().generation(), generation + 1);
    assert_eq!(next.restores()[0].status(), RestoreStatus::Applied);
    assert_eq!(record.inputs().order(), next.inputs().order());
}

fn checkpoint(byte: u8, record: &ConversationRecord) -> UserCheckpoint {
    UserCheckpoint::new(
        CheckpointId::new([byte; 16]).expect("checkpoint"),
        "capacity regression".to_owned(),
        CheckpointReferences::new(record.revision(), record.inputs().generation(), 0, None),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("checkpoint")
}

fn apply(
    record: &ConversationRecord,
    byte: u8,
    intent: ControlIntent,
) -> Result<ConversationRecord, ControlError> {
    let base = create();
    let operation = ControlOperation::new(
        OperationId::new([byte; 16]).expect("operation"),
        base.conversation(),
        ActorId::new([3; 16]).expect("actor"),
        WorkspaceId::new([4; 16]).expect("workspace"),
        record.revision(),
        intent,
    );
    ConversationRecord::apply(Some(record), &operation).map(|(next, _)| next)
}
