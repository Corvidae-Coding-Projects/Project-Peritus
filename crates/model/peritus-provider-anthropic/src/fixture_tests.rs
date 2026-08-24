//! Versioned official-contract fixture inventory and digest verification.

#[test]
fn fixture_manifest_is_complete_and_sha256_sums_match() {
    let names = [
        "auth_error.json",
        "corrupt.sse",
        "error_after_success.sse",
        "golden_request.json",
        "incomplete.sse",
        "rate_error.json",
        "runtime_auth_false.json",
        "runtime_auth_true.json",
        "runtime_error.json",
        "runtime_incomplete.json",
        "runtime_malformed.json",
        "runtime_success.json",
        "runtime_tool.json",
        "text.sse",
        "tool_thinking.sse",
        "transient_error.json",
        "unknown_ancillary.sse",
        "unknown_critical.sse",
    ];
    let manifest = String::from_utf8(crate::test_support::fixture("MANIFEST"))
        .expect("fixture manifest is UTF-8");
    let sums = String::from_utf8(crate::test_support::fixture("SHA256SUMS"))
        .expect("fixture digest inventory is UTF-8");
    for name in names {
        let bytes = crate::test_support::fixture(name);
        assert!(manifest.lines().any(|line| line.starts_with(&format!("{name}="))));
        let line = sums
            .lines()
            .find(|line| line.ends_with(&format!("  {name}")))
            .expect("fixture digest inventory entry");
        let expected = decode_digest(&line[..64]);
        assert_eq!(peritus_codec::sha256(&bytes).into_bytes(), expected, "{name}");
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
        _ => panic!("invalid checked fixture digest"),
    }
}
