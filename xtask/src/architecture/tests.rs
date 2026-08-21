use super::dependency::validate_dependency_edges;
use super::package_readme_path;
use super::policy::validate_policy;
use crate::error::Diagnostic;
use crate::model::{
    ArchitecturePolicy, CargoDependency, CargoDependencyKind, CargoMetadata, CargoPackage,
    CargoPackageMetadata, ForbiddenDependencyPolicy, PackagePolicy,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn policy() -> ArchitecturePolicy {
    toml::from_str(
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
            forbidden_dependencies = []
            controlled_source_roots = []

            [[layers]]
            name = "core"
            path = "crates/core"
            may_depend_on = ["core"]
            may_dev_depend_on = ["core", "testing"]

            [[layers]]
            name = "app"
            path = "crates/app"
            may_depend_on = ["core", "app"]
            may_dev_depend_on = ["core", "app", "testing"]

            [[layers]]
            name = "testing"
            path = "crates/app/testing"
            required_verification_class = "C"
            may_depend_on = ["core", "app", "testing"]
            may_dev_depend_on = ["core", "app", "testing"]

            [[verification_classes]]
            name = "V"
            may_depend_on = ["V"]
            may_dev_depend_on = ["V", "H", "C"]

            [[verification_classes]]
            name = "H"
            may_depend_on = ["V", "H"]
            may_dev_depend_on = ["V", "H", "C"]

            [[verification_classes]]
            name = "T"
            may_depend_on = ["V", "H", "T"]
            may_dev_depend_on = ["V", "H", "T", "C"]

            [[verification_classes]]
            name = "C"
            may_depend_on = ["V", "H", "C"]
            may_dev_depend_on = ["V", "H", "C"]

            [[packages]]
            name = "core"
            path = "crates/core/core"
            owner = "A1"
            layer = "core"
            verification_class = "V"

            [[packages]]
            name = "peritus-test-support"
            path = "crates/app/testing/peritus-test-support"
            owner = "A2"
            layer = "testing"
            verification_class = "C"

            [[packages]]
            name = "peritus-conformance"
            path = "crates/app/testing/peritus-conformance"
            owner = "A2"
            layer = "testing"
            verification_class = "C"

            [[packages]]
            name = "client"
            path = "crates/app/client"
            owner = "G1"
            layer = "app"
            verification_class = "C"
        "#,
    )
    .expect("fixture policy must parse")
}

fn dependency(name: &str, kind: Option<CargoDependencyKind>) -> CargoDependency {
    CargoDependency {
        name: name.to_owned(),
        source: None,
        req: "=0.0.0".to_owned(),
        path: Some(PathBuf::from(name)),
        kind,
        target: None,
        optional: false,
    }
}

fn package(name: &str, dependencies: Vec<CargoDependency>) -> CargoPackage {
    CargoPackage {
        id: name.to_owned(),
        name: name.to_owned(),
        version: "0.0.0".to_owned(),
        edition: "2024".to_owned(),
        rust_version: Some("1.97.1".to_owned()),
        license: Some("MIT".to_owned()),
        manifest_path: PathBuf::from(format!("crates/{name}/Cargo.toml")),
        readme: Some(PathBuf::from("README.md")),
        dependencies,
        targets: Vec::new(),
        metadata: CargoPackageMetadata::default(),
    }
}

fn dependency_diagnostics(
    policy: &ArchitecturePolicy,
    packages: &[CargoPackage],
) -> Vec<Diagnostic> {
    let references: Vec<_> = packages.iter().collect();
    let by_name: BTreeMap<_, _> =
        packages.iter().map(|package| (package.name.as_str(), package)).collect();
    let mut diagnostics = Vec::new();
    validate_dependency_edges(Path::new("."), policy, &references, &by_name, &mut diagnostics);
    diagnostics
}

#[test]
fn verified_crate_may_dev_depend_on_class_c_testing_crate_without_a_production_cycle() {
    for target in ["peritus-test-support", "peritus-conformance"] {
        let core =
            package("core", vec![dependency(target, Some(CargoDependencyKind::Development))]);
        let testing = package(target, vec![dependency("core", None)]);
        assert!(dependency_diagnostics(&policy(), &[core, testing]).is_empty());
    }
}

#[test]
fn verified_crate_may_not_normally_depend_on_class_c_testing_crate() {
    let core = package("core", vec![dependency("peritus-test-support", None)]);
    let support = package("peritus-test-support", vec![dependency("core", None)]);
    let diagnostics = dependency_diagnostics(&policy(), &[core, support]);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains("layer")));
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message().contains("verification class"))
    );
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains("cycle")));
}

#[test]
fn verified_crate_may_not_dev_depend_on_class_c_app_client() {
    let core = package("core", vec![dependency("client", Some(CargoDependencyKind::Development))]);
    let client = package("client", Vec::new());
    let diagnostics = dependency_diagnostics(&policy(), &[core, client]);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains("layer")));
    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic.message().contains("verification class"))
    );
}

#[test]
fn build_edge_preserves_optional_and_target_details_in_class_diagnostic() {
    let mut policy = policy();
    policy.packages[1].verification_class = "T".to_owned();
    let mut edge = dependency("peritus-test-support", Some(CargoDependencyKind::Build));
    edge.optional = true;
    edge.target = Some("cfg(unix)".to_owned());
    let diagnostics = dependency_diagnostics(
        &policy,
        &[package("core", vec![edge]), package("peritus-test-support", Vec::new())],
    );
    let message = diagnostics
        .iter()
        .map(Diagnostic::message)
        .find(|message| message.contains("verification class"))
        .expect("class direction must be rejected");
    assert!(message.contains("build dependency, optional, target `cfg(unix)`"));
}

