//! Failed link creation must leave the existing candidate path intact.

use super::*;

#[cfg(unix)]
#[test]
fn failed_symlink_creation_does_not_delete_the_previous_file_or_link() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("candidate");
    fs::write(&path, b"current human bytes").unwrap();
    assert!(restore_link(&path, b"invalid\0target").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"current human bytes");
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink("previous-relative-target", &path).unwrap();
    assert!(restore_link(&path, b"invalid\0target").is_err());
    assert_eq!(fs::read_link(&path).unwrap(), Path::new("previous-relative-target"));
}

#[cfg(unix)]
#[test]
fn replacement_link_preserves_relative_target_bytes_without_touching_its_referent() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("candidate");
    let target = temporary.path().join("target");
    fs::write(&target, b"unrelated target contents").unwrap();
    fs::write(&path, b"replace these candidate bytes").unwrap();
    restore_link(&path, b"target").unwrap();
    assert_eq!(fs::read_link(&path).unwrap(), Path::new("target"));
    assert_eq!(fs::read(&target).unwrap(), b"unrelated target contents");
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 2);
}
