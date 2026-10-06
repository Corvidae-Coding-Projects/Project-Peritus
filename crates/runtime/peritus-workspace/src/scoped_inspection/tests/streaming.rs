//! Ordinary inspection crosses source and inclusion thresholds without weakening identity.

use super::*;
use sha2::{Digest as _, Sha256};

#[test]
fn ordinary_small_selection_hashes_a_source_above_64_mib() {
    let (directory, reader) = fixture();
    let file = fs::File::create(directory.path().join("large")).unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    let observed = reader
        .read_file(
            &path("large"),
            FileReadSelection::bytes(64 * 1024 * 1024, 64 * 1024 * 1024 + 1).unwrap(),
            1,
        )
        .expect("small selection has no full-source ceiling");
    assert_eq!(observed.bytes(), &[0]);
    assert_eq!(observed.source_bytes(), 64 * 1024 * 1024 + 1);
    assert_eq!(observed.range(), (64 * 1024 * 1024, 64 * 1024 * 1024 + 1));
    let mut expected = Sha256::new();
    let chunk = vec![0; 64 * 1024];
    for _ in 0..1024 {
        expected.update(&chunk);
    }
    expected.update([0]);
    assert_eq!(observed.source_digest(), Sha256Digest::new(expected.finalize().into()));
}

#[test]
fn ordinary_explicit_buffer_capacity_has_no_compiled_8_mib_ceiling() {
    let (directory, reader) = fixture();
    let source = vec![19; 8 * 1024 * 1024 + 1];
    fs::write(directory.path().join("large"), &source).unwrap();
    let observed = reader
        .read_file(&path("large"), FileReadSelection::all(), source.len() as u64)
        .expect("caller owns its complete buffer capacity");
    assert_eq!(observed.bytes(), source);
    assert_eq!(observed.source_digest(), peritus_codec::sha256(&source));
}

#[test]
fn inspection_selection_constructors_only_reject_invalid_range_shapes() {
    assert!(FileReadSelection::bytes(64 * 1024 * 1024, u64::MAX).is_ok());
    assert!(FileReadSelection::lines(67_108_865, u32::MAX).is_ok());
    assert!(FileReadSelection::lines_u64(u64::from(u32::MAX) + 1, u64::MAX).is_ok());
    assert!(FileReadSelection::bytes(0, 0).is_err());
    assert!(FileReadSelection::bytes(2, 1).is_err());
    assert!(FileReadSelection::lines(0, 1).is_err());
    assert!(FileReadSelection::lines(2, 1).is_err());
}
