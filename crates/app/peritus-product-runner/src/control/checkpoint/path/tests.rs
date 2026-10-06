//! Capture identity, versioned capability and canonical range invariants.

use super::*;

fn file() -> CheckpointFileVersion {
    CheckpointFileVersion::present(
        peritus_types::Sha256Digest::new([3; 32]),
        100,
        super::super::CheckpointFileMode::Regular,
    )
}

#[test]
fn old_file_manifest_bytes_survive_typed_owned_directory_sealing_and_capture_replay() {
    let mut path = CheckpointPath::new("node".to_owned(), file()).unwrap();
    let original = serde_json::to_vec(&path).unwrap();
    assert!(!String::from_utf8(original.clone()).unwrap().contains("coverage_schema"));
    path.seal(CheckpointFileVersion::empty_directory(
        peritus_patch::DirectoryMode::new(0o750).unwrap(),
    ))
    .unwrap();
    path.validate().unwrap();
    assert_eq!(path.coverage_schema, Some(2));
    let reopened: CheckpointPath =
        serde_json::from_slice(&serde_json::to_vec(&path).unwrap()).unwrap();
    reopened.validate().unwrap();
    path.unseal();
    assert_eq!(serde_json::to_vec(&path).unwrap(), original);
}

#[test]
fn selected_ranges_are_canonical_without_a_count_or_complete_source_ceiling() {
    let version = CheckpointFileVersion::present(
        peritus_types::Sha256Digest::new([4; 32]),
        100 * 1024 * 1024,
        super::super::CheckpointFileMode::Regular,
    );
    let mut ranges = (0..2048)
        .rev()
        .map(|i| {
            CheckpointRange::new(
                FileRange::Bytes { start: i * 2, end: i * 2 + 1 },
                i * 2,
                i * 2 + 1,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    ranges.push(ranges[0]);
    let path = CheckpointPath::selected_ranges("large".to_owned(), version, ranges).unwrap();
    let CheckpointCoverage::SelectedRanges(ranges) = path.coverage() else {
        panic!("scope");
    };
    assert_eq!(ranges.len(), 2048);
    path.validate().unwrap();
    let mut downgraded = path.clone();
    downgraded.coverage_schema = None;
    assert_eq!(downgraded.validate(), Err(ControlError::UnsupportedSchema));
    assert!(
        CheckpointPath::selected_ranges(
            "node".to_owned(),
            CheckpointFileVersion::Absent,
            ranges.to_vec()
        )
        .is_err()
    );
}
