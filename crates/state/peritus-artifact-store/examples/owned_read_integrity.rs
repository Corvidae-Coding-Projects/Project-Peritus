//! Public-interface regression: a successfully completed stream must contain the original bytes.

use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, ErrorCode, MediaType, StoreConfig,
    WriteRequest,
};
use peritus_types::EventId;
use sha2::{Digest, Sha256};

type Outcome<T> = Result<T, Box<dyn Error>>;
const ORIGINAL: &[u8] = b"0123456789ABCDEF";

fn publish(root: &Path, bytes: &[u8]) -> Outcome<(ArtifactStore, PathBuf, ArtifactDigest)> {
    let store = ArtifactStore::open(StoreConfig::new(root, 256, 4096)?)?;
    let digest = ArtifactDigest::new(Sha256::digest(bytes).into());
    let size = u64::try_from(bytes.len())?;
    let event = EventId::new([19; 16]).map_err(|error| format!("{error:?}"))?;
    let mut writer = store.begin_owned_write(WriteRequest::new(
        digest,
        size,
        256,
        MediaType::new("application/octet-stream")?,
        EncryptionMetadata::unencrypted(),
        event,
    ))?;
    writer.write_chunk(bytes)?;
    store.complete_write(writer)?;
    assert_eq!(store.read(digest, 256)?, bytes);
    let hex = digest.to_hex();
    let object = root.join("objects/sha256").join(&hex[..2]).join(&hex);
    Ok((store, object, digest))
}

fn unchanged(root: &Path) -> Outcome<()> {
    let (store, _, digest) = publish(&root.join("unchanged"), ORIGINAL)?;
    let mut reader = store.open_read(digest)?;
    assert_eq!(reader.read_chunk(0).expect_err("zero bound").code(), ErrorCode::ByteLimitExceeded);
    assert_eq!(reader.next_offset(), 0);
    assert_eq!(
        reader.read_chunk(257).expect_err("oversized bound").code(),
        ErrorCode::ByteLimitExceeded
    );
    assert_eq!(reader.next_offset(), 0);
    let mut returned = Vec::new();
    while let Some(chunk) = reader.read_chunk(4)? {
        assert_eq!(chunk.offset(), u64::try_from(returned.len())?);
        returned.extend_from_slice(chunk.bytes());
    }
    assert_eq!(returned, ORIGINAL);
    assert!(reader.read_chunk(4)?.is_none());
    println!("PASS unchanged bytes, exact offsets, reversible bounds and repeated EOF");
    Ok(())
}

fn same_size_mutation(root: &Path, between_chunks: bool) -> Outcome<()> {
    let name = if between_chunks { "between-chunks" } else { "before-first" };
    let (store, object, digest) = publish(&root.join(name), ORIGINAL)?;
    let mut reader = store.open_read(digest)?;
    let mut returned = Vec::new();
    if between_chunks {
        let first = reader.read_chunk(4)?.ok_or("missing first chunk")?;
        assert_eq!(first.bytes(), b"0123");
        returned.extend_from_slice(first.bytes());
    }
    let mut external = OpenOptions::new().write(true).open(object)?;
    external.seek(SeekFrom::Start(if between_chunks { 8 } else { 0 }))?;
    external.write_all(b"Z")?;
    external.sync_all()?;
    loop {
        match reader.read_chunk(4) {
            Ok(Some(chunk)) => returned.extend_from_slice(chunk.bytes()),
            Ok(None) => panic!(
                "corrupt stream completed: {name}, original digest {}, returned {returned:?}",
                digest.to_hex()
            ),
            Err(error) => {
                assert_eq!(error.code(), ErrorCode::CorruptObject);
                break;
            }
        }
    }
    assert_eq!(
        reader.read_chunk(4).expect_err("failed stream must remain failed").code(),
        ErrorCode::CorruptObject
    );
    println!("PASS {name}: same-size corruption rejected and terminal failure retained");
    Ok(())
}

fn empty_growth(root: &Path) -> Outcome<()> {
    let (store, object, digest) = publish(&root.join("empty-growth"), b"")?;
    let mut reader = store.open_read(digest)?;
    fs::write(object, b"X")?;
    assert_eq!(
        reader.read_chunk(1).expect_err("empty stream grew").code(),
        ErrorCode::CorruptObject
    );
    assert!(reader.read_chunk(1).is_err());
    println!("PASS growth before empty-stream completion rejected");
    Ok(())
}

fn failed_growth_retry(root: &Path) -> Outcome<()> {
    let (store, object, digest) = publish(&root.join("growth-retry"), ORIGINAL)?;
    let mut reader = store.open_read(digest)?;
    let mut external = OpenOptions::new().append(true).open(&object)?;
    external.write_all(b"X")?;
    external.sync_all()?;
    assert_eq!(reader.read_chunk(16).expect_err("stream grew").code(), ErrorCode::CorruptObject);
    external.set_len(16)?;
    assert!(reader.read_chunk(0).is_err());
    assert_eq!(
        reader.read_chunk(16).expect_err("failed read cannot turn into successful EOF").code(),
        ErrorCode::CorruptObject
    );
    println!("PASS rejected terminal chunk cannot turn into successful EOF after repair");
    Ok(())
}

fn main() -> Outcome<()> {
    let root = std::env::args_os().nth(1).ok_or("pass a new disposable data directory")?;
    let root = Path::new(&root);
    fs::create_dir(root)?;
    unchanged(root)?;
    same_size_mutation(root, false)?;
    same_size_mutation(root, true)?;
    empty_growth(root)?;
    failed_growth_retry(root)?;
    Ok(())
}
