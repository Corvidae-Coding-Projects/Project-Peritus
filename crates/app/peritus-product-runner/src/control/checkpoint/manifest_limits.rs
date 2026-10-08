//! Regressions for checkpoint-owned data exceeding historical representation ceilings.

use super::*;

fn checkpoint(
    paths: Vec<CheckpointPath>,
    exclusions: Vec<String>,
    effects: Vec<String>,
) -> Result<UserCheckpoint, ControlError> {
    UserCheckpoint::new(
        CheckpointId::new([1; 16]).expect("id"),
        "before work".to_owned(),
        CheckpointReferences::new(1, 1, 0, None),
        paths,
        exclusions,
        effects,
    )
}

#[test]
fn manifest_retains_more_than_u16_paths() {
    let paths = (0..=u16::MAX)
        .map(|index| {
            CheckpointPath::new(format!("files/{index:05}.txt"), CheckpointFileVersion::Absent)
                .expect("path")
        })
        .collect();
    let value =
        checkpoint(paths, Vec::new(), Vec::new()).expect("complete manifest must be representable");
    assert_eq!(value.paths().len(), 65_536);
}

#[test]
fn manifest_retains_more_than_u16_exclusions() {
    let value = checkpoint(Vec::new(), vec!["deselected source".to_owned(); 65_536], Vec::new())
        .expect("complete exclusions must be representable");
    assert_eq!(value.exclusions().count(), 65_536);
}

#[test]
fn manifest_retains_more_than_u16_external_effects() {
    let value = checkpoint(Vec::new(), Vec::new(), vec!["external effect".to_owned(); 65_536])
        .expect("complete effects must be representable");
    assert_eq!(value.external_effects().count(), 65_536);
}

#[test]
fn manifest_retains_name_past_former_byte_ceiling() {
    let name = "é".repeat(129);
    let value = UserCheckpoint::new(
        CheckpointId::new([1; 16]).expect("id"),
        name.clone(),
        CheckpointReferences::new(1, 1, 0, None),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("inert name must remain exact");
    assert_eq!(value.name(), name);
}

#[test]
fn manifest_retains_path_past_former_byte_ceiling() {
    let path = format!("{}target.txt", "directory/".repeat(420));
    assert!(peritus_patch::WorkspacePath::new(path.clone()).is_ok());
    let value = CheckpointPath::new(path.clone(), CheckpointFileVersion::Absent)
        .expect("authority-safe native path must remain representable");
    assert_eq!(value.path(), path);
}

#[test]
fn manifest_retains_exclusion_label_without_truncation() {
    let label = format!("{}empty directory", "directory/".repeat(60));
    let exclusion = format!("{label}: empty directory removal cannot restore the directory");
    assert!(peritus_patch::WorkspacePath::new(label).is_ok());
    let value = checkpoint(Vec::new(), vec![exclusion.clone()], Vec::new())
        .expect("valid label plus explanation must remain exact");
    assert_eq!(value.exclusions().next(), Some(exclusion.as_str()));
}

#[test]
fn manifest_retains_external_effect_past_former_byte_ceiling() {
    let effect = "effect".repeat(100);
    let value = checkpoint(Vec::new(), Vec::new(), vec![effect.clone()])
        .expect("inert effect must remain exact");
    assert_eq!(value.external_effects().next(), Some(effect.as_str()));
}

#[test]
fn restore_retains_more_than_u16_conflicting_paths() {
    let conflicts = (0..=u16::MAX).map(|index| format!("files/{index:05}.txt")).collect::<Vec<_>>();
    let mut restore = RestoreOperation::prepared(
        RestoreId::new([2; 16]).expect("restore"),
        CheckpointId::new([1; 16]).expect("checkpoint"),
        Sha256Digest::new([3; 32]),
        Sha256Digest::new([4; 32]),
        CheckpointId::new([5; 16]).expect("recovery"),
    )
    .expect("prepare");
    restore
        .settle(RestoreStatus::Conflict, conflicts.clone(), None)
        .expect("complete conflict evidence must remain representable");
    assert!(restore.conflicts().eq(conflicts.iter().map(String::as_str)));
}

#[test]
fn restore_retains_conflicting_path_past_former_byte_ceiling() {
    let conflict = format!("{}target.txt", "directory/".repeat(420));
    let mut restore = RestoreOperation::prepared(
        RestoreId::new([2; 16]).expect("restore"),
        CheckpointId::new([1; 16]).expect("checkpoint"),
        Sha256Digest::new([3; 32]),
        Sha256Digest::new([4; 32]),
        CheckpointId::new([5; 16]).expect("recovery"),
    )
    .expect("prepare");
    restore
        .settle(RestoreStatus::Conflict, vec![conflict.clone()], None)
        .expect("authority-safe conflict path must remain representable");
    assert_eq!(restore.conflicts().next(), Some(conflict.as_str()));
}

#[test]
fn legacy_strings_reencode_exactly_without_a_new_discriminator() {
    let value = checkpoint(
        vec![CheckpointPath::new("source.txt".to_owned(), CheckpointFileVersion::Absent).unwrap()],
        vec!["source.txt: deselected".to_owned()],
        vec!["External effects are retained".to_owned()],
    )
    .unwrap();
    let expected = [
        "{\"id\":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],",
        "\"name\":\"before work\",\"references\":{\"source_conversation_revision\":1,",
        "\"context_generation\":1,\"brief_revision\":0,\"goal_revision\":null},",
        "\"paths\":[{\"path\":\"source.txt\",\"checkpoint\":\"absent\",",
        "\"owned_postchange\":null}],\"exclusions\":[\"source.txt: deselected\"],",
        "\"external_effects\":[\"External effects are retained\"],\"sealed_by_run\":null}",
    ]
    .join("");
    assert_eq!(serde_json::to_string(&value).unwrap(), expected);
    assert_eq!(serde_json::from_str::<UserCheckpoint>(&expected).unwrap(), value);
}

#[test]
fn structured_exclusion_recovers_separate_reason_path_and_display_after_reopen() {
    let path = format!("{}source.txt", "directory/".repeat(420));
    let exclusion = CheckpointExclusion::new(
        path.clone(),
        Some(path.clone()),
        CheckpointExclusionReason::Deselected,
    )
    .unwrap();
    let value =
        checkpoint(Vec::new(), Vec::new(), Vec::new()).unwrap().with_exclusions(vec![exclusion]);
    let bytes = serde_json::to_vec(&value).unwrap();
    let reopened: UserCheckpoint = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reopened, value);
    let details = reopened.exclusion_records()[0].details().unwrap();
    assert_eq!(details.path(), Some(path.as_str()));
    assert_eq!(details.label(), path);
    assert_eq!(details.reason(), CheckpointExclusionReason::Deselected);
    assert_eq!(details.explanation(), "deselected");
    assert_eq!(reopened.exclusions().next(), Some(format!("{path}: deselected").as_str()));
    assert!(!format!("{reopened:?}").contains(&path));
}

