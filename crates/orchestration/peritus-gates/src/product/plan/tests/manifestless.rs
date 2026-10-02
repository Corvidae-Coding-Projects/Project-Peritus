//! Manifestless project discovery regressions.

use super::*;

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
    std::fs::write(project.join("client.py"), "def convert():\n    pass\n")
        .expect("production module");
    std::fs::write(project.join("tests/test_pricing.py"), "def test_price():\n    pass\n")
        .expect("test module");
    std::fs::write(project.join("tests/TEST_INTENT.md"), "pricing contract\n")
        .expect("test documentation");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![
            PathBuf::from("in/ordercalc/client.py"),
            PathBuf::from("in/ordercalc/tests/TEST_INTENT.md"),
            PathBuf::from("in/ordercalc/tests/test_pricing.py"),
        ],
        &[],
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
        &[],
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

#[test]
fn conventional_sqlite_workspace_runs_migration_verification() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(
        temporary.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact workspace marker");
    let database = temporary.path().join("in/db");
    std::fs::create_dir_all(&database).expect("database directory");
    std::fs::write(database.join("schema.sql"), "CREATE TABLE item(id INTEGER);\n")
        .expect("schema");
    std::fs::write(database.join("migration.sql"), "ALTER TABLE item ADD COLUMN name TEXT;\n")
        .expect("migration");
    std::fs::write(database.join("migration_report.md"), "# Migration\n").expect("report");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![PathBuf::from("in/db/migration.sql"), PathBuf::from("in/db/migration_report.md")],
        &[],
    )
    .expect("SQLite plan");

    assert!(plan.has_complete_coverage());
    assert_eq!(plan.projects().len(), 1);
    assert_eq!(plan.projects()[0].kind(), ProjectKind::Sqlite);
    assert_eq!(plan.projects()[0].root(), Path::new("in/db"));
    assert_eq!(plan.projects()[0].manifest(), Some(Path::new("in/db/schema.sql")));
    let verification = plan
        .commands()
        .iter()
        .find(|command| command.label() == "SQLite migration verification")
        .expect("SQLite verification command");
    assert_eq!(verification.program(), "peritus-internal");
    assert_eq!(verification.arguments(), ["sqlite-migration"]);
}

#[test]
fn root_level_python_tests_cover_workflow_and_documentation_changes() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(
        temporary.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact workspace marker");
    let project = temporary.path().join("in/project");
    std::fs::create_dir_all(project.join(".github/workflows")).expect("workflow directory");
    std::fs::write(project.join("app.py"), "VALUE = 1\n").expect("source");
    std::fs::write(project.join("test_app.py"), "def test_value():\n    assert True\n")
        .expect("root test");
    std::fs::write(project.join(".github/workflows/ci.yml"), "name: CI\n").expect("workflow");
    std::fs::write(project.join("ci_design_notes.md"), "# CI\n").expect("notes");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![
            PathBuf::from("in/project/.github/workflows/ci.yml"),
            PathBuf::from("in/project/ci_design_notes.md"),
        ],
        &[],
    )
    .expect("manifestless root-test plan");

    assert!(plan.has_complete_coverage());
    assert_eq!(plan.projects().len(), 1);
    assert_eq!(plan.projects()[0].kind(), ProjectKind::Python);
    assert_eq!(plan.projects()[0].root(), Path::new("in/project"));
    assert!(plan.commands().iter().any(|command| command.label() == "YAML structure"));
    let compile = plan
        .commands()
        .iter()
        .find(|command| command.label() == "Python compile")
        .expect("Python syntax command");
    let tests = plan
        .commands()
        .iter()
        .find(|command| command.label() == "Python tests")
        .expect("Python test command");
    assert_eq!(&compile.arguments()[..2], ["-B", "-c"]);
    assert!(compile.arguments()[2].contains("ast.parse"));
    assert_eq!(tests.arguments(), ["-B", "-m", "pytest", "-p", "no:cacheprovider"]);
}

#[test]
fn python_requirements_are_verified_without_installing_or_network_access() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    let project = temporary.path().join("in/depsvc");
    std::fs::create_dir_all(project.join("tests")).expect("test directory");
    std::fs::write(project.join("requirements.txt"), "some-package>=8,<9\n").expect("requirements");
    std::fs::write(project.join("slugger.py"), "def make_slug(value):\n    return value\n")
        .expect("source");
    std::fs::write(project.join("tests/test_slugger.py"), "def test_slug():\n    pass\n")
        .expect("test");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![PathBuf::from("in/depsvc/requirements.txt")],
        &[],
    )
    .expect("Python dependency plan");

    let dependencies = plan
        .commands()
        .iter()
        .find(|command| command.label() == "Python dependencies")
        .expect("Python dependency gate");
    assert_eq!(dependencies.program(), "python");
    assert_eq!(dependencies.current_dir(), Path::new("in/depsvc"));
    assert_eq!(
        dependencies.arguments(),
        [
            "-B",
            "-m",
            "pip",
            "install",
            "--dry-run",
            "--no-index",
            "--disable-pip-version-check",
            "--requirement",
            "requirements.txt",
        ]
    );
}

