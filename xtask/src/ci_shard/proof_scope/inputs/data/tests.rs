use super::discover;
use crate::{
    metadata,
    model::{CargoMetadata, CargoTarget},
    source,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("peritus-proof-data-{}-{id}", std::process::id()));
        fs::create_dir_all(root.join("crate/src")).expect("create data fixture");
        fs::write(root.join("crate/Cargo.toml"), b"fixture manifest").expect("manifest");
        Self(root)
    }

    fn cargo(&self) -> CargoMetadata {
        serde_json::from_value(serde_json::json!({
            "workspace_members": ["fixture"],
            "packages": [{
                "id": "fixture", "name": "fixture", "version": "0.0.0", "edition": "2024",
                "manifest_path": self.0.join("crate/Cargo.toml"),
                "dependencies": [], "targets": []
            }]
        }))
        .expect("fixture Cargo metadata")
    }

    fn scan(&self, contents: &str) -> Result<BTreeSet<PathBuf>, crate::error::XtaskError> {
        let source = self.0.join("crate/src/lib.rs");
        fs::write(&source, contents).expect("write source fixture");
        discover(&self.0, &[source], &self.cargo())
    }

    fn data(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("data parent")).expect("data directory");
        fs::write(&path, bytes).expect("data fixture");
        path
    }

    fn with_other_package(&self, directory: &str) -> CargoMetadata {
        let mut cargo = self.cargo();
        let manifest = self.data(&format!("{directory}/Cargo.toml"), b"other manifest");
        let package = serde_json::from_value(serde_json::json!({
            "id": "other", "name": "other", "version": "0.0.0", "edition": "2024",
            "manifest_path": manifest, "dependencies": [], "targets": []
        }))
        .expect("other package metadata");
        cargo.packages.push(package);
        cargo.workspace_members.push("other".to_owned());
        cargo
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn literals_are_decoded_and_binary_data_is_never_scanned_as_rust() {
    let fixture = Fixture::new();
    let text = fixture.data("crate/src/data.txt", b"include_str!(\"absent\")");
    let binary = fixture.data("crate/src/data.bin", &[0xff, 0x00, 0x80]);
    let found = fixture
        .scan(
            r##"
            const TEXT: &str = include_str!(r#"data.txt"#,);
            const DUPLICATE: &str = include_str!("d\x61ta.txt");
            const BYTES: &[u8] = include_bytes!["data.bin"];
        "##,
        )
        .expect("literal data discovery");
    assert_eq!(found, BTreeSet::from([text, binary]));
}

#[test]
fn comments_and_all_string_literal_forms_do_not_create_data_inputs() {
    let fixture = Fixture::new();
    let found = fixture
        .scan(
            r####"
            // include_str!("absent")
            /* outer /* include_bytes!("absent") */ end */
            const TEXT: &str = "include_str!(\"absent\")";
            const RAW: &str = r###"include_bytes!("absent")"###;
            const BYTES: &[u8] = br#"include_str!("absent")"#;
            const C: &std::ffi::CStr = c"include_bytes!(absent)";
        "####,
        )
        .expect("only comments and strings");
    assert!(found.is_empty());
}

#[test]
fn concat_manifest_paths_use_package_metadata_and_normalize_internal_parents() {
    let fixture = Fixture::new();
    let target = fixture.data("assets/data.txt", b"data");
    let found = fixture
        .scan(
            r#"
            const DATA: &str = include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"), "/../assets/", concat!("data", ".txt"),
            ));
        "#,
        )
        .expect("deterministic Cargo manifest expression");
    assert_eq!(found, BTreeSet::from([target]));
}

#[test]
fn unsupported_operands_aliases_and_incomplete_invocations_fail_closed() {
    let fixture = Fixture::new();
    fixture.data("crate/src/data.txt", b"data");
    for source in [
        r#"include_str!(some_macro!("data.txt"))"#,
        r#"include_bytes!(concat!(env!("OUT_DIR"), "/data.txt"))"#,
        r#"include_str!(concat!("data", 1, ".txt"))"#,
        r#"include_str!("data.txt", "other.txt")"#,
        r#"include_str!("data.txt""#,
        r#"include_str!(concat!("data", ".txt")"#,
        r#"use std::include_str as load; load!("data.txt");"#,
        r#"use std::{include_bytes as load}; load!("data.txt");"#,
        r#"unknown::include_bytes!("data.txt")"#,
    ] {
        assert!(fixture.scan(source).is_err(), "unsupported source accepted: {source}");
    }
}

