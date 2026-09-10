use super::*;

#[test]
fn range_scanning_is_chunk_independent_and_preserves_original_line_terminators() {
    let source = b"first\r\n\nlast";
    for width in 1..=source.len() {
        let mut scan = Scan::new(Selection::Lines { first: 2, last: 3 }, 32, source.len() as u64)
            .expect("scan");
        for chunk in source.chunks(width) {
            scan.accept(chunk).expect("chunk");
        }
        assert_eq!(scan.finish().expect("range"), ((7, 12), b"\nlast".to_vec()));
    }
}

#[test]
fn nonexistent_last_line_and_oversized_selected_lines_reject_without_partial_result() {
    let mut scan = Scan::new(Selection::Lines { first: 2, last: 2 }, 32, 2).expect("scan");
    scan.accept(b"a\n").expect("first line");
    assert!(scan.finish().is_err(), "trailing newline is not an extra line");
    let mut scan = Scan::new(Selection::Lines { first: 1, last: 1 }, 2, 4).expect("scan");
    assert!(scan.accept(b"long").is_err());
    let mut scan = Scan::new(Selection::All, 32, 2).expect("scan");
    assert!(scan.accept(b"grew").is_err());
}

#[test]
fn replaced_file_identity_and_changed_same_length_content_invalidate_the_observation() {
    let directory = tempfile::tempdir().expect("directory");
    let target = directory.path().join("file");
    std::fs::write(&target, b"before").expect("file");
    let identity = FolderIdentity::observe(directory.path()).expect("identity");
    let reader = FolderInspection::open(&identity).expect("reader");
    let path = WorkspacePath::new("file").expect("path");
    let original = reader.open_file(&path).expect("open");
    let before = original.metadata().expect("metadata");
    std::fs::write(&target, b"edited").expect("same-length change");
    let opened = std::fs::OpenOptions::new().write(true).open(&target).expect("set timestamp");
    opened
        .set_times(std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH))
        .expect("deterministic timestamp change");
    assert!(!same_version(&before, &original.metadata().expect("metadata")).expect("comparison"));
    drop(opened);
    std::fs::rename(&target, directory.path().join("old")).expect("move original");
    std::fs::write(&target, b"before").expect("same-size replacement");
    assert!(
        !same_version(
            &before,
            &reader.open_file(&path).expect("replacement").metadata().expect("metadata")
        )
        .expect("identity comparison")
    );
}
