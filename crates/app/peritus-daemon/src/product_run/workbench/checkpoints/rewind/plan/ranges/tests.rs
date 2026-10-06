use super::*;
use peritus_patch::FileMode;
use peritus_product_runner::control::FileRange;
use std::io::Write as _;

fn snapshot(bytes: &[u8], mode: FileMode) -> SnapshotFile {
    let mut file = tempfile::NamedTempFile::new().expect("temporary snapshot");
    file.write_all(bytes).expect("snapshot bytes");
    SnapshotFile::from_source(
        Arc::new(MergedSource(file.into_temp_path())),
        peritus_codec::sha256(bytes),
        bytes.len() as u64,
        mode,
    )
}
fn bytes(snapshot: &SnapshotFile) -> Vec<u8> {
    let mut bytes = Vec::new();
    snapshot.write_to(&mut bytes).expect("verified snapshot");
    bytes
}

#[test]
fn selected_line_restores_original_length_and_preserves_current_surroundings_and_mode() {
    let saved = snapshot(b"savedhead\nold\nsavetail\n", FileMode::Regular);
    let before =
        snapshot(b"currentheader\nchanged line longer\ncurrenttail\n", FileMode::Executable);
    let selected = CheckpointRange::new(FileRange::Lines { first: 2, last: 2 }, 10, 14)
        .expect("selected line");
    let result = materialize(&saved, &before, &[selected]).expect("partial restore");
    assert_eq!(bytes(&result), b"currentheader\nold\ncurrenttail\n");
    assert!(matches!(result.identity(), Preimage::Present { mode: FileMode::Executable, .. }));
    assert_ne!(result.identity(), saved.identity());
}

#[test]
fn overlapping_and_duplicate_byte_and_line_selections_restore_one_union() {
    let saved = snapshot(b"a\nbbbb\ncccc\nz\n", FileMode::Regular);
    let before = snapshot(b"X\nEEEE\nFFFF\nY\n", FileMode::Regular);
    let line = CheckpointRange::new(FileRange::Lines { first: 2, last: 3 }, 2, 12).expect("lines");
    let byte = CheckpointRange::new(FileRange::Bytes { start: 4, end: 9 }, 4, 9).expect("bytes");
    assert_eq!(
        bytes(&materialize(&saved, &before, &[line, byte, line]).expect("union restore")),
        b"X\nbbbb\ncccc\nY\n"
    );
}

#[test]
fn disjoint_line_replacements_preserve_current_gaps_with_different_selected_lengths() {
    let saved = snapshot(b"a\nold\nc\nx\ne\n", FileMode::Regular);
    let before =
        snapshot(b"A\nlong new text\nCURRENT GAP\nvery long replacement\nE\n", FileMode::Regular);
    let first = CheckpointRange::new(FileRange::Lines { first: 2, last: 2 }, 2, 6).expect("first");
    let second =
        CheckpointRange::new(FileRange::Lines { first: 4, last: 4 }, 8, 10).expect("second");
    assert_eq!(
        bytes(&materialize(&saved, &before, &[first, second]).expect("disjoint restore")),
        b"A\nold\nCURRENT GAP\nx\nE\n"
    );
}

#[test]
fn missing_selection_and_forged_resolved_capture_are_rejected_before_any_workspace_effect() {
    let saved = snapshot(b"a\nold\nz\n", FileMode::Regular);
    let before = snapshot(b"only one line\n", FileMode::Regular);
    let selected =
        CheckpointRange::new(FileRange::Lines { first: 2, last: 2 }, 2, 6).expect("line");
    assert!(materialize(&saved, &before, &[selected]).is_err());
    let forged = CheckpointRange::new(FileRange::Lines { first: 2, last: 2 }, 0, 2)
        .expect("structural interval");
    assert!(materialize(&saved, &saved, &[forged]).is_err());
}

#[test]
fn thousands_of_disjoint_line_selections_preserve_all_unselected_current_lines() {
    let saved_bytes = b"old\nsaved gap\n".repeat(2048);
    let current_bytes = b"much longer owned line\nCURRENT GAP\n".repeat(2048);
    let saved = snapshot(&saved_bytes, FileMode::Regular);
    let current = snapshot(&current_bytes, FileMode::Regular);
    let selected = (0..2048_u32)
        .map(|index| {
            CheckpointRange::new(
                FileRange::Lines { first: index * 2 + 1, last: index * 2 + 1 },
                u64::from(index) * 14,
                u64::from(index) * 14 + 4,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let restored = materialize(&saved, &current, &selected).unwrap();
    assert_eq!(bytes(&restored), b"old\nCURRENT GAP\n".repeat(2048));
}