#[test]
fn locked_uv_project_runs_python_checks_in_its_frozen_environment() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    let project = temporary.path().join("in/service");
    std::fs::create_dir_all(project.join("tests")).expect("test directory");
    std::fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"service\"\nversion = \"0.1.0\"\n\n[tool.pytest.ini_options]\n\n[tool.ruff]\n",
    )
    .expect("Python manifest");
    std::fs::write(project.join("uv.lock"), "version = 1\n").expect("uv lockfile");
    std::fs::write(project.join("service.py"), "VALUE = 1\n").expect("source");
    std::fs::write(project.join("tests/test_service.py"), "def test_value():\n    assert True\n")
        .expect("test");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![PathBuf::from("in/service/service.py")],
        &[],
    )
    .expect("locked Python plan");

    for label in ["Python compile", "Python tests", "Python lint"] {
        let command = plan
            .commands()
            .iter()
            .find(|command| command.label() == label)
            .unwrap_or_else(|| panic!("missing {label}"));
        assert_eq!(command.program(), "uv");
        assert_eq!(&command.arguments()[..3], ["run", "--frozen", "python"]);
        assert_eq!(command.current_dir(), Path::new("in/service"));
    }
}

#[test]
fn standalone_changed_python_source_gets_syntax_acceptance() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(
        temporary.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact workspace marker");
    let project = temporary.path().join("in/scripts");
    std::fs::create_dir_all(&project).expect("project directory");
    std::fs::write(project.join("catalog.py"), "def lookup(value):\n    return value\n")
        .expect("standalone source");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![PathBuf::from("in/scripts/catalog.py")],
        &[],
    )
    .expect("standalone Python plan");

    assert!(plan.has_complete_coverage());
    assert_eq!(plan.projects().len(), 1);
    assert_eq!(plan.projects()[0].kind(), ProjectKind::Python);
    assert_eq!(plan.projects()[0].root(), Path::new("in/scripts"));
    assert_eq!(plan.projects()[0].manifest(), None);
    assert_eq!(
        plan.commands().iter().map(GateCommandSpec::label).collect::<Vec<_>>(),
        ["Source readability", "Python compile"]
    );
}

#[test]
fn standalone_python_change_covers_adjacent_supporting_documentation() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    let project = temporary.path().join("in/scripts");
    std::fs::create_dir_all(&project).expect("project directory");
    std::fs::write(project.join("catalog.py"), "def lookup(value):\n    return value\n")
        .expect("standalone source");
    std::fs::write(project.join("README.md"), "# Catalog\n").expect("support documentation");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![PathBuf::from("in/scripts/README.md"), PathBuf::from("in/scripts/catalog.py")],
        &[],
    )
    .expect("standalone Python plan");

    assert!(plan.has_complete_coverage());
    assert!(plan.uncovered_paths().is_empty());
    assert_eq!(plan.projects().len(), 1);
    assert_eq!(plan.projects()[0].kind(), ProjectKind::Python);
    assert_eq!(plan.projects()[0].root(), Path::new("in/scripts"));
    assert_eq!(
        plan.commands().iter().map(GateCommandSpec::label).collect::<Vec<_>>(),
        ["Source readability", "Python compile"]
    );
}

#[test]
fn standalone_python_change_does_not_cover_unrelated_nested_file() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(temporary.path().join("catalog.py"), "VALUE = 1\n").expect("standalone source");
    std::fs::create_dir(temporary.path().join("unrelated")).expect("unrelated directory");
    std::fs::write(temporary.path().join("unrelated/README.md"), "# Other\n")
        .expect("unrelated documentation");

    let plan = TargetGatePlan::discover(
        temporary.path(),
        vec![PathBuf::from("catalog.py"), PathBuf::from("unrelated/README.md")],
        &[],
    )
    .expect("standalone Python plan");

    assert!(!plan.has_complete_coverage());
    assert_eq!(plan.uncovered_paths(), [PathBuf::from("unrelated/README.md")]);
}
