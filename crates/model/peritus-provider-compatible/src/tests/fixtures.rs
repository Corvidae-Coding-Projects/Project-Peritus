use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use peritus_types::Sha256Digest;

use super::support::fixture;

#[test]
fn fixture_inventory_and_digests_are_complete_and_exact() {
    let root = Path::new("fixtures/v1");
    let manifest_bytes = fixture("MANIFEST");
    let manifest = core::str::from_utf8(&manifest_bytes).expect("UTF-8 manifest");
    let expected: BTreeSet<_> =
        manifest.lines().skip(1).filter(|line| !line.is_empty()).map(str::to_owned).collect();
    assert_eq!(manifest.lines().next(), Some("version=1"));
    let actual: BTreeSet<_> = fs::read_dir(root)
        .expect("fixture directory")
        .map(|entry| {
            entry.expect("fixture entry").file_name().into_string().expect("UTF-8 filename")
        })
        .collect();
    assert_eq!(actual, expected);

    let sums_bytes = fixture("SHA256SUMS");
    let sums = core::str::from_utf8(&sums_bytes).expect("UTF-8 digests");
    let inventory: BTreeMap<_, _> = sums
        .lines()
        .map(|line| {
            let (digest, name) = line.split_once("  ").expect("digest line");
            (name.to_owned(), digest.to_owned())
        })
        .collect();
    let expected_digests: BTreeSet<_> =
        expected.iter().filter(|name| name.as_str() != "SHA256SUMS").cloned().collect();
    assert_eq!(inventory.keys().cloned().collect::<BTreeSet<_>>(), expected_digests);
    for (name, digest) in inventory {
        let actual = peritus_codec::sha256(&fs::read(root.join(name)).expect("fixture"));
        assert_eq!(hex(actual), digest);
    }
}

fn hex(digest: Sha256Digest) -> String {
    let mut value = String::with_capacity(64);
    for byte in digest.into_bytes() {
        use core::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("string write");
    }
    value
}
