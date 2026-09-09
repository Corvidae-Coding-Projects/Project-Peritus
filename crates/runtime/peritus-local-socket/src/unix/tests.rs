use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use super::{MAX_PATH_BYTES, STANDARD_PATH_BYTES, bind, connect};

fn long_socket_path() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("temporary directory");
    let mut directory = root.path().to_path_buf();
    while directory.as_os_str().len() < STANDARD_PATH_BYTES + 40 {
        directory.push("a".repeat(40));
    }
    std::fs::create_dir_all(&directory).expect("nested directories");
    let path = directory.join("peritus-0123456789abcdef0123456789abcdef.sock");
    assert!(path.as_os_str().len() > STANDARD_PATH_BYTES);
    assert!(path.as_os_str().len() <= MAX_PATH_BYTES);
    (root, path)
}

fn round_trip(path: &std::path::Path) {
    let listener = bind(path).expect("bind");
    let server = std::thread::spawn(move || {
        let (mut accepted, _) = listener.accept().expect("accept");
        let mut received = [0_u8; 5];
        accepted.read_exact(&mut received).expect("read request");
        accepted.write_all(&received).expect("echo");
    });
    let mut client = connect(path).expect("connect");
    client.write_all(b"hello").expect("write");
    let mut echoed = [0_u8; 5];
    client.read_exact(&mut echoed).expect("read echo");
    assert_eq!(&echoed, b"hello");
    server.join().expect("server thread");
    let metadata = std::fs::symlink_metadata(path).expect("socket file at the real path");
    assert!(std::os::unix::fs::FileTypeExt::is_socket(&metadata.file_type()));
}

#[test]
fn standard_length_paths_round_trip() {
    let root = tempfile::tempdir().expect("temporary directory");
    let path = root.path().join("short.sock");
    assert!(path.as_os_str().len() <= STANDARD_PATH_BYTES);
    round_trip(&path);
}

#[test]
fn long_paths_round_trip_at_their_real_location() {
    let (_root, path) = long_socket_path();
    round_trip(&path);
}

#[test]
fn long_paths_need_the_long_form() {
    let (_root, path) = long_socket_path();
    let _listener = bind(&path).expect("bind");
    let error = UnixStream::connect(&path).expect_err("standard connect refuses the long path");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn paths_beyond_the_platform_limit_are_refused_with_the_limit_named() {
    let path = PathBuf::from(format!("/{}", "b".repeat(MAX_PATH_BYTES + 8)));
    let error = bind(&path).expect_err("over-long path");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains(&format!("at most {MAX_PATH_BYTES}")));
    let error = connect(&path).expect_err("over-long path");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn standard_limit_matches_the_platform_structure() {
    #[cfg(target_os = "macos")]
    assert_eq!(STANDARD_PATH_BYTES, 103);
    #[cfg(target_os = "linux")]
    assert_eq!(STANDARD_PATH_BYTES, 107);
}
