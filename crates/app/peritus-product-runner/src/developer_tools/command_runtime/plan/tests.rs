#[cfg(unix)]
#[test]
fn executable_resolution_preserves_multicall_name_and_normalizes_parent() {
    use std::{os::unix::fs::symlink, process::Command};
    let root = tempfile::tempdir().expect("fixture");
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).expect("bin");
    symlink("/bin/sh", bin.join("named-shell")).expect("multicall symlink");
    symlink(&bin, root.path().join("linked-bin")).expect("parent symlink");
    for program in [
        "linked-bin/named-shell".to_owned(),
        root.path().join("linked-bin/named-shell").to_string_lossy().into_owned(),
    ] {
        let resolved = super::resolve_executable(&program, root.path()).expect("resolve");
        assert_eq!(
            std::path::Path::new(&resolved),
            bin.canonicalize().expect("bin").join("named-shell")
        );
        let output =
            Command::new(&resolved).args(["-c", "printf '%s' \"$0\""]).output().expect("launch");
        assert!(output.status.success());
        assert_eq!(output.stdout, resolved.as_bytes());
    }
}

#[cfg(unix)]
#[test]
fn executable_resolution_rejects_dangling_symlinks() {
    let root = tempfile::tempdir().expect("fixture");
    std::os::unix::fs::symlink(root.path().join("missing"), root.path().join("launcher"))
        .expect("dangling link");
    assert!(super::resolve_executable("./launcher", root.path()).is_err());
}
