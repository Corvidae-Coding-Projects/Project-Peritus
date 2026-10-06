use super::*;

fn paged(paths: impl Iterator<Item = String>) -> Manifest {
    let file =
        FinalFile::new(Vec::new(), FileMode::Regular, LineEndingPolicy::Preserve).expect("file");
    let patch = PatchSet::new(
        WorkspaceId::new([41; 16]).expect("workspace"),
        Generation::first(),
        RevisionNumber::first(),
        paths
            .map(|path| {
                PatchOperation::create(WorkspacePath::new(path).expect("path"), file.clone())
            })
            .collect(),
    )
    .expect("metadata authority");
    let manifest = Manifest::from_patch(&patch, Vec::new());
    assert_eq!(manifest.schema, 4);
    manifest
}

#[test]
fn ordinary_manifest_paging_crosses_old_count_and_message_boundaries() {
    let prefix = vec!["d".repeat(255); 15].join("/");
    for manifest in [
        paged((0..65_536).map(|index| format!("file-{index:05}"))),
        paged((0..5_000).map(|index| format!("{prefix}/file-{index:05}"))),
    ] {
        let encoded = manifest.encode().expect("encode");
        if manifest.entries.len() == 5_000 {
            assert!(encoded.len() > CodecLimits::LEGACY_V1.max_payload_bytes);
        }
        assert_eq!(Manifest::decode(&encoded).expect("all physical pages"), manifest);
        assert_eq!(Manifest::decode(&encoded).expect("decode").encode().expect("encode"), encoded);
    }
}

fn rehash(bytes: &mut [u8]) {
    let payload = bytes.len() - 32;
    let checksum = peritus_codec::sha256(&bytes[..payload]);
    bytes[payload..].copy_from_slice(checksum.as_bytes());
}

#[test]
fn malformed_page_counts_checksums_and_truncated_manifests_reject() {
    let manifest = paged((0..1_025).map(|index| format!("file-{index:05}")));
    let encoded = manifest.encode().expect("encode");
    let count_offset = MAGIC.len() + 2 + 1 + 16 + 8 + 8 + 32;
    for offset in [count_offset, count_offset + 8] {
        let mut forged = encoded.clone();
        forged[offset..offset + 8].copy_from_slice(&u64::MAX.to_be_bytes());
        rehash(&mut forged);
        assert!(Manifest::decode(&forged).is_err());
    }
    let mut corrupt = encoded.clone();
    // Retain valid whole-manifest checksum while violating the first physical page's checksum.
    corrupt[count_offset + 8 + 8 + 4 + 10] ^= 1;
    rehash(&mut corrupt);
    assert!(Manifest::decode(&corrupt).is_err());
    for length in [0, 32, 100, encoded.len() - 1] {
        assert!(Manifest::decode(&encoded[..length]).is_err());
    }
}

#[test]
fn paged_implicit_directories_exceed_the_legacy_collection_capacity() {
    let mut manifest =
        paged((0..1_024).map(|index| format!("p{index}/{}/file", vec!["d"; 63].join("/"))));
    let mut directories: Vec<_> = (0..1_024)
        .flat_map(|index| {
            (0..64).map(move |depth| {
                (depth, WorkspacePath::new(format!("p{index}{}", "/d".repeat(depth))).unwrap())
            })
        })
        .collect();
    directories.sort_unstable();
    manifest.created_directories = directories.into_iter().map(|(_, path)| path).collect();
    assert_eq!(manifest.created_directories.len(), 65_536);
    let encoded = manifest.encode().expect("all directory pages");
    assert_eq!(Manifest::decode(&encoded).expect("all directory identities"), manifest);
}

#[test]
fn paged_manifest_preserves_storage_pressure_as_a_retryable_io_error() {
    use std::{error::Error as _, io};
    struct Full {
        remaining: usize,
    }
    impl io::Write for Full {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::from(io::ErrorKind::StorageFull));
            }
            let count = bytes.len().min(self.remaining);
            self.remaining -= count;
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let manifest = paged((0..2_000).map(|index| format!("file-{index:05}")));
    let error = manifest
        .write_to(&mut Full { remaining: 64 * 1024 + 150 })
        .expect_err("storage pressure after a physical page");
    assert_eq!(error.code(), ErrorCode::Io);
    assert_eq!(error.recovery_class(), RecoveryClass::Retry);
    assert_eq!(
        error.source().unwrap().downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::StorageFull
    );
    let encoded = manifest.encode().expect("same logical manifest remains publishable");
    assert_eq!(Manifest::decode(&encoded).expect("retry"), manifest);
}
