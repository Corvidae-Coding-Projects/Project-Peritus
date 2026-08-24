//! Canonical v1 decoder, compatibility corpus, and corruption contracts.

mod support;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use peritus_model_protocol::{
    OutputLimitEnforcement, ProtocolErrorKind, ProtocolLimits, RequestId, WireDialect,
    decode_request,
};

const ACCEPTED: [&str; 3] = ["minimal.hex", "realistic.hex", "boundary.hex"];
const REJECTED: [&str; 5] = [
    "corrupt-truncated.hex",
    "unknown-tag.hex",
    "trailing-field.hex",
    "invalid-option.hex",
    "invalid-boolean.hex",
];

#[test]
fn accepted_fixtures_decode_and_reencode_byte_identically() {
    let profile = support::profile();
    let expected = [
        support::minimal_request(&profile),
        support::realistic_request(&profile),
        support::boundary_request(&profile),
    ];
    for (name, expected) in ACCEPTED.into_iter().zip(expected) {
        let bytes = fixture(name);
        assert_eq!(expected.canonical_bytes().expect("encode"), bytes, "fixture drift: {name}");
        let decoded =
            decode_request(&bytes, &profile, support::request_id(), ProtocolLimits::PRODUCTION)
                .unwrap_or_else(|error| panic!("decode {name}: {error}"));
        assert_eq!(decoded, expected);
        assert_eq!(decoded.canonical_bytes().expect("re-encode"), bytes);
    }
}

#[test]
fn runtime_dialects_have_stable_closed_tags_and_advisory_profiles() {
    for (name, dialect) in [
        ("codex-runtime.hex", WireDialect::OpenAiCodexRuntime),
        ("claude-runtime.hex", WireDialect::AnthropicClaudeRuntime),
    ] {
        let profile = support::runtime_profile(dialect);
        assert_eq!(profile.output_limit_enforcement(), OutputLimitEnforcement::Advisory);
        let expected = support::minimal_request(&profile);
        let bytes = fixture(name);
        assert_eq!(expected.canonical_bytes().expect("encode"), bytes, "fixture drift: {name}");
        let decoded =
            decode_request(&bytes, &profile, support::request_id(), ProtocolLimits::PRODUCTION)
                .unwrap_or_else(|error| panic!("decode {name}: {error}"));
        assert_eq!(decoded, expected);
    }
}

#[test]
fn corpus_inventory_and_binary_digests_are_complete_and_stable() {
    let root = fixture_root();
    let manifest = fs::read_to_string(root.join("MANIFEST")).expect("manifest");
    let manifest_names: BTreeSet<_> = manifest
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('|').next().expect("manifest name").trim().to_owned())
        .collect();
    let directory_names: BTreeSet<_> = fs::read_dir(&root)
        .expect("fixture directory")
        .map(|entry| entry.expect("fixture entry").file_name().to_string_lossy().into_owned())
        .filter(|name| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("hex"))
        })
        .collect();
    assert_eq!(manifest_names, directory_names);

    let sums = fs::read_to_string(root.join("SHA256SUMS")).expect("digest inventory");
    let mut digest_names = BTreeSet::new();
    for line in sums.lines().filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let (expected, name) = line.split_once("  ").expect("digest record");
        let bytes = fixture(name);
        assert_eq!(hex(peritus_codec::sha256(&bytes).as_bytes()), expected, "digest: {name}");
        assert!(digest_names.insert(name.to_owned()), "duplicate digest entry");
    }
    assert_eq!(digest_names, directory_names);
}

#[test]
fn corrupt_closed_tags_unknown_tags_and_trailing_fields_are_rejected() {
    let profile = support::profile();
    for name in REJECTED {
        assert!(
            decode_request(
                &fixture(name),
                &profile,
                support::request_id(),
                ProtocolLimits::PRODUCTION,
            )
            .is_err(),
            "{name} unexpectedly decoded"
        );
    }
}

