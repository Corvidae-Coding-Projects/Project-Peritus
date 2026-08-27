//! Streaming, integrity, idempotency, and bound tests.

mod support;

use std::fs;

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, ErrorCode, MediaType, Publication,
    StoreConfig, WriteRequest,
};

use support::{digest, event, object_path, request, store};

#[test]
fn chunked_streaming_finalizes_exact_bytes_and_durable_metadata() {
    let (directory, store) = store(1024, 4096);
    let content = b"streamed in several chunks";
    let mut writer = store.begin_write(request(content, 64, 1)).expect("writer begins");
    writer.write_chunk(b"streamed ").expect("first chunk");
    writer.write_chunk(b"in several ").expect("second chunk");
    writer.write_chunk(b"chunks").expect("third chunk");
    let finalized = writer.finalize().expect("artifact finalizes");

    assert_eq!(finalized.digest(), digest(content));
    assert_eq!(finalized.size(), content.len() as u64);
    assert_eq!(finalized.publication(), Publication::New);
    assert_eq!(
        fs::read(object_path(directory.path(), digest(content))).expect("object bytes"),
        content
    );
    assert!(store.verify(digest(content)).expect("object verifies").is_referenceable());
    assert_eq!(store.read(digest(content), 64).expect("bounded verified read"), content);
    assert_eq!(
        store.read(digest(content), 4).expect_err("read bound is enforced").code(),
        ErrorCode::ByteLimitExceeded,
    );

    drop(store);
    let reopened =
        ArtifactStore::open(StoreConfig::new(directory.path(), 1024, 4096).expect("config"))
            .expect("restart opens");
    assert_eq!(
        reopened.metadata(digest(content)).expect("metadata query").expect("metadata").size(),
        content.len() as u64,
    );
}

#[test]
fn owned_handles_stream_across_calls_without_borrowing_the_store() {
    let (_directory, store) = store(64, 256);
    let content = b"owned streaming handle";
    let mut writer = store.begin_owned_write(request(content, 64, 7)).expect("owned writer begins");
    let _store_remains_borrowable = store.root();
    writer.write_chunk(b"owned ").expect("first owned chunk");
    writer.write_chunk(b"streaming handle").expect("second owned chunk");
    let finalized = store.complete_write(writer).expect("store completes owned writer");
    assert_eq!(finalized.digest(), digest(content));

    let mut reader = store.open_read(digest(content)).expect("owned reader opens");
    assert_eq!(reader.metadata().size(), content.len() as u64);
    let first = reader.read_chunk(5).expect("first read").expect("first chunk");
    assert_eq!(first.offset(), 0);
    assert_eq!(first.bytes(), b"owned");
    let second = reader.read_chunk(64).expect("second read").expect("second chunk");
    assert_eq!(second.offset(), 5);
    assert_eq!(second.bytes(), b" streaming handle");
    assert_eq!(reader.remaining_bytes(), 0);
    assert!(reader.read_chunk(1).expect("exact completion").is_none());
}

#[test]
fn exact_limit_succeeds_and_one_over_is_rejected_without_partial_chunk() {
    let (_directory, store) = store(8, 64);
    let content = b"12345678";
    let mut exact = store.begin_write(request(content, 8, 1)).expect("exact writer");
    exact.write_chunk(content).expect("exact limit accepted");
    exact.finalize().expect("exact limit finalizes");

    let other = b"abcdefgh";
    let mut over = store.begin_write(request(other, 8, 2)).expect("over writer begins");
    over.write_chunk(b"1234567").expect("prefix accepted");
    let error = over.write_chunk(b"89").expect_err("one-over rejected");
    assert_eq!(error.code(), ErrorCode::ByteLimitExceeded);
    assert_eq!(over.bytes_written(), 7);
    assert_eq!(
        over.finalize().expect_err("failed writer cannot finalize").code(),
        ErrorCode::InvalidWriteRequest
    );
}

