//! Extended path records cross physical pages without changing legacy representations.

use super::*;

#[test]
fn extended_path_record_crosses_byte_pages_and_reopens_exactly() {
    let parent = vec!["d".repeat(200); 400].join("/");
    let patch = patch(std::iter::once(format!("{parent}/saved")));
    let manifest = Manifest::from_patch(&patch, vec![WorkspacePath::new(parent).unwrap()]);
    assert_eq!(manifest.schema, 5);
    let bytes = manifest.encode().unwrap();
    assert!(bytes.len() > 2 * 64 * 1024);
    assert_eq!(Manifest::decode(&bytes).unwrap(), manifest);
    for length in [0, 8, 32, bytes.len() - 1] {
        assert!(Manifest::decode(&bytes[..length]).is_err());
    }
    let mut corrupt = bytes;
    corrupt[100] ^= 1;
    assert!(Manifest::decode(&corrupt).is_err());
    reseal(&mut corrupt);
    assert!(Manifest::decode(&corrupt).is_err());
}

#[test]
fn extended_manifest_rejects_foreign_platforms_and_legacy_reinterpretation() {
    let patch = patch(std::iter::once(vec!["d"; 257].join("/")));
    let manifest = Manifest::from_patch(&patch, Vec::new());
    assert_eq!(manifest.schema, 5);
    let encoded = manifest.encode().unwrap();
    for schema in [1_u16, 2, 3, 4] {
        let mut bytes = encoded.clone();
        bytes[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&schema.to_be_bytes());
        reseal(&mut bytes);
        assert!(Manifest::decode(&bytes).is_err());
    }
    let mut bytes = encoded;
    bytes[MAGIC.len() + 2] = 255;
    reseal(&mut bytes);
    assert!(Manifest::decode(&bytes).is_err());
}

fn reseal(bytes: &mut [u8]) {
    let payload = bytes.len() - 32;
    let digest = peritus_codec::sha256(&bytes[..payload]);
    bytes[payload..].copy_from_slice(digest.as_bytes());
}
