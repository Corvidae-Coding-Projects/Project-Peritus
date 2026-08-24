use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

#[test]
fn manifest_and_digests_cover_the_complete_immutable_corpus() {
    let root = fixture_root();
    let manifest = fs::read_to_string(root.join("MANIFEST")).expect("fixture manifest");
    let manifest_names: BTreeSet<_> = manifest
        .lines()
        .skip_while(|line| !line.starts_with("reviewed="))
        .skip(1)
        .map(|line| line.split_once('=').expect("manifest entry").0.to_owned())
        .collect();
    let directory_names: BTreeSet<_> = fs::read_dir(&root)
        .expect("fixture directory")
        .map(|entry| entry.expect("fixture entry").file_name().to_string_lossy().into_owned())
        .filter(|name| !matches!(name.as_str(), "MANIFEST" | "SHA256SUMS"))
        .collect();
    assert_eq!(manifest_names, directory_names);

    let inventory = fs::read_to_string(root.join("SHA256SUMS")).expect("digest inventory");
    let mut digest_names = BTreeSet::new();
    for line in inventory.lines().filter(|line| !line.is_empty()) {
        let (expected, name) = line.split_once("  ").expect("digest entry");
        let bytes = fs::read(root.join(name)).expect("fixture bytes");
        assert_eq!(hex(peritus_codec::sha256(&bytes).as_bytes()), expected, "digest: {name}");
        assert!(digest_names.insert(name.to_owned()), "duplicate digest entry");
    }
    assert_eq!(digest_names, directory_names);
}

fn fixture_root() -> PathBuf {
    PathBuf::from("fixtures/v1")
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}