#[test]
fn forbidden_package_pair_applies_to_development_edges() {
    let mut policy = policy();
    policy.forbidden_dependencies.push(ForbiddenDependencyPolicy {
        from: "core".to_owned(),
        to: "peritus-test-support".to_owned(),
        rationale: "Core authority must not import adversarial fixtures.".to_owned(),
    });
    let edge = dependency("peritus-test-support", Some(CargoDependencyKind::Development));
    let diagnostics = dependency_diagnostics(
        &policy,
        &[package("core", vec![edge]), package("peritus-test-support", Vec::new())],
    );
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains("forbidden")));
}

#[test]
fn policy_rejects_layer_cycles_and_unknown_verification_classes() {
    let mut policy = policy();
    policy.layers[0].may_depend_on.push("testing".to_owned());
    policy.packages.push(PackagePolicy {
        name: "bad".to_owned(),
        path: "crates/core/bad".into(),
        owner: "A1".to_owned(),
        layer: "core".to_owned(),
        verification_class: "unknown".to_owned(),
    });
    let diagnostics = validate_policy(&policy);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains("cycle")));
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message().contains("verification class"))
    );
}

#[test]
fn nested_testing_package_cannot_be_registered_as_an_app_client() {
    let mut policy = policy();
    policy.packages[1].layer = "app".to_owned();
    let diagnostics = validate_policy(&policy);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message().contains("most specific physical layer"))
    );
}

#[test]
fn testing_layer_package_cannot_masquerade_as_a_verified_class() {
    for class in ["V", "H", "T"] {
        let mut policy = policy();
        policy.packages[1].verification_class = class.to_owned();
        let diagnostics = validate_policy(&policy);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message()
                .contains("physical layer `testing` requires verification class `C`")
        }));
    }
}

#[test]
fn layer_cannot_require_an_unknown_verification_class() {
    let mut policy = policy();
    policy.layers[2].required_verification_class = Some("unknown".to_owned());
    let diagnostics = validate_policy(&policy);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message().contains("requires unknown verification class"))
    );
}

#[test]
fn trusted_source_roots_must_be_narrow_safe_and_unignored() {
    for root in ["", ".", "../outside", "/absolute", "target/trusted"] {
        let mut policy = policy();
        policy.ignored_directories = vec!["target".to_owned()];
        policy.trusted_source_roots = vec![PathBuf::from(root)];
        let diagnostics = validate_policy(&policy);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message().contains("trusted source root") })
        );
    }

    let mut policy = policy();
    policy.trusted_source_roots =
        vec![PathBuf::from("crates/trusted"), PathBuf::from("crates/trusted/nested")];
    let diagnostics = validate_policy(&policy);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message().contains("trusted source roots overlap"))
    );
}

#[test]
fn cargo_dependency_deserializes_kind_target_and_optional_details() {
    let dependency: CargoDependency = serde_json::from_str(
        r#"{
            "name":"peritus-test-support",
            "source":null,
            "req":"=0.0.0",
            "path":"crates/app/testing/peritus-test-support",
            "kind":"dev",
            "target":"cfg(windows)",
            "optional":true
        }"#,
    )
    .expect("Cargo metadata dependency fixture must decode");
    assert_eq!(dependency.kind, Some(CargoDependencyKind::Development));
    assert_eq!(dependency.target.as_deref(), Some("cfg(windows)"));
    assert!(dependency.optional);
}

#[test]
fn cargo_metadata_deserializes_target_source_paths() {
    let cargo: CargoMetadata = serde_json::from_str(
        r#"{
            "packages":[{
                "id":"fixture 0.0.0 (path+file:///workspace/fixture)",
                "name":"fixture",
                "version":"0.0.0",
                "edition":"2024",
                "rust_version":"1.97.1",
                "license":"MIT",
                "manifest_path":"/workspace/fixture/Cargo.toml",
                "readme":"README.md",
                "dependencies":[],
                "targets":[{
                    "kind":["bin"],
                    "crate_types":["bin"],
                    "name":"fixture",
                    "src_path":"/workspace/fixture/src/main.txt"
                }],
                "metadata":{}
            }],
            "workspace_members":["fixture 0.0.0 (path+file:///workspace/fixture)"]
        }"#,
    )
    .expect("Cargo metadata target fixture must decode");

    assert_eq!(cargo.packages[0].targets[0].src_path, Path::new("/workspace/fixture/src/main.txt"));
    assert_eq!(cargo.packages[0].targets[0].kind, ["bin"]);
    assert_eq!(cargo.packages[0].targets[0].crate_types, ["bin"]);
}

#[test]
fn relative_readme_is_resolved_from_absolute_package_manifest_directory() {
    let mut package = package("xtask", Vec::new());
    let package_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    package.manifest_path = package_root.join("Cargo.toml");
    package.readme = Some(PathBuf::from("README.md"));

    let resolved = package_readme_path(&package).expect("fixture declares a README");
    assert_eq!(resolved, package_root.join("README.md"));
    assert!(resolved.is_absolute());
    assert!(resolved.is_file());
}
