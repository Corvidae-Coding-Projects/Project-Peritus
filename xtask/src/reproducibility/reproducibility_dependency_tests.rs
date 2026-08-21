use super::reproducibility_dependencies::{is_exact_registry_requirement, validate};
use crate::error::Diagnostic;
use crate::model::{CargoDependency, CargoMetadata, CargoPackage, CargoPackageMetadata};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn path_dependency_must_be_inside_and_a_registered_workspace_member() {
    let fixture = Fixture::new();
    let valid = fixture.root.join("member");
    let unregistered = fixture.root.join("unregistered");
    for directory in [&valid, &unregistered, &fixture.outside] {
        fs::create_dir_all(directory).expect("package directory must be creatable");
    }

    assert!(check(&fixture, &valid, true).is_empty());
    assert_message(
        &check(&fixture, &unregistered, false),
        "not a direct registered workspace member",
    );
    assert_message(
        &check(&fixture, &fixture.outside, false),
        "not a direct registered workspace member",
    );
}

#[cfg(unix)]
#[test]
fn in_root_symlink_alias_to_a_member_is_rejected() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let member = fixture.root.join("member");
    fs::create_dir_all(&member).expect("member directory must be creatable");
    symlink(&member, fixture.root.join("alias")).expect("alias symlink must be creatable");

    let diagnostics = check(&fixture, &fixture.root.join("alias"), true);
    assert_message(&diagnostics, "not a direct registered workspace member");
}

#[test]
fn registry_requirements_are_one_complete_exact_semver_comparator() {
    for requirement in ["=0.0.0", "=1.2.3", "=1.2.3-alpha.1", "=1.2.3-rc-1.0"] {
        assert!(is_exact_registry_requirement(requirement), "expected `{requirement}` to pass");
    }
    for requirement in [
        "1.2.3",
        "=1",
        "=1.0",
        "=1.0.*",
        "=1.2.3,=1.2.4",
        ">=1.2.3",
        "=01.2.3",
        "=1.02.3",
        "=1.2.03",
        "=1.2.3+build",
        "=1.2.3-01",
        "=1.2.3-",
        "=1.2.3-alpha..1",
    ] {
        assert!(!is_exact_registry_requirement(requirement), "expected `{requirement}` to fail");
    }
}

fn check(fixture: &Fixture, dependency_path: &Path, register_member: bool) -> Vec<Diagnostic> {
    let consumer = package(
        "consumer",
        fixture.root.join("consumer/Cargo.toml"),
        vec![dependency("subject", dependency_path)],
    );
    fs::create_dir_all(fixture.root.join("consumer")).expect("consumer must be creatable");
    let member = package("member", fixture.root.join("member/Cargo.toml"), Vec::new());
    let mut packages = vec![consumer];
    let mut workspace_members = vec!["consumer".to_owned()];
    if register_member {
        packages.push(member);
        workspace_members.push("member".to_owned());
    }
    let cargo = CargoMetadata { packages, workspace_members };
    let mut diagnostics = Vec::new();
    validate(&fixture.root, &cargo, &mut diagnostics);
    diagnostics
}

fn package(id: &str, manifest_path: PathBuf, dependencies: Vec<CargoDependency>) -> CargoPackage {
    CargoPackage {
        id: id.to_owned(),
        name: id.to_owned(),
        version: "0.0.0".to_owned(),
        edition: "2024".to_owned(),
        rust_version: Some("1.97.1".to_owned()),
        license: Some("MIT".to_owned()),
        manifest_path,
        readme: None,
        dependencies,
        targets: Vec::new(),
        metadata: CargoPackageMetadata::default(),
    }
}

fn dependency(name: &str, path: &Path) -> CargoDependency {
    CargoDependency {
        name: name.to_owned(),
        source: None,
        req: "*".to_owned(),
        path: Some(path.to_path_buf()),
        kind: None,
        target: None,
        optional: false,
    }
}

fn assert_message(diagnostics: &[Diagnostic], expected: &str) {
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message().contains(expected)),
        "expected `{expected}`, got {diagnostics:?}"
    );
}

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "peritus-repro-dependency-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("workspace");
        let outside = base.join("outside");
        fs::create_dir_all(&root).expect("fixture root must be creatable");
        Self { base, root, outside }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).expect("fixture must be removable");
    }
}
