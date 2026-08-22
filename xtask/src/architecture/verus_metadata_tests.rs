use super::validate_verus_opt_ins;
use crate::model::{
    ArchitecturePolicy, CargoMetadata, CargoPackage, CargoPackageMetadata, VerusPackageMetadata,
};
use std::path::{Path, PathBuf};

#[test]
fn every_formal_class_fails_closed_without_an_explicit_true_verus_opt_in() {
    for class in ["V", "H", "T"] {
        let policy = policy(class);
        for metadata in [None, Some(VerusPackageMetadata::default())] {
            let formal = package("core", metadata);
            let cargo = cargo(vec![formal]);
            let diagnostics = validate_verus_opt_ins(Path::new("."), &policy, &cargo);
            assert!(diagnostics.iter().any(|item| item.message().contains("opted out")));
        }
    }
}

#[test]
fn explicit_true_is_accepted_for_formal_crates_and_class_c_may_opt_out() {
    let formal = package(
        "core",
        Some(VerusPackageMetadata { verify: true, ..VerusPackageMetadata::default() }),
    );
    let client = package("client", None);
    let cargo = cargo(vec![formal, client]);
    let mut policy = policy("V");
    policy.packages.push(crate::model::PackagePolicy {
        name: "client".to_owned(),
        path: "crates/client".into(),
        owner: "G1".to_owned(),
        layer: "app".to_owned(),
        verification_class: "C".to_owned(),
    });
    assert!(validate_verus_opt_ins(Path::new("."), &policy, &cargo).is_empty());
}

#[test]
fn formal_crates_reject_cargo_verus_bootstrap_modes() {
    for metadata in [
        VerusPackageMetadata {
            verify: true,
            is_vstd: Some(true),
            ..VerusPackageMetadata::default()
        },
        VerusPackageMetadata {
            verify: true,
            is_builtin: Some(true),
            ..VerusPackageMetadata::default()
        },
        VerusPackageMetadata {
            verify: true,
            no_vstd: Some(false),
            ..VerusPackageMetadata::default()
        },
    ] {
        let formal = package("core", Some(metadata));
        let cargo = cargo(vec![formal]);
        assert!(!validate_verus_opt_ins(Path::new("."), &policy("V"), &cargo).is_empty());
    }
}

#[test]
fn unknown_cargo_verus_metadata_keys_fail_deserialization() {
    let error =
        serde_json::from_str::<VerusPackageMetadata>(r#"{"verify":true,"future-trust-mode":true}"#)
            .expect_err("unknown pinned cargo-verus metadata must fail closed");
    assert!(error.to_string().contains("unknown field"));
}

fn policy(class: &str) -> ArchitecturePolicy {
    toml::from_str(&format!(
        r#"
schema = 2
soft_source_lines = 400
hard_source_lines = 700
root_module_lines = 80
required_license = "MIT"
ignored_directories = []
forbidden_module_names = []
trusted_source_roots = []
source_exceptions = []
layers = []
verification_classes = []
forbidden_dependencies = []
controlled_source_roots = []

[[packages]]
name = "core"
path = "crates/core"
owner = "A1"
layer = "foundation"
verification_class = "{class}"
"#
    ))
    .expect("Verus metadata policy fixture must parse")
}

fn package(name: &str, verus: Option<VerusPackageMetadata>) -> CargoPackage {
    CargoPackage {
        id: name.to_owned(),
        name: name.to_owned(),
        version: "0.0.0".to_owned(),
        edition: "2024".to_owned(),
        rust_version: Some("1.97.1".to_owned()),
        license: Some("MIT".to_owned()),
        manifest_path: PathBuf::from(format!("crates/{name}/Cargo.toml")),
        readme: Some(PathBuf::from("README.md")),
        dependencies: Vec::new(),
        targets: Vec::new(),
        metadata: CargoPackageMetadata { peritus: None, verus },
    }
}

fn cargo(packages: Vec<CargoPackage>) -> CargoMetadata {
    CargoMetadata {
        workspace_members: packages.iter().map(|package| package.id.clone()).collect(),
        packages,
    }
}
