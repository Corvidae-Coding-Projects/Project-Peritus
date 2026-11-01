//! Actual Unix endpoint custody and byte exchange in an owned disposable fixture.

#[cfg(unix)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use peritus_local_socket::{NATIVE_MAX_PATH_BYTES, PreparedSocketPath};
    use std::{
        fs,
        io::{Read, Write},
        os::{
            unix::fs::MetadataExt,
            unix::net::{UnixListener, UnixStream},
        },
    };

    let scratch = tempfile::tempdir()?;
    let canary = scratch.path().join("sibling-canary");
    fs::write(&canary, b"unchanged")?;
    let original = scratch.path().join("a".repeat(NATIVE_MAX_PATH_BYTES)).join("daemon.sock");
    let owner_uid = fs::metadata(scratch.path())?.uid();
    let prepared = PreparedSocketPath::prepare(&original, owner_uid)?;
    let endpoint = prepared.path().to_path_buf();
    assert_ne!(endpoint, original);
    assert!(endpoint.as_os_str().as_encoded_bytes().len() <= NATIVE_MAX_PATH_BYTES);
    let directory = endpoint.parent().ok_or("missing private endpoint parent")?.to_path_buf();
    let metadata = fs::symlink_metadata(&directory)?;
    assert!(metadata.file_type().is_dir());
    assert_eq!(metadata.uid(), owner_uid);
    assert_eq!(metadata.mode() & 0o777, 0o700);
    assert!(PreparedSocketPath::prepare(&original, owner_uid.wrapping_add(1)).is_err());

    let listener = UnixListener::bind(&endpoint)?;
    let mut client = UnixStream::connect(&endpoint)?;
    let (mut server, _) = listener.accept()?;
    client.write_all(b"request")?;
    let mut request = [0; 7];
    server.read_exact(&mut request)?;
    assert_eq!(&request, b"request");
    server.write_all(b"response")?;
    let mut response = [0; 8];
    client.read_exact(&mut response)?;
    assert_eq!(&response, b"response");
    drop(server);
    drop(client);
    drop(listener);
    fs::remove_file(&endpoint)?;
    drop(prepared);
    assert!(!directory.exists(), "owned empty runtime directory should be removed");
    assert_eq!(fs::read(&canary)?, b"unchanged");
    println!(
        "stage6-native-socket pass: exact request/response, owner mode, cleanup, sibling canary"
    );
    Ok(())
}

#[cfg(not(unix))]
fn main() {
    println!("stage6-native-socket unsupported on this OS");
}
