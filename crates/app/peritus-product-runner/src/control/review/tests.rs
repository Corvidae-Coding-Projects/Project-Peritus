//! Anchored feedback transition regressions.

use super::*;
use peritus_types::ActorId;

fn anchor(path: &str, candidate: u8) -> ReviewAnchor {
    ReviewAnchor::new(
        RunId::new([1; 16]).expect("run"),
        WorkspaceId::new([2; 16]).expect("workspace"),
        Sha256Digest::new([candidate; 32]),
        Sha256Digest::new([4; 32]),
        PathBuf::from(path),
        Sha256Digest::new([5; 32]),
        Sha256Digest::new([6; 32]),
        Sha256Digest::new([7; 32]),
        ReviewTarget::Hunk,
        ReviewRange::hunk(3, 2, 3, 4).expect("range"),
    )
    .expect("anchor")
}

#[test]
fn leave_alone_is_a_revisioned_input_and_stays_protected_until_dismissed() {
    let ledger = ReviewLedger::default();
    let inputs = InputLedger::default();
    let actor = ActorId::new([8; 16]).expect("actor");
    let id = OperationId::new([9; 16]).expect("operation");
    let (ledger, inputs) = ledger
        .add(
            &inputs,
            actor,
            id,
            anchor("src/lib.rs", 3),
            ReviewFeedback::LeaveAlone,
            ControlText::new("Keep this exact implementation".to_owned()).expect("message"),
        )
        .expect("add");
    assert_eq!(ledger.protected_paths(), [PathBuf::from("src/lib.rs")]);
    assert_eq!(ledger.pending_pipeline_permission(&inputs), Some(false));
    assert!(
        inputs
            .latest(InputId::new([9; 16]).expect("input"))
            .expect("input")
            .text()
            .contains("Hard constraint")
    );

    let (ledger, inputs) = ledger
        .rebind(
            &inputs,
            actor,
            OperationId::new([10; 16]).expect("rebind"),
            id,
            anchor("src/new.rs", 11),
        )
        .expect("rebind");
    assert_eq!(ledger.comment(id).expect("comment").revision(), 2);
    assert_eq!(ledger.protected_paths(), [PathBuf::from("src/new.rs")]);

    let (ledger, _) = ledger
        .dismiss(&inputs, actor, OperationId::new([12; 16]).expect("dismiss"), id)
        .expect("dismiss");
    assert!(ledger.protected_paths().is_empty());
    assert_eq!(ledger.comment(id).expect("comment").state(), ReviewCommentState::Dismissed);
}

#[test]
fn anchors_reject_line_only_and_unsafe_path_identity() {
    assert!(ReviewRange::hunk(0, 1, 1, 1).is_err());
    let mut invalid = anchor("src/lib.rs", 3);
    invalid.path = PathBuf::from("../secret");
    assert_eq!(invalid.validate(), Err(ControlError::InvalidInput));
    assert_ne!(anchor("src/lib.rs", 3), anchor("src/lib.rs", 4));
}