#[test]
fn missing_outside_and_non_file_paths_fail_closed() {
    let fixture = Fixture::new();
    for source in [
        r#"include_str!("absent.txt")"#,
        r#"include_bytes!("../../../outside.txt")"#,
        r#"include_str!(".")"#,
        r#"include_str!("")"#,
        r#"include_str!("..\\outside.txt")"#,
    ] {
        assert!(fixture.scan(source).is_err(), "invalid data path accepted: {source}");
    }
    let file = fixture.data("crate/src/data.txt", b"data");
    assert!(fixture.scan(r#"include_str!("data.txt/../data.txt")"#).is_err());
    assert!(fixture.scan(&format!("include_str!({:?})", file.parent().expect("parent"))).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_components_are_rejected_before_parent_normalization() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let file = fixture.data("crate/src/data.txt", b"data");
    symlink(&file, fixture.0.join("crate/src/linked.txt")).expect("file symlink");
    symlink(fixture.0.join("crate"), fixture.0.join("crate/src/linked"))
        .expect("directory symlink");
    for source in [
        r#"include_str!("linked.txt")"#,
        r#"include_str!("linked/src/data.txt")"#,
        r#"include_str!("linked/../data.txt")"#,
    ] {
        assert!(fixture.scan(source).is_err(), "symlink data path accepted: {source}");
    }
}

#[test]
fn manifest_environment_requires_a_declared_workspace_owner() {
    let fixture = Fixture::new();
    let source = fixture.0.join("outside.rs");
    fs::write(&source, r#"include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data.txt"))"#)
        .expect("unowned source");
    assert!(discover(&fixture.0, &[source], &fixture.cargo()).is_err());
}

fn library_target(path: PathBuf) -> CargoTarget {
    CargoTarget {
        name: "fixture".to_owned(),
        kind: vec!["lib".to_owned()],
        crate_types: vec!["lib".to_owned()],
        src_path: path,
    }
}

fn manifest_data_source(fixture: &Fixture, relative: &str) -> PathBuf {
    fixture.data(
        relative,
        br#"const DATA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data.txt"));"#,
    )
}

#[test]
fn relocated_cargo_target_cannot_hash_the_physical_owners_data() {
    let fixture = Fixture::new();
    let mut cargo = fixture.with_other_package("other");
    let shared = manifest_data_source(&fixture, "other/src/shared.rs");
    fixture.data("crate/data.txt", b"compiling package data");
    fixture.data("other/data.txt", b"physical source owner data");
    cargo.packages[0].targets.push(library_target(shared.clone()));
    let error = discover(&fixture.0, &[shared], &cargo).expect_err("relocated target rejected");
    assert!(error.to_string().contains("outside its compiling package"));
}

#[test]
fn cross_package_include_and_path_sources_cannot_rebind_manifest_data() {
    let fixture = Fixture::new();
    let mut cargo = fixture.with_other_package("other");
    let shared = manifest_data_source(&fixture, "other/src/shared.rs");
    fixture.data("crate/data.txt", b"compiling package data");
    fixture.data("other/data.txt", b"physical source owner data");
    let source = fixture.0.join("crate/src/lib.rs");
    cargo.packages[0].targets.push(library_target(source.clone()));
    for contents in [
        r#"include!("../../other/src/shared.rs");"#,
        r#"#[path = "../../other/src/shared.rs"] mod shared;"#,
    ] {
        fs::write(&source, contents).expect("cross-package source reference");
        let error = discover(&fixture.0, &[source.clone(), shared.clone()], &cargo)
            .expect_err("cross-package source context rejected");
        assert!(error.to_string().contains("crosses a package source boundary"));
    }
}

#[test]
fn nested_package_roots_cannot_confuse_default_module_context() {
    let fixture = Fixture::new();
    let mut cargo = fixture.with_other_package("crate/src/child");
    let source = fixture.data("crate/src/lib.rs", b"mod child;");
    let child = manifest_data_source(&fixture, "crate/src/child/mod.rs");
    fixture.data("crate/data.txt", b"compiling parent package data");
    fixture.data("crate/src/child/data.txt", b"nested package data");
    cargo.packages[0].targets.push(library_target(source.clone()));
    let error = discover(&fixture.0, &[source, child], &cargo)
        .expect_err("nested source ownership rejected");
    assert!(error.to_string().contains("nested or duplicate package directory"));
}

#[test]
fn same_package_include_and_path_references_preserve_manifest_context() {
    let fixture = Fixture::new();
    let source = fixture.data(
        "crate/src/lib.rs",
        br#"
        include!("pieces/shared.rs");
        #[path = "pieces/../pieces/shared.rs"] mod shared;
    "#,
    );
    let shared = manifest_data_source(&fixture, "crate/src/pieces/shared.rs");
    let expected = fixture.data("crate/data.txt", b"same package data");
    let mut cargo = fixture.cargo();
    cargo.packages[0].targets.push(library_target(source.clone()));
    let found = discover(&fixture.0, &[source, shared], &cargo)
        .expect("package-local source sharing remains supported");
    assert_eq!(found, BTreeSet::from([expected]));
}

#[test]
fn current_workspace_data_includes_are_all_resolvable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root");
    let policy = metadata::architecture_policy(root).expect("architecture policy");
    let cargo = metadata::cargo_metadata(root).expect("workspace metadata");
    let roots: Vec<_> = cargo
        .packages
        .iter()
        .flat_map(|package| package.targets.iter().map(|target| target.src_path.clone()))
        .collect();
    let rust = source::discover_compilation_sources(root, &policy, &roots)
        .expect("compilation source discovery");
    assert!(rust.diagnostics.is_empty(), "{:?}", rust.diagnostics);
    let data = discover(root, &rust.files, &cargo).expect("baseline data discovery");
    for relative in [
        "security/threat-model-v1.toml",
        "release/templates/release-inputs.template.json",
        "packaging/linux/Install-Peritus.sh",
        "xtask/src/reproducibility/canonical/formal-authority.yml",
        "xtask/src/reproducibility/canonical/formal-governance.yml",
        "crates/app/testing/peritus-bug-discovery/corpus/sse/basic",
    ] {
        assert!(data.contains(&root.join(relative)), "missing compiled data: {relative}");
    }
}
