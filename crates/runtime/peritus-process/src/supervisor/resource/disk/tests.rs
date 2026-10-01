//! Deletions between real filesystem observations must not terminate a process owner.

use super::*;

#[test]
fn removed_entries_do_not_hide_surviving_file_usage() {
    let root = tempfile::tempdir().expect("workspace");
    let removed = root.path().join("temporary-build-output");
    fs::write(&removed, b"temporary").expect("temporary output");
    fs::write(root.path().join("retained"), b"retained").expect("retained output");
    let entries: Vec<_> = fs::read_dir(root.path()).expect("enumerate workspace").collect();
    fs::remove_file(removed).expect("build cleanup after enumeration");

    let mut pending = Vec::new();
    assert_eq!(entries_usage(entries, &mut pending).expect("sample after deletion"), 8);
    assert!(pending.is_empty());
}

#[test]
fn removed_pending_directories_do_not_hide_surviving_file_usage() {
    let root = tempfile::tempdir().expect("workspace");
    let removed = root.path().join("old-build");
    let retained = root.path().join("new-build");
    fs::create_dir(&removed).expect("old directory");
    fs::create_dir(&retained).expect("new directory");
    fs::write(retained.join("output"), b"output").expect("retained output");
    let mut pending = Vec::new();
    let total = entries_usage(fs::read_dir(root.path()).expect("enumerate"), &mut pending)
        .expect("queue descendants");
    assert_eq!(pending.len(), 2);
    fs::remove_dir(removed).expect("build cleanup after metadata lookup");

    assert_eq!(descendants_usage(pending, total).expect("sample remaining descendants"), 6);
}

#[test]
fn missing_workspace_and_invalid_descendants_remain_errors() {
    let root = tempfile::tempdir().expect("workspace");
    assert!(disk_usage(&root.path().join("absent")).is_err());
    let file = root.path().join("not-a-directory");
    fs::write(&file, b"file").expect("file");
    assert!(disk_usage(&file).is_err());
    assert!(descendants_usage(vec![file], 0).is_err());
}

#[test]
fn enumeration_errors_are_not_treated_as_removed_files() {
    let mut pending = Vec::new();
    let entries = [Err(io::Error::from(io::ErrorKind::PermissionDenied))];
    assert!(entries_usage(entries, &mut pending).is_err());
}

#[cfg(unix)]
#[test]
fn unreadable_descendants_remain_errors() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("workspace");
    let child = root.path().join("private");
    fs::create_dir(&child).expect("private directory");
    fs::set_permissions(&child, fs::Permissions::from_mode(0o0)).expect("deny traversal");
    let inaccessible = fs::read_dir(&child).is_err();
    let result = disk_usage(root.path());
    fs::set_permissions(child, fs::Permissions::from_mode(0o700)).expect("restore traversal");
    // A privileged test process can bypass mode bits; it still exercises successful sampling.
    assert_eq!(result.is_err(), inaccessible);
}
