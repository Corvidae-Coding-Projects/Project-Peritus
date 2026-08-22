//! Exact-representation tests for SHA-256 digest bytes.

use peritus_types::Sha256Digest;

#[test]
fn digest_preserves_every_exact_byte_pattern() {
    let zero = [0; Sha256Digest::LENGTH];
    assert_eq!(Sha256Digest::new(zero).into_bytes(), zero);

    let mut patterned = [0; Sha256Digest::LENGTH];
    for (index, byte) in patterned.iter_mut().enumerate() {
        *byte = u8::try_from(index).expect("digest index fits in u8");
    }
    let digest = Sha256Digest::new(patterned);
    assert_eq!(digest.as_bytes(), &patterned);
    assert_eq!(digest.into_bytes(), patterned);
}
