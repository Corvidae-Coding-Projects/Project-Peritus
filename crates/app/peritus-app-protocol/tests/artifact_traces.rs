//! Artifact transfer ordering and conservation integration tests.

mod support;

use peritus_app_protocol::{
    ArtifactCancellation, ArtifactChunk, ArtifactMetadata, ArtifactTerminalDisposition,
    ArtifactTransferErrorKind, ArtifactTransferPhase, ArtifactTransferState, CanonicalMediaType,
    CorrelationId, TransferId,
};
use peritus_types::{ArtifactId, Sha256Digest};
use support::fixture_id;

fn metadata_for(
    transfer_byte: u8,
    artifact_byte: u8,
    byte_size: u64,
    digest: Sha256Digest,
) -> ArtifactMetadata {
    ArtifactMetadata::new(
        fixture_id(transfer_byte, TransferId::new),
        fixture_id(artifact_byte, ArtifactId::new),
        byte_size,
        CanonicalMediaType::new("application/octet-stream".to_owned(), 64)
            .expect("canonical media type"),
        digest,
        3,
        4,
    )
    .expect("checked artifact metadata")
}

fn chunk(metadata: &ArtifactMetadata, ordinal: u64, offset: u64, bytes: &[u8]) -> ArtifactChunk {
    ArtifactChunk::new(
        metadata.transfer_id(),
        metadata.artifact_id(),
        ordinal,
        offset,
        bytes.to_vec(),
        4,
    )
    .expect("bounded nonempty chunk")
}

#[test]
fn chunks_conserve_order_size_digest_and_terminal_state() {
    let digest = Sha256Digest::new([9; 32]);
    let metadata = metadata_for(30, 31, 5, digest);
    let mut transfer =
        ArtifactTransferState::new(metadata.clone(), 4).expect("metadata fits transfer limits");

    assert_eq!(
        transfer
            .accept_chunk(&chunk(&metadata, 1, 0, b"ab"))
            .expect_err("ordinal must start at zero")
            .kind(),
        ArtifactTransferErrorKind::UnexpectedOrdinal,
    );
    assert_eq!(
        transfer
            .accept_chunk(&chunk(&metadata, 0, 1, b"ab"))
            .expect_err("offset must equal conserved length")
            .kind(),
        ArtifactTransferErrorKind::UnexpectedOffset,
    );
    assert_eq!(transfer.accept_chunk(&chunk(&metadata, 0, 0, b"ab")).unwrap(), 2);
    assert_eq!(transfer.next_ordinal(), 1);
    assert_eq!(
        transfer.complete(digest).expect_err("partial transfer cannot complete").kind(),
        ArtifactTransferErrorKind::Incomplete,
    );
    assert_eq!(
        transfer
            .accept_chunk(&chunk(&metadata, 1, 2, b"cdef"))
            .expect_err("chunk cannot exceed declared total size")
            .kind(),
        ArtifactTransferErrorKind::SizeOverflow,
    );
    assert_eq!(transfer.accept_chunk(&chunk(&metadata, 1, 2, b"cde")).unwrap(), 5);
    assert_eq!(transfer.conserved_bytes(), metadata.byte_size());
    transfer.complete(digest).expect("exact size and digest complete");
    assert!(
        matches!(transfer.phase(), ArtifactTransferPhase::Completed(value) if *value == digest)
    );
    assert_eq!(
        transfer
            .accept_chunk(&chunk(&metadata, 2, 5, b"f"))
            .expect_err("completed transfer is terminal")
            .kind(),
        ArtifactTransferErrorKind::AlreadyTerminal,
    );

    let mismatch_metadata = metadata_for(32, 33, 2, digest);
    let mut mismatch = ArtifactTransferState::new(mismatch_metadata.clone(), 4).unwrap();
    mismatch
        .accept_chunk(&chunk(&mismatch_metadata, 0, 0, b"ab"))
        .expect("complete byte conservation");
    assert_eq!(
        mismatch
            .complete(Sha256Digest::new([8; 32]))
            .expect_err("wrong observed digest is terminal failure")
            .kind(),
        ArtifactTransferErrorKind::DigestMismatch,
    );
    assert!(matches!(mismatch.phase(), ArtifactTransferPhase::Failed(_)));

    let zero_metadata = metadata_for(34, 35, 0, Sha256Digest::new([0; 32]));
    let mut zero = ArtifactTransferState::new(zero_metadata, 4).unwrap();
    zero.complete(Sha256Digest::new([0; 32]))
        .expect("zero-sized artifact completes without chunks");

    let cancelled_metadata = metadata_for(36, 37, 1, digest);
    let mut cancelled = ArtifactTransferState::new(cancelled_metadata.clone(), 4).unwrap();
    let cancellation = ArtifactCancellation::new(
        cancelled_metadata.transfer_id(),
        cancelled_metadata.artifact_id(),
        fixture_id(38, CorrelationId::new),
    );
    assert_eq!(
        cancelled.cancel(cancellation).expect("cancellation applies"),
        ArtifactTerminalDisposition::Applied,
    );
    assert_eq!(
        cancelled.cancel(cancellation).expect("same cancellation repeats"),
        ArtifactTerminalDisposition::Repeated,
    );
    assert_eq!(
        cancelled
            .accept_chunk(&chunk(&cancelled_metadata, 0, 0, b"a"))
            .expect_err("cancelled transfer rejects chunks")
            .kind(),
        ArtifactTransferErrorKind::AlreadyTerminal,
    );
}
