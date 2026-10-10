use super::*;

#[test]
fn same_run_can_advance_the_exact_owned_postimage_but_another_run_cannot() {
    let mut checkpoint = UserCheckpoint::new(
        CheckpointId::new([1; 16]).expect("checkpoint ID"),
        "before mutation".to_owned(),
        CheckpointReferences::new(1, 1, 1, None),
        vec![
            CheckpointPath::new("artifact.txt".to_owned(), CheckpointFileVersion::Absent)
                .expect("path"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("checkpoint");
    let first =
        CheckpointFileVersion::present(Sha256Digest::new([2; 32]), 5, CheckpointFileMode::Regular);
    let second =
        CheckpointFileVersion::present(Sha256Digest::new([3; 32]), 6, CheckpointFileMode::Regular);

    checkpoint.seal([4; 16], &[("artifact.txt".to_owned(), first)]).expect("first seal");
    checkpoint.seal([4; 16], &[("artifact.txt".to_owned(), second)]).expect("same run");

    assert_eq!(checkpoint.paths()[0].owned_postchange(), Some(second));
    assert_eq!(
        checkpoint.seal([5; 16], &[("artifact.txt".to_owned(), first)]),
        Err(ControlError::IdempotencyConflict)
    );
}

#[test]
fn checkpoint_exclusions_keep_legacy_text_and_round_trip_long_paths() {
    let legacy =
        CheckpointExclusion::from_label_and_reason("a.txt".to_owned(), "deselected".to_owned())
            .expect("legacy-sized exclusion");
    let legacy_json = serde_json::to_string(&legacy).expect("legacy encoding");
    assert_eq!(legacy_json, "\"a.txt: deselected\"");
    let decoded_legacy: CheckpointExclusion =
        serde_json::from_str(&legacy_json).expect("legacy decoding");
    assert_eq!(decoded_legacy, legacy);

    let path = (0..4).map(|_| "a".repeat(255)).collect::<Vec<_>>().join("/");
    let structured =
        CheckpointExclusion::from_label_and_reason(path.clone(), "deselected".to_owned())
            .expect("long path exclusion");
    let encoded = serde_json::to_value(&structured).expect("structured encoding");
    assert_eq!(encoded, serde_json::json!({"path": path, "reason": "deselected"}));
    let decoded: CheckpointExclusion = serde_json::from_value(encoded).expect("decoding");
    assert_eq!(decoded.rendered(), format!("{path}: deselected"));
}

#[test]
fn checkpoint_exclusion_object_rejects_unknown_and_duplicate_fields() {
    assert!(
        serde_json::from_str::<CheckpointExclusion>(
            r#"{"path":"a.txt","reason":"excluded","extra":true}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<CheckpointExclusion>(
            r#"{"path":"a.txt","path":"b.txt","reason":"excluded"}"#
        )
        .is_err()
    );
}
