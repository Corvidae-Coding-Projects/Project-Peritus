//! Canonical compatibility manifest, digest, inventory, and coverage tests.

use peritus_test_support::{
    CompatibilityCoverage, CompatibilityPolicy, FixtureCase, FixtureCatalog, FixtureErrorKind,
    FixtureKind, FixturePath,
};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

struct TestRoot(PathBuf);

impl TestRoot {
    fn create(label: &str) -> Self {
        let path = std::env::temp_dir()
            .join(format!("peritus-test-fixtures-{}-{label}", std::process::id()));
        fs::create_dir(&path).expect("explicit fixture root must not exist");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let guarded = self
            .0
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("peritus-test-fixtures-"));
        if guarded {
            let _ignored = fs::remove_dir_all(&self.0);
        }
    }
}

fn digest_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn create_case(
    root: &Path,
    surface: &str,
    version: &str,
    case: &str,
    kind: &str,
    bytes: &[u8],
) -> PathBuf {
    let directory = root.join(surface).join(version).join(case);
    fs::create_dir_all(&directory).expect("case directory must be created");
    fs::write(directory.join("payload.bin"), bytes).expect("payload must be written");
    let manifest = format!(
        "schema = 1\nsurface = \"{surface}\"\nsurface_version = \"{version}\"\ncase = \"{case}\"\nkind = \"{kind}\"\n\n[[files]]\npath = \"payload.bin\"\nsha256 = \"{}\"\n",
        digest_hex(bytes)
    );
    fs::write(directory.join("fixture.toml"), manifest).expect("manifest must be written");
    directory
}

#[test]
fn catalog_preserves_exact_bytes_and_requires_all_fixture_kinds() {
    let root = TestRoot::create("coverage");
    create_case(root.path(), "journal", "1.0.0", "minimal", "minimal", b"{}\r\n");
    create_case(root.path(), "journal", "1.0.0", "realistic", "realistic", b"realistic\nbytes\n");
    create_case(root.path(), "journal", "1.0.0", "corrupt", "corrupt", &[0xff, 0x00, 0x80]);
    create_case(root.path(), "journal", "1.0.0", "adversarial", "adversarial", b"../escape\0");
    let catalog = FixtureCatalog::load(root.path()).expect("catalog must verify");
    let coverage = catalog
        .verify_compatibility_coverage(CompatibilityPolicy::RequireFixtures)
        .expect("all mandatory kinds exist");
    assert_eq!(coverage, CompatibilityCoverage::Covered);
    assert_eq!(catalog.cases().len(), 4);
    assert_eq!(catalog.cases()[0].manifest().kind(), FixtureKind::Adversarial);

    let minimal = catalog
        .cases()
        .iter()
        .find(|case| case.manifest().kind() == FixtureKind::Minimal)
        .expect("minimal case must exist");
    let path = FixturePath::new("payload.bin").expect("portable path");
    assert_eq!(minimal.read(&path).expect("exact bytes must verify"), b"{}\r\n");
}

#[test]
fn empty_catalog_is_allowed_only_by_explicit_pre_release_policy() {
    let root = TestRoot::create("empty");
    let catalog = FixtureCatalog::load(root.path()).expect("empty layout is structurally valid");
    let coverage = catalog
        .verify_compatibility_coverage(CompatibilityPolicy::AllowEmptyPreRelease)
        .expect("explicit pre-release policy permits empty catalog");
    assert_eq!(coverage, CompatibilityCoverage::EmptyPreRelease);
    let error = catalog
        .verify_compatibility_coverage(CompatibilityPolicy::RequireFixtures)
        .expect_err("released catalog must not be empty");
    assert_eq!(error.kind(), FixtureErrorKind::EmptyCatalog);
}

