use super::*;

#[test]
fn rust_plan_builds_the_exact_nested_package() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    let project = temporary.path().join("game");
    std::fs::create_dir_all(project.join("src")).expect("nested package directory");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"game\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .expect("nested package manifest");
    std::fs::write(project.join("src/main.rs"), "fn main() { println!(\"game\"); }\n")
        .expect("nested package source");

    let plan = TargetGatePlan::discover(temporary.path(), vec![PathBuf::from("game/src/main.rs")])
        .expect("exact target plan");

    let build = plan
        .commands()
        .iter()
        .find(|command| command.label() == "Rust build")
        .expect("Rust build gate");
    let format = plan
        .commands()
        .iter()
        .find(|command| command.label() == "Rust format")
        .expect("Rust format gate");
    assert_eq!(format.program(), "cargo");
    assert_eq!(format.current_dir(), Path::new(""));
    assert_eq!(
        format.arguments(),
        ["fmt", "--manifest-path", "game/Cargo.toml", "--all", "--", "--check"]
    );
    assert_eq!(build.program(), "cargo");
    assert_eq!(build.current_dir(), Path::new(""));
    assert_eq!(
        build.arguments(),
        [
            "build",
            "--locked",
            "--all-targets",
            "--all-features",
            "--manifest-path",
            "game/Cargo.toml",
            "--workspace",
        ]
    );
}

#[test]
fn explicit_artifact_workspace_covers_general_outputs() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(
        temporary.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact workspace marker");
    std::fs::create_dir(temporary.path().join("out")).expect("output directory");
    std::fs::write(temporary.path().join("out/result.txt"), "result\n").expect("output artifact");

    let plan = TargetGatePlan::discover(temporary.path(), vec![PathBuf::from("out/result.txt")])
        .expect("artifact plan");

    assert!(plan.has_complete_coverage());
    assert!(plan.uncovered_paths().is_empty());
    assert_eq!(plan.projects()[0].kind(), ProjectKind::Artifact);
    assert_eq!(plan.commands().len(), 2);
    assert_eq!(plan.commands()[0].label(), "Source layout");
    assert_eq!(plan.commands()[1].label(), "Artifact CSV structure");
}

#[test]
fn manifestless_python_tests_use_their_nearest_conventional_project() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(
        temporary.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact workspace marker");
    let project = temporary.path().join("in/ordercalc");
    std::fs::create_dir_all(project.join("ordercalc")).expect("package directory");
    std::fs::create_dir_all(project.join("tests")).expect("test directory");
    std::fs::write(project.join("ordercalc/__init__.py"), "").expect("package marker");
    std::fs::write(project.join("tests/test_pricing.py"), "def test_price():\n    pass\n")
        .expect("test module");
    std::fs::write(project.join("tests/TEST_INTENT.md"), "pricing contract\n")
        .expect("test documentation");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![
            PathBuf::from("in/ordercalc/tests/TEST_INTENT.md"),
            PathBuf::from("in/ordercalc/tests/test_pricing.py"),
        ],
    )
    .expect("manifestless Python plan");

    assert!(plan.has_complete_coverage());
    assert_eq!(plan.projects().len(), 1);
    assert_eq!(plan.projects()[0].kind(), ProjectKind::Python);
    assert_eq!(plan.projects()[0].root(), Path::new("in/ordercalc"));
    assert_eq!(plan.projects()[0].manifest(), None);
    assert!(plan.commands().iter().any(|command| command.label() == "Python compile"));
    assert!(plan.commands().iter().any(|command| command.label() == "Python tests"));
    assert!(plan.commands().iter().all(|command| {
        command.current_dir() == Path::new("in/ordercalc")
            && command.project().kind() == ProjectKind::Python
    }));
}

#[test]
fn manifestless_node_module_runs_adjacent_test_files() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(
        temporary.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact workspace marker");
    let source = temporary.path().join("in/cart-ui/src");
    std::fs::create_dir_all(&source).expect("source directory");
    std::fs::write(source.join("cartState.js"), "module.exports = {};\n").expect("source module");
    std::fs::write(source.join("cartState.test.js"), "require('./cartState');\n")
        .expect("adjacent test");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![PathBuf::from("in/cart-ui/src/cartState.js")],
    )
    .expect("manifestless Node plan");

    assert!(plan.has_complete_coverage());
    assert_eq!(plan.projects().len(), 1);
    assert_eq!(plan.projects()[0].kind(), ProjectKind::Node);
    assert_eq!(plan.projects()[0].root(), Path::new("in/cart-ui/src"));
    assert_eq!(plan.projects()[0].manifest(), None);
    let test = plan
        .commands()
        .iter()
        .find(|command| command.label() == "Node tests")
        .expect("Node test command");
    assert_eq!(test.program(), "node");
    assert_eq!(test.arguments(), ["cartState.test.js"]);
    assert_eq!(test.current_dir(), Path::new("in/cart-ui/src"));
}
