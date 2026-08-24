//! Versioned official-contract fixture inventory tests.

use std::collections::BTreeMap;

use crate::test_support::fixture;

#[test]
fn manifest_is_stable_v1_only_and_covers_failure_contracts() {
    let manifest = String::from_utf8(fixture("MANIFEST")).expect("manifest UTF-8");
    assert!(manifest.contains("reviewed=2026-08-24"));
    assert!(manifest.contains("api-version=v1"));
    assert!(!manifest.contains("v1beta"));
    for required in [
        "golden_interactions_minimal.json",
        "golden_generate_minimal.json",
        "interactions_tool_thinking.sse",
        "generate_tool_thinking.sse",
        "generate_error_after_success.sse",
        "generate_unknown_ancillary.sse",
        "corrupt.sse",
        "incomplete.sse",
        "unknown_ancillary.sse",
        "unknown_critical.sse",
        "error_after_success.sse",
        "auth_error.json",
        "rate_error.json",
        "generate_rate_error.json",
        "quota_error.json",
        "transient_error.json",
    ] {
        assert!(manifest.lines().any(|line| line.starts_with(required)), "{required}");
    }
}

#[test]
fn every_manifest_artifact_has_an_exact_sha256_inventory_entry() {
    let manifest = String::from_utf8(fixture("MANIFEST")).expect("manifest UTF-8");
    let inventory = String::from_utf8(fixture("SHA256SUMS")).expect("inventory UTF-8");
    let digests = inventory
        .lines()
        .map(|line| {
            let (digest, name) = line.split_once("  ").expect("digest record");
            (name, digest)
        })
        .collect::<BTreeMap<_, _>>();
    for name in manifest.lines().filter_map(|line| {
        let (name, _description) = line.split_once('=')?;
        std::path::Path::new(name)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("json") || extension.eq_ignore_ascii_case("sse")
            })
            .then_some(name)
    }) {
        let expected = decode_digest(digests.get(name).copied().expect("inventory entry"));
        assert_eq!(peritus_codec::sha256(&fixture(name)).into_bytes(), expected, "{name}");
    }
}

fn decode_digest(hex: &str) -> [u8; 32] {
    let mut digest = [0_u8; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        digest[index] = nibble(pair[0]) * 16 + nibble(pair[1]);
    }
    digest
}

const fn nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => panic!("invalid fixture digest"),
    }
}