#[test]
fn path_identity_is_stable_across_versions_and_distinct_for_other_targets() {
    let captured =
        CheckpointPath::new("source.txt".to_owned(), CheckpointFileVersion::Absent).unwrap();
    let different_version = CheckpointPath::new(
        "source.txt".to_owned(),
        CheckpointFileVersion::present(Sha256Digest::new([8; 32]), 4, CheckpointFileMode::Regular),
    )
    .unwrap();
    let different_path =
        CheckpointPath::new("other.txt".to_owned(), CheckpointFileVersion::Absent).unwrap();
    assert_eq!(captured.path_id(), different_version.path_id());
    assert_ne!(captured.path_id(), different_path.path_id());
}

#[test]
fn removing_length_allowances_preserves_native_authority_and_inert_text_checks() {
    for path in ["", "/etc/passwd", "a/../b", "a//b", ".git/config", "a\0b"] {
        assert!(CheckpointPath::new(path.to_owned(), CheckpointFileVersion::Absent).is_err());
    }
    for text in ["", " \n\t", "\u{1b}]0;title\u{7}"] {
        assert!(CheckpointText::new(text.to_owned()).is_err());
        assert!(
            serde_json::from_value::<CheckpointText>(serde_json::Value::String(text.to_owned()))
                .is_err()
        );
    }
    let path = CheckpointPath::new("source.txt".to_owned(), CheckpointFileVersion::Absent).unwrap();
    assert_eq!(
        checkpoint(vec![path.clone(), path], Vec::new(), Vec::new()),
        Err(ControlError::InvalidInput)
    );
    // An excluded metadata label does not grant authority to the protected target it names.
    assert!(
        CheckpointExclusion::new(
            ".git/config".to_owned(),
            Some(".git/config".to_owned()),
            CheckpointExclusionReason::Deselected
        )
        .is_ok()
    );
}

#[test]
fn durable_decode_checks_canonical_coverage_and_run_identity_without_count_allowances() {
    let path = CheckpointPath::new("source.txt".to_owned(), CheckpointFileVersion::Absent).unwrap();
    let value = checkpoint(vec![path], Vec::new(), Vec::new()).unwrap();
    let mut duplicate = serde_json::to_value(&value).unwrap();
    let repeated = duplicate["paths"][0].clone();
    duplicate["paths"].as_array_mut().unwrap().push(repeated);
    assert!(serde_json::from_value::<UserCheckpoint>(duplicate).is_err());
    let mut reserved_run = serde_json::to_value(&value).unwrap();
    reserved_run["automatic_run"] = serde_json::to_value([0_u8; 16]).unwrap();
    assert!(serde_json::from_value::<UserCheckpoint>(reserved_run).is_err());
    let mut invalid_coverage = serde_json::to_value(&value).unwrap();
    invalid_coverage["paths"][0]["coverage_schema"] = serde_json::json!(2);
    assert!(serde_json::from_value::<UserCheckpoint>(invalid_coverage).is_err());
}
