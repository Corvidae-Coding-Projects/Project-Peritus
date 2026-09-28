//! Workspace replacement preserves existing file identity properties and adjacent user data.

use super::*;

#[test]
fn replacement_preserves_adjacent_user_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("program.py");
    let adjacent = path.with_extension("peritus-new");
    fs::write(&path, "before").unwrap();
    fs::write(&adjacent, "user-owned data").unwrap();
    atomic_write(&path, b"after").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"after");
    assert_eq!(fs::read(&adjacent).unwrap(), b"user-owned data");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn rejected_replacement_preserves_directory_contents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("user.txt"), "user-owned data").unwrap();
    assert!(atomic_write(&path, b"after").is_err());
    assert_eq!(fs::read(path.join("user.txt")).unwrap(), b"user-owned data");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn replacement_keeps_executable_and_private_permissions() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("program.sh");
    for mode in [0o755, 0o700, 0o640, 0o600] {
        fs::write(&path, "before").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        atomic_write(&path, b"#!/bin/sh\nprintf after").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, mode);
        if mode & 0o100 != 0 {
            let output = std::process::Command::new(&path).output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"after");
        }
    }
}
