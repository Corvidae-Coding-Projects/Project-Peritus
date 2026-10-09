use super::*;
use std::fs;

#[cfg(unix)]
mod races;

fn fixture() -> (tempfile::TempDir, FolderInspection) {
    let directory = tempfile::tempdir().expect("root");
    let identity = FolderIdentity::observe(directory.path()).expect("identity");
    let reader = FolderInspection::open(&identity).expect("reader");
    (directory, reader)
}
fn path(value: &str) -> WorkspacePath {
    WorkspacePath::new(value).expect("path")
}

#[test]
fn exact_bytes_ranges_and_full_source_digests_are_independent_and_never_truncated() {
    let (directory, reader) = fixture();
    fs::create_dir(directory.path().join("src")).expect("subdir");
    let source = "first\r\nλ line\nlast".as_bytes();
    fs::write(directory.path().join("src/reference.txt"), source).expect("source");
    for (selection, expected, range) in [
        (FileReadSelection::all(), source, (0, source.len() as u64)),
        (FileReadSelection::bytes(0, 5).expect("bytes"), b"first".as_slice(), (0, 5)),
        (FileReadSelection::lines(2, 2).expect("lines"), "λ line\n".as_bytes(), (7, 15)),
    ] {
        let result = reader.read_file(&path("src/reference.txt"), selection, 1024).expect("read");
        assert_eq!(result.bytes(), expected);
        assert_eq!(result.range(), range);
        assert_eq!(result.source_bytes(), source.len() as u64);
        assert_eq!(result.source_digest(), peritus_codec::sha256(source));
        assert_eq!(result.digest(), peritus_codec::sha256(expected));
        assert!(!format!("{result:?}").contains("λ line"), "debug hides source content");
    }
    assert!(reader.read_file(&path("src/reference.txt"), FileReadSelection::all(), 5).is_err());
    assert!(
        reader
            .read_file(
                &path("src/reference.txt"),
                FileReadSelection::bytes(0, 5).expect("selection"),
                4
            )
            .is_err()
    );
    assert!(
        reader
            .read_file(
                &path("src/reference.txt"),
                FileReadSelection::lines(4, 4).expect("missing"),
                1024
            )
            .is_err()
    );
}

#[test]
fn empty_files_large_ranged_files_and_invalid_bounds_have_explicit_results() {
    let (directory, reader) = fixture();
    let target = path("reference");
    fs::write(directory.path().join("reference"), []).expect("empty");
    let empty = reader.read_file(&target, FileReadSelection::all(), 1).expect("empty read");
    assert!(empty.bytes().is_empty());
    assert_eq!(empty.range(), (0, 0));
    assert!(reader.read_file(&target, FileReadSelection::lines(1, 1).expect("line"), 1).is_err());
    let file = fs::File::create(directory.path().join("reference")).expect("file");
    file.set_len(crate::MAX_INSPECTION_FILE_BYTES + 1).expect("large");
    assert!(
        reader
            .read_file(&target, FileReadSelection::all(), crate::MAX_INSPECTION_FILE_BYTES)
            .is_err()
    );
    assert_eq!(
        reader
            .read_file(&target, FileReadSelection::bytes(0, 1).expect("range"), 1)
            .expect("explicit range")
            .bytes(),
        &[0]
    );
    file.set_len(MAX_INSPECTION_SOURCE_BYTES + 1).expect("excessive");
    assert!(reader.read_file(&target, FileReadSelection::bytes(0, 1).expect("range"), 1).is_err());
    for bound in [0, crate::MAX_INSPECTION_FILE_BYTES + 1] {
        assert!(reader.read_file(&target, FileReadSelection::all(), bound).is_err());
    }
    for (start, end) in [(0, 0), (2, 1), (0, MAX_INSPECTION_SOURCE_BYTES + 1)] {
        assert!(FileReadSelection::bytes(start, end).is_err());
    }
    for (first, last) in [(0, 1), (2, 1), (1, u32::MAX)] {
        assert!(FileReadSelection::lines(first, last).is_err());
    }
}

#[cfg(unix)]
#[test]
fn symlinks_at_every_component_and_replaced_roots_cannot_read_an_outside_file() {
    use std::os::unix::fs::symlink;
    let (directory, reader) = fixture();
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("secret"), b"outside-only").expect("outside bytes");
    symlink(outside.path(), directory.path().join("alias")).expect("directory symlink");
    symlink(outside.path().join("secret"), directory.path().join("leaf")).expect("file symlink");
    assert!(reader.read_file(&path("alias/secret"), FileReadSelection::all(), 100).is_err());
    assert!(reader.read_file(&path("leaf"), FileReadSelection::all(), 100).is_err());
    let child = directory.path().join("selected");
    fs::create_dir(&child).expect("child");
    let identity = FolderIdentity::observe(&child).expect("identity");
    fs::rename(&child, directory.path().join("moved")).expect("move");
    fs::create_dir(&child).expect("replacement");
    assert!(FolderInspection::open(&identity).is_err());
}
