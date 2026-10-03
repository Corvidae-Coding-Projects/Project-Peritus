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
