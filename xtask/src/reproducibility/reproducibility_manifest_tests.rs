use super::reproducibility_manifests::validate;
use crate::error::Diagnostic;
use crate::model::{CargoMetadata, CargoPackage, CargoPackageMetadata};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn root_registry_git_and_replace_tables_are_rejected_structurally() {
    let fixture = Fixture::new();
    for override_table in [
        "\n[patch.crates-io]\nitoa = { path = \"../outside\" }\n",
        "\n[patch.crates-io]\nitoa = { git = \"https://example.invalid/itoa\", rev = \"0123456789012345678901234567890123456789\" }\n",
        "\n[replace]\n\"itoa:1.0.18\" = { path = \"../outside\" }\n",
    ] {
        fixture.write("workspace/Cargo.toml", &format!("[workspace]\n{override_table}"));
        let mut diagnostics = Vec::new();
        validate(
            &fixture.workspace,
            &CargoMetadata { packages: Vec::new(), workspace_members: Vec::new() },
            &mut diagnostics,
        )
        .expect("fixture manifest must parse");
        assert_message(&diagnostics, "dependency overrides are forbidden");
    }
}

#[test]
fn workspace_member_override_is_checked_even_when_cargo_ignores_it() {
    let fixture = Fixture::new();
    fixture.write("workspace/Cargo.toml", "[workspace]\nmembers = [\"member\"]\n");
    fixture.write(
        "workspace/member/Cargo.toml",
        "[package]\nname = \"member\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[patch.crates-io]\nitoa = { path = \"../../outside\" }\n",
    );
    let package = package("member", fixture.workspace.join("member/Cargo.toml"));
    let cargo =
        CargoMetadata { packages: vec![package], workspace_members: vec!["member".to_owned()] };
    let mut diagnostics = Vec::new();
    validate(&fixture.workspace, &cargo, &mut diagnostics).expect("fixture manifests must parse");
    assert_message(&diagnostics, "dependency overrides are forbidden");
}

#[test]
fn consumed_outside_patch_is_visible_to_build_but_hidden_by_no_deps_metadata() {
    let fixture = Fixture::new();
    fixture.write(
        "outside/Cargo.toml",
        "[package]\nname = \"itoa\"\nversion = \"1.0.18\"\nedition = \"2024\"\n",
    );
    fixture.write("outside/src/lib.rs", "pub const PATCHED: bool = true;\n");
    let outside = toml_path(&fixture.outside);
    fixture.write(
        "workspace/Cargo.toml",
        &format!(
            "[package]\nname = \"consumer\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[package.metadata.peritus]\nowner = \"test\"\nlayer = \"test\"\nverification-class = \"C\"\n\n[dependencies]\nitoa = \"=1.0.18\"\n\n[patch.crates-io]\nitoa = {{ path = \"{outside}\" }}\n"
        ),
    );
    fixture.write("workspace/src/main.rs", "fn main() { assert!(itoa::PATCHED); }\n");

    let check = Command::new("cargo")
        .args(["check", "--offline", "--manifest-path"])
        .arg(fixture.workspace.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(fixture.base.join("target"))
        .output()
        .expect("fixture Cargo check must execute");
    assert!(
        check.status.success(),
        "outside patch must be consumed by the build: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    let metadata = Command::new("cargo")
        .args(["metadata", "--offline", "--format-version", "1", "--no-deps", "--manifest-path"])
        .arg(fixture.workspace.join("Cargo.toml"))
        .output()
        .expect("fixture Cargo metadata must execute");
    assert!(metadata.status.success(), "fixture metadata must succeed");
    let raw: serde_json::Value =
        serde_json::from_slice(&metadata.stdout).expect("metadata JSON must decode");
    let dependency = &raw["packages"][0]["dependencies"][0];
    assert!(dependency["source"].as_str().is_some_and(|source| source.starts_with("registry+")));
    assert!(dependency["path"].is_null(), "--no-deps metadata unexpectedly exposed the patch");

    let cargo: CargoMetadata =
        serde_json::from_slice(&metadata.stdout).expect("typed metadata must decode");
    let mut diagnostics = Vec::new();
    validate(&fixture.workspace, &cargo, &mut diagnostics).expect("fixture manifest must parse");
    assert_message(&diagnostics, "Cargo [patch] dependency overrides are forbidden");
}

fn package(id: &str, manifest_path: PathBuf) -> CargoPackage {
    CargoPackage {
        id: id.to_owned(),
        name: id.to_owned(),
        version: "0.0.0".to_owned(),
        edition: "2024".to_owned(),
        rust_version: None,
        license: None,
        manifest_path,
        readme: None,
        dependencies: Vec::new(),
        targets: Vec::new(),
        metadata: CargoPackageMetadata::default(),
    }
}

fn toml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

fn assert_message(diagnostics: &[Diagnostic], expected: &str) {
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message().contains(expected)),
        "expected `{expected}`, got {diagnostics:?}"
    );
}

struct Fixture {
    base: PathBuf,
    workspace: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "peritus-repro-manifest-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let workspace = base.join("workspace");
        let outside = base.join("outside");
        fs::create_dir_all(&workspace).expect("fixture workspace must be creatable");
        Self { base, workspace, outside }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.base.join(relative);
        fs::create_dir_all(path.parent().expect("fixture path must have parent"))
            .expect("fixture directory must be creatable");
        fs::write(path, contents).expect("fixture file must be writable");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).expect("fixture must be removable");
    }
}