#[test]
fn every_truncated_prefix_and_oversized_collection_count_is_rejected() {
    let profile = support::profile();
    let bytes = fixture("minimal.hex");
    for end in 0..bytes.len() {
        assert!(
            decode_request(
                &bytes[..end],
                &profile,
                support::request_id(),
                ProtocolLimits::PRODUCTION,
            )
            .is_err(),
            "truncated prefix {end} decoded"
        );
    }
    let mut oversized = bytes;
    let selected = selected_mask_offset(&oversized);
    let message_count = selected + 8 + 8 + 8 + 4 + 4 + 8;
    oversized[message_count..message_count + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode(&oversized).is_err());
}

#[test]
fn version_and_every_profile_binding_field_are_checked_before_reconstruction() {
    let base = fixture("minimal.hex");
    let mut cases = Vec::new();

    let mut version = base.clone();
    version[4..6].copy_from_slice(&2_u16.to_be_bytes());
    assert_eq!(
        decode(&version).expect_err("version").kind(),
        ProtocolErrorKind::UnsupportedVersion
    );

    let mut profile_id = base.clone();
    profile_id[8] ^= 1;
    cases.push(profile_id);
    let mut revision = base.clone();
    revision[31] ^= 1;
    cases.push(revision);
    let mut provider = base.clone();
    let provider_start = find(&provider, b"fixture-provider");
    provider[provider_start] = b'F';
    cases.push(provider);
    let mut dialect = base.clone();
    let provider_start = find(&dialect, b"fixture-provider");
    dialect[provider_start + b"fixture-provider".len()] = 6;
    cases.push(dialect);
    let mut model = base;
    let model_start = find(&model, b"fixture-model-v1");
    model[model_start] = b'F';
    cases.push(model);

    for bytes in cases {
        assert_eq!(
            decode(&bytes).expect_err("profile drift").kind(),
            ProtocolErrorKind::InvalidRequest
        );
    }
}

#[test]
fn invalid_nested_values_and_complete_request_validation_fail_closed() {
    let mut media_mismatch = fixture("realistic.hex");
    let media = find(&media_mismatch, b"\x02\x01\0\0\0\x09image/png");
    media_mismatch[media + 1] = 2;
    assert!(decode(&media_mismatch).is_err());

    let mut unknown_capability = fixture("minimal.hex");
    let selected = selected_mask_offset(&unknown_capability);
    unknown_capability[selected..selected + 8].copy_from_slice(&(1_u64 << 63).to_be_bytes());
    assert!(decode(&unknown_capability).is_err());

    let mut invalid_generation = fixture("minimal.hex");
    let tail = find(&invalid_generation, &minimal_tail());
    invalid_generation[tail + 4..tail + 12].fill(0);
    assert!(decode(&invalid_generation).is_err());
}

#[test]
fn caller_request_id_is_restored_but_excluded_from_canonical_bytes() {
    let profile = support::profile();
    let bytes = fixture("minimal.hex");
    let request_id = RequestId::new("a-different-caller-id".to_owned()).expect("request ID");
    let decoded =
        decode_request(&bytes, &profile, request_id, ProtocolLimits::PRODUCTION).expect("decode");
    assert_eq!(decoded.request_id().expose_for_wire(), "a-different-caller-id");
    assert_eq!(decoded.canonical_bytes().expect("encode"), bytes);
}

fn decode(
    bytes: &[u8],
) -> Result<peritus_model_protocol::ModelRequest, peritus_model_protocol::ProtocolError> {
    decode_request(bytes, &support::profile(), support::request_id(), ProtocolLimits::PRODUCTION)
}

fn selected_mask_offset(bytes: &[u8]) -> usize {
    find(bytes, b"fixture-model-v1") + b"fixture-model-v1".len()
}

const fn minimal_tail() -> [u8; 27] {
    [2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]
}

fn find(bytes: &[u8], needle: &[u8]) -> usize {
    bytes.windows(needle.len()).position(|window| window == needle).expect("fixture pattern")
}

fn fixture(name: &str) -> Vec<u8> {
    let text = fs::read_to_string(fixture_root().join(name)).expect("fixture");
    let digits: Vec<_> = text.bytes().filter(|byte| !byte.is_ascii_whitespace()).collect();
    assert_eq!(digits.len() % 2, 0, "hex fixture length");
    digits.chunks_exact(2).map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1])).collect()
}

fn fixture_root() -> PathBuf {
    PathBuf::from("fixtures/v1")
}

fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => panic!("invalid hex fixture"),
    }
}

fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("string write");
    }
    output
}