#[test]
fn exact_size_and_digest_are_checked_independently() {
    let (_directory, store) = store(64, 256);
    let expected = b"four";
    let mut short = store.begin_write(request(expected, 16, 1)).expect("writer");
    short.write_chunk(b"for").expect("short bytes stream");
    assert_eq!(short.finalize().expect_err("size mismatch").code(), ErrorCode::SizeMismatch);

    let wrong_digest = ArtifactDigest::new([0x55; 32]);
    let request = WriteRequest::new(
        wrong_digest,
        4,
        16,
        MediaType::new("text/plain").expect("media type"),
        EncryptionMetadata::unencrypted(),
        event(2),
    );
    let mut wrong = store.begin_write(request).expect("writer");
    wrong.write_chunk(expected).expect("bytes stream");
    assert_eq!(wrong.finalize().expect_err("digest mismatch").code(), ErrorCode::DigestMismatch);
}

#[test]
fn duplicate_content_is_idempotent_but_corrupted_destination_is_terminal() {
    let (directory, store) = store(128, 512);
    let content = b"deduplicated object";
    let mut first = store.begin_write(request(content, 64, 1)).expect("first writer");
    first.write_chunk(content).expect("first bytes");
    assert_eq!(first.finalize().expect("first finalize").publication(), Publication::New);

    let mut duplicate = store.begin_write(request(content, 64, 2)).expect("second writer");
    duplicate.write_chunk(content).expect("duplicate bytes");
    assert_eq!(
        duplicate.finalize().expect("duplicate is idempotent").publication(),
        Publication::Existing,
    );

    fs::write(object_path(directory.path(), digest(content)), b"same byte count!!!")
        .expect("inject destination corruption");
    let mut collision = store.begin_write(request(content, 64, 3)).expect("collision writer");
    collision.write_chunk(content).expect("collision bytes");
    assert_eq!(
        collision.finalize().expect_err("corruption is rejected").code(),
        ErrorCode::CorruptObject,
    );
}

#[test]
fn dropping_partial_writer_removes_exclusive_temporary_file() {
    let (directory, store) = store(64, 256);
    let content = b"partial";
    let mut writer = store.begin_write(request(content, 16, 1)).expect("writer");
    writer.write_chunk(b"par").expect("partial bytes");
    drop(writer);
    assert_eq!(
        fs::read_dir(directory.path().join("temporary")).expect("temp directory").count(),
        0
    );
}

#[test]
fn invalid_config_and_writer_bounds_are_rejected() {
    let directory = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        StoreConfig::new(directory.path(), 0, 1).expect_err("zero limit").code(),
        ErrorCode::InvalidConfiguration,
    );
    let (_root, store) = store(16, 32);
    let invalid = request(b"content", 17, 1);
    let Err(error) = store.begin_write(invalid) else {
        panic!("configured limit must be enforced");
    };
    assert_eq!(error.code(), ErrorCode::InvalidWriteRequest);
}

#[test]
fn encryption_binding_metadata_round_trips_durably() {
    let (_directory, store) = store(128, 512);
    let content = b"ciphertext";
    let encryption = EncryptionMetadata::envelope(
        "aes-256-gcm",
        peritus_types::Sha256Digest::new([0x31; 32]),
        peritus_types::Sha256Digest::new([0x32; 32]),
    )
    .expect("encryption metadata");
    let request = WriteRequest::new(
        digest(content),
        content.len() as u64,
        32,
        MediaType::new("application/vnd.peritus.encrypted").expect("media type"),
        encryption.clone(),
        event(4),
    );
    let mut writer = store.begin_write(request).expect("writer");
    writer.write_chunk(content).expect("ciphertext");
    writer.finalize().expect("finalize");
    let stored = store.metadata(digest(content)).expect("query").expect("record");
    assert_eq!(stored.encryption(), &encryption);
    assert_eq!(stored.encryption().algorithm(), Some("aes-256-gcm"));
    assert!(stored.encryption().is_encrypted());
    assert_eq!(
        EncryptionMetadata::envelope(
            "contains spaces",
            peritus_types::Sha256Digest::new([1; 32]),
            peritus_types::Sha256Digest::new([2; 32]),
        )
        .expect_err("invalid algorithm")
        .code(),
        ErrorCode::InvalidMetadata,
    );
}
