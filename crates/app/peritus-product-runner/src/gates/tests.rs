//! Candidate-aware gate execution regressions.

use std::fs;

use super::*;

#[test]
fn literal_manifestless_output_is_covered_by_its_host_owned_request() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("result.txt"), "RESPONSIVE").expect("requested output");

    let report = run_scoped(
        root.path(),
        vec![PathBuf::from("result.txt")],
        None,
        ProductDeliveryScope::WorkspaceChanges,
        "Create result.txt containing exactly RESPONSIVE.",
    )
    .expect("gate report");

    assert!(report.report.passed(), "{}", report.output);
    assert!(report.report.uncovered_paths().is_empty());
    assert!(report.output.contains("result.txt: present"));
    assert!(report.output.contains("Exact-target acceptance: PASS"));
}

#[test]
fn artifact_layout_checks_candidate_sources_without_rejecting_untouched_vendor_code() {
    let root = tempfile::tempdir().expect("root");
    fs::write(
        root.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact marker");
    fs::create_dir_all(root.path().join("third_party")).expect("vendor directory");
    fs::write(root.path().join("third_party/legacy.c"), "line\n".repeat(700))
        .expect("legacy source");
    fs::write(
        root.path().join("vm.js"),
        format!("const ready = true;\n{}", "// source detail\n".repeat(700)),
    )
    .expect("candidate source beyond the former line limit");

    let report = run_scoped(
        root.path(),
        vec![PathBuf::from("third_party"), PathBuf::from("vm.js")],
        None,
        ProductDeliveryScope::WorkspaceChanges,
        "",
    )
    .expect("gate report");

    assert!(report.report.passed(), "{}", report.output);
    assert!(report.output.contains("Read 1 changed source file"));
    assert!(!report.output.contains("legacy.c"));
}

#[test]
fn nested_rust_target_cannot_be_satisfied_by_unrelated_root_tests() {
    let root = tempfile::tempdir().expect("root");
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"root-crate\"]\nresolver = \"2\"\n",
    )
    .expect("root manifest");
    fs::create_dir_all(root.path().join("root-crate/src")).expect("root crate");
    fs::write(
        root.path().join("root-crate/Cargo.toml"),
        "[package]\nname = \"root-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("manifest");
    fs::write(root.path().join("root-crate/src/lib.rs"), "pub fn ok() -> bool { true }\n")
        .expect("source");
    fs::create_dir_all(root.path().join("game/src")).expect("game");
    fs::write(
        root.path().join("game/Cargo.toml"),
        "[package]\nname = \"game\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("game manifest");
    fs::write(root.path().join("game/src/main.rs"), "fn main() { assert!(true); }\n")
        .expect("game source");

    let report = run_scoped(
        root.path(),
        vec![PathBuf::from("game/Cargo.toml"), PathBuf::from("game/src/main.rs")],
        None,
        ProductDeliveryScope::WorkspaceChanges,
        "",
    )
    .expect("gate report");
    let nested_manifest = PathBuf::from("game").join("Cargo.toml").to_string_lossy().into_owned();
    let root_manifest =
        PathBuf::from("root-crate").join("Cargo.toml").to_string_lossy().into_owned();

    assert!(!report.report.passed());
    assert!(report.output.contains("--manifest-path"));
    assert!(report.output.contains(&nested_manifest));
    assert!(!report.output.contains(&root_manifest));
}

#[test]
fn explicit_nested_output_cannot_be_satisfied_at_the_workspace_root() {
    let root = tempfile::tempdir().expect("root");
    fs::write(
        root.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact marker");
    fs::write(root.path().join("main.py.c"), "int main(void) { return 0; }\n")
        .expect("misplaced candidate");
    let transcript =
        format!("Write a single file in {}/polyglot/main.py.c.", root.path().display());

    let report = run_scoped(
        root.path(),
        vec![PathBuf::from("main.py.c")],
        None,
        ProductDeliveryScope::WorkspaceChanges,
        &transcript,
    )
    .expect("gate report");

    assert!(!report.report.passed());
    assert!(report.output.contains("[Explicit output paths]"));
    assert!(report.output.contains("required explicit output path is missing"));
    assert!(report.output.contains("candidate main.py.c has the requested basename"));
    assert!(report.output.contains("Exact-target acceptance: FAIL"));
}

#[test]
fn standalone_python_source_and_adjacent_readme_pass_exact_target_gate() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("dailylog.py"), "print('ready')\n").expect("Python source");
    fs::write(root.path().join("README.md"), "# Daily log\n").expect("documentation");

    let report = run_scoped(
        root.path(),
        vec![PathBuf::from("README.md"), PathBuf::from("dailylog.py")],
        None,
        ProductDeliveryScope::WorkspaceChanges,
        "",
    )
    .expect("gate report");

    assert!(report.report.passed(), "{}", report.output);
    assert!(report.report.uncovered_paths().is_empty());
    assert!(report.output.contains("[Source readability]"));
    assert!(report.output.contains("[Python compile]"));
    assert!(report.output.contains("Exact-target acceptance: PASS"));
}

#[test]
fn ordinary_csv_and_sqlite_work_without_optional_contracts_remains_unblocked() {
    let artifact = tempfile::tempdir().expect("artifact workspace");
    fs::write(
        artifact.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact manifest");
    fs::write(artifact.path().join("result.csv"), "name,value\nready,1\n").expect("CSV");
    let csv_report = run_scoped_with_cancellation(
        artifact.path(),
        vec![PathBuf::from("result.csv")],
        None,
        ProductDeliveryScope::WorkspaceChanges,
        "",
        "initial database: seed.db\nmigration: migration.sql",
        &GateCancellation::default(),
    )
    .expect("CSV gate report");
    assert!(csv_report.report.passed(), "{}", csv_report.output);
    assert_eq!(csv_report.report.observations().len(), 1);
    assert!(csv_report.report.observations()[0].output.contains("NOT EVALUATED"));
    assert!(csv_report.report.records().iter().all(GateExecutionRecord::passed));

    let sqlite = tempfile::tempdir().expect("SQLite workspace");
    fs::write(sqlite.path().join("schema.sql"), "CREATE TABLE items(id INTEGER);\n")
        .expect("schema");
    fs::write(sqlite.path().join("migration.sql"), "CREATE TABLE notes(id INTEGER);\n")
        .expect("migration");
    let sqlite_report = run_scoped_with_cancellation(
        sqlite.path(),
        vec![PathBuf::from("migration.sql")],
        None,
        ProductDeliveryScope::WorkspaceChanges,
        "",
        "",
        &GateCancellation::default(),
    )
    .expect("SQLite gate report");
    assert!(sqlite_report.report.passed(), "{}", sqlite_report.output);
    assert_eq!(sqlite_report.report.observations().len(), 1);
    assert!(sqlite_report.report.observations()[0].output.contains("NOT EVALUATED"));
    assert!(sqlite_report.report.records().iter().all(GateExecutionRecord::passed));
}

#[test]
fn selected_invalid_csv_contract_still_fails_acceptance() {
    let root = tempfile::tempdir().expect("workspace");
    fs::write(
        root.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact manifest");
    fs::write(root.path().join("result.csv"), "name,value\nready,1\n").expect("CSV");
    let request = "CSV contract for result.csv:\ndelimiter: multiple\nencoding: utf-8\nrow shape: rectangular";

    let report = run_scoped_with_cancellation(
        root.path(),
        vec![PathBuf::from("result.csv")],
        None,
        ProductDeliveryScope::WorkspaceChanges,
        request,
        request,
        &GateCancellation::default(),
    )
    .expect("CSV gate report");

    assert!(!report.report.passed(), "{}", report.output);
    let csv = report
        .report
        .records()
        .iter()
        .find(|record| record.label == "Artifact CSV structure")
        .expect("required CSV record");
    assert_eq!(csv.exit_code, Some(1));
    assert!(csv.output.contains("invalid CSV contract"));
}

#[test]
fn selected_incomplete_sqlite_contract_remains_unsatisfied() {
    let root = tempfile::tempdir().expect("workspace");
    fs::write(root.path().join("schema.sql"), "CREATE TABLE items(id INTEGER);\n").expect("schema");
    fs::write(root.path().join("migration.sql"), "CREATE TABLE notes(id INTEGER);\n")
        .expect("migration");
    let request = "SQLite initial database: seed.db";

    let report = run_scoped_with_cancellation(
        root.path(),
        vec![PathBuf::from("migration.sql")],
        None,
        ProductDeliveryScope::WorkspaceChanges,
        request,
        request,
        &GateCancellation::default(),
    )
    .expect("SQLite gate report");

    assert!(!report.report.passed(), "{}", report.output);
    assert!(report.report.observations().is_empty());
    let sqlite = report
        .report
        .records()
        .iter()
        .find(|record| record.label == "SQLite migration verification")
        .expect("required SQLite record");
    assert_eq!(sqlite.exit_code, None);
    assert!(sqlite.output.contains("NOT EVALUATED"));
}
