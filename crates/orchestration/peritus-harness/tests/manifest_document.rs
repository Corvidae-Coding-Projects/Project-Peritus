//! Strict manifest parsing, inventory, and C1 loading tests.

#![allow(clippy::unwrap_used, reason = "fixed checked E1 test corpus")]

use core::fmt::Write as _;

use peritus_harness::domain::HarnessLimits;
use peritus_harness::{HarnessManifest, ManifestErrorKind};

fn valid_manifest() -> Vec<u8> {
    let content = b"exact harness component\n";
    let mut digest = String::with_capacity(64);
    for byte in peritus_codec::sha256(content).as_bytes() {
        write!(digest, "{byte:02x}").expect("writing to a String cannot fail");
    }
    format!(
        r#"schema_version = 1
lineage_seed = "{lineage}"
provider_features = []
platform_features = []

[limits]
components = 1

[[components]]
id = "base.instructions"
kind = "base_instruction_fragment"
schema_version = 1
source_path = ".peritus-harness/components/base.txt"
target_path = "runtime/base.txt"
media_type = "text/plain"
byte_length = 24
content_sha256 = "{digest}"
owner = "test-owner"
provenance = "fixed manifest fixture"
dependencies = []
declared_authority = []
protection_class = "evolvable"

[components.compatibility]
minimum_schema = 1
maximum_schema = 1
provider_features = []
platform_features = []
"#,
        lineage = "11".repeat(32)
    )
    .into_bytes()
}

#[test]
fn strict_manifest_retains_exact_bytes_and_checked_declaration() {
    let bytes = valid_manifest();
    let manifest = HarnessManifest::parse(&bytes, HarnessLimits::compiled()).unwrap();
    assert_eq!(manifest.exact_bytes(), bytes);
    assert_eq!(manifest.digest().digest(), peritus_codec::sha256(&bytes));
    assert_eq!(manifest.declarations().len(), 1);
    assert_eq!(manifest.declarations()[0].target_path().as_str(), "runtime/base.txt");
}

#[test]
fn unknown_manifest_field_and_noncanonical_digest_fail_closed() {
    let mut unknown = valid_manifest();
    unknown.extend_from_slice(b"\nunknown = true\n");
    assert_eq!(
        HarnessManifest::parse(&unknown, HarnessLimits::compiled()).unwrap_err().kind(),
        ManifestErrorKind::InvalidToml,
    );
    let invalid = String::from_utf8(valid_manifest())
        .unwrap()
        .replace("content_sha256 = \"", "content_sha256 = \"A");
    assert_eq!(
        HarnessManifest::parse(invalid.as_bytes(), HarnessLimits::compiled()).unwrap_err().kind(),
        ManifestErrorKind::InvalidDigest,
    );
}