#[test]
fn empty_surface_and_version_levels_are_rejected() {
    let empty_surface = TestRoot::create("empty-surface");
    fs::create_dir(empty_surface.path().join("journal"))
        .expect("surface directory must be created");
    let error =
        FixtureCatalog::load(empty_surface.path()).expect_err("surface without versions must fail");
    assert_eq!(error.kind(), FixtureErrorKind::IncompleteCoverage);

    let empty_version = TestRoot::create("empty-version");
    fs::create_dir_all(empty_version.path().join("journal").join("v1"))
        .expect("version directory must be created");
    let error =
        FixtureCatalog::load(empty_version.path()).expect_err("version without cases must fail");
    assert_eq!(error.kind(), FixtureErrorKind::IncompleteCoverage);
}

#[test]
fn incomplete_coverage_names_a_real_failure() {
    let root = TestRoot::create("incomplete");
    create_case(root.path(), "protocol", "v1", "minimal", "minimal", b"one");
    let catalog = FixtureCatalog::load(root.path()).expect("single case is structurally valid");
    let error = catalog
        .verify_compatibility_coverage(CompatibilityPolicy::RequireFixtures)
        .expect_err("mandatory kinds are missing");
    assert_eq!(error.kind(), FixtureErrorKind::IncompleteCoverage);
    assert!(error.detail().contains("protocol/v1"));
}

#[test]
fn unknown_manifest_fields_and_digest_divergence_are_rejected() {
    let root = TestRoot::create("adversarial");
    let directory = create_case(root.path(), "provider", "v1", "minimal", "minimal", b"original");
    let manifest_path = directory.join("fixture.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("manifest must be readable");
    fs::write(&manifest_path, format!("{manifest}unknown = true\n"))
        .expect("manifest mutation must work");
    let syntax = FixtureCase::load(&directory).expect_err("unknown field must fail closed");
    assert_eq!(syntax.kind(), FixtureErrorKind::ManifestSyntax);

    fs::write(&manifest_path, manifest).expect("valid manifest must be restored");
    fs::write(directory.join("payload.bin"), b"changed").expect("payload mutation must work");
    let digest = FixtureCase::load(&directory).expect_err("digest divergence must fail");
    assert_eq!(digest.kind(), FixtureErrorKind::DigestMismatch);
}

#[test]
fn unlisted_files_and_nonportable_paths_fail_closed() {
    let root = TestRoot::create("inventory");
    let directory = create_case(root.path(), "tool", "v1", "minimal", "minimal", b"payload");
    fs::write(directory.join("extra.bin"), b"extra").expect("extra file must be written");
    let error = FixtureCase::load(&directory).expect_err("unlisted file must fail");
    assert_eq!(error.kind(), FixtureErrorKind::UnexpectedFile);

    assert_eq!(
        FixturePath::new("../escape").expect_err("parent traversal must fail").kind(),
        FixtureErrorKind::InvalidPath
    );
    assert_eq!(
        FixturePath::new("windows\\escape").expect_err("backslashes must fail").kind(),
        FixtureErrorKind::InvalidPath
    );
    assert_eq!(
        FixturePath::new("/absolute").expect_err("rooted paths must fail").kind(),
        FixtureErrorKind::InvalidPath
    );
    assert_eq!(
        FixturePath::new("NUL.txt").expect_err("Windows device names must fail").kind(),
        FixtureErrorKind::InvalidPath
    );
    assert_eq!(
        FixturePath::new("Cargo.toml")
            .expect("ordinary repository names must be portable")
            .as_str(),
        "Cargo.toml"
    );
    assert_eq!(
        FixturePath::new(".gitignore").expect("ordinary dotfiles must be portable").as_str(),
        ".gitignore"
    );
}

#[cfg(unix)]
#[test]
fn symlink_payload_is_rejected_without_following_it() {
    let root = TestRoot::create("symlink");
    let directory = create_case(root.path(), "workspace", "v1", "minimal", "minimal", b"payload");
    fs::remove_file(directory.join("payload.bin")).expect("payload must be removed");
    std::os::unix::fs::symlink("fixture.toml", directory.join("payload.bin"))
        .expect("symlink must be created");
    let error = FixtureCase::load(&directory).expect_err("fixture symlink must fail");
    assert_eq!(error.kind(), FixtureErrorKind::UnsafeFileType);
}
