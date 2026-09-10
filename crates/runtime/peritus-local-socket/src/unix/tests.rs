use std::{
    fs,
    io::{Read as _, Write as _},
    os::unix::{
        fs::{MetadataExt as _, PermissionsExt as _, symlink},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::Duration,
};

use super::PreparedSocketPath;
use crate::{NATIVE_MAX_PATH_BYTES, bounded_path};

fn original(root: &Path) -> PathBuf {
    root.join("long-state-root".repeat(18)).join("peritus-0123456789abcdef0123456789abcdef.sock")
}

fn owner(root: &Path) -> u32 {
    fs::metadata(root).expect("root metadata").uid()
}

#[test]
fn ordinary_clients_round_trip_at_a_short_private_runtime_address() {
    let root = tempfile::tempdir().expect("state root");
    let original = original(root.path());
    assert!(original.as_os_str().len() > 252, "also exceeds the old macOS extension");
    let prepared = PreparedSocketPath::prepare(&original, owner(root.path())).expect("prepare");
    let path = prepared.path().to_path_buf();
    assert!(path.as_os_str().len() <= NATIVE_MAX_PATH_BYTES);
    let directory = path.parent().expect("private parent").to_path_buf();
    let metadata = fs::symlink_metadata(&directory).expect("directory metadata");
    assert_eq!(metadata.uid(), owner(root.path()));
    assert_eq!(metadata.mode() & 0o777, 0o700);
    let listener = UnixListener::bind(&path).expect("ordinary bind");
    let mut client = UnixStream::connect(&path).expect("ordinary connect");
    let (mut server, _) = listener.accept().expect("ordinary accept");
    client.set_read_timeout(Some(Duration::from_secs(2))).expect("read bound");
    server.write_all(b"hello").expect("write");
    let mut bytes = [0; 5];
    client.read_exact(&mut bytes).expect("read");
    assert_eq!(&bytes, b"hello");
    drop((client, server, listener));
    fs::remove_file(&path).expect("listener owner removes socket");
    drop(prepared);
    assert!(!directory.exists(), "empty owned runtime directory is removed");
}

#[test]
fn refuses_symlinks_wrong_owner_and_broad_permissions_without_repair() {
    let root = tempfile::tempdir().expect("state root");
    let original = original(root.path());
    let actual = bounded_path(&original, NATIVE_MAX_PATH_BYTES).expect("path");
    let directory = actual.parent().expect("private parent");
    symlink(root.path(), directory).expect("plant symlink");
    assert_eq!(
        PreparedSocketPath::prepare(&original, owner(root.path())).expect_err("symlink").kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert!(fs::symlink_metadata(directory).expect("symlink retained").file_type().is_symlink());
    fs::remove_file(directory).expect("remove fixture symlink");
    fs::create_dir(directory).expect("plant broad directory");
    fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).expect("broad mode");
    assert!(PreparedSocketPath::prepare(&original, owner(root.path())).is_err());
    assert_eq!(fs::metadata(directory).expect("mode retained").mode() & 0o777, 0o755);
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).expect("private mode");
    assert!(PreparedSocketPath::prepare(&original, owner(root.path()).wrapping_add(1)).is_err());
    fs::remove_dir(directory).expect("remove fixture directory");
}

#[test]
fn cleanup_preserves_replacements_and_nonempty_directories() {
    let root = tempfile::tempdir().expect("state root");
    let original = original(root.path());
    let prepared = PreparedSocketPath::prepare(&original, owner(root.path())).expect("prepare");
    let directory = prepared.path().parent().expect("parent").to_path_buf();
    let moved = directory.with_extension("retained");
    fs::rename(&directory, &moved).expect("move original directory");
    fs::create_dir(&directory).expect("replacement");
    drop(prepared);
    assert!(directory.is_dir(), "replacement directory survives");
    fs::remove_dir(&directory).expect("remove replacement fixture");
    fs::rename(&moved, &directory).expect("restore original");
    let prepared = PreparedSocketPath::prepare(&original, owner(root.path())).expect("reopen");
    let retained = directory.join("retained-data");
    fs::write(&retained, b"keep").expect("unrelated contents");
    drop(prepared);
    assert_eq!(fs::read(&retained).expect("retained"), b"keep");
    fs::remove_file(retained).expect("remove fixture contents");
    fs::remove_dir(directory).expect("remove fixture directory");
}
