use std::{fs, path::Path, process::Command};

use serde::Deserialize;

use crate::{BenchmarkError, candidate, workspace};

use super::fixture::Expected;

const CASES: &str = include_str!("../../tests/fixtures/general-capability/repository/cases.json");

#[derive(Deserialize)]
struct RepositoryFixtures {
    inventory_files: usize,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    expected: Expected,
}

#[test]
fn repository_inventory_and_drift_are_observed_without_rewriting_state() {
    let fixtures: RepositoryFixtures = serde_json::from_str(CASES).expect("repository fixtures");
    assert_case_shape(&fixtures.cases);
    let root = tempfile::tempdir().expect("workspace");
    let source = root.path().join("src");
    fs::create_dir(&source).expect("source directory");
    for index in 0..fixtures.inventory_files {
        fs::write(source.join(format!("item-{index:03}.txt")), format!("item {index}\n"))
            .expect("inventory file");
    }
    let nested = root.path().join("vendor/nested");
    fs::create_dir_all(&nested).expect("nested repository");
    fs::write(nested.join("README.md"), "nested\n").expect("nested file");
    initialize_repository(&nested);
    let nested_head = output(&nested, &["rev-parse", "HEAD"]);

    let baseline = workspace::prepare(root.path()).expect("large baseline");
    assert_eq!(output(&nested, &["rev-parse", "HEAD"]), nested_head);
    fs::write(source.join("item-127.txt"), "authorized update\n").expect("authorized edit");
    git(root.path(), &["add", "src/item-127.txt"]);
    commit(root.path(), "authorized task commit");
    let head_candidate =
        candidate::capture(root.path(), Some(&baseline.head)).expect("head change");
    assert_eq!(head_candidate.changed_paths, [Path::new("src/item-127.txt")]);

    fs::write(root.path().join("external-drift.log"), "external observation\n")
        .expect("external drift");
    let drift = candidate::capture(root.path(), Some(&baseline.head)).expect("drift capture");
    assert!(drift.changed_paths.contains(&Path::new("src/item-127.txt").to_owned()));
    assert!(drift.changed_paths.contains(&Path::new("external-drift.log").to_owned()));

    let error =
        candidate::capture(root.path(), Some(&"f".repeat(40))).expect_err("unknown baseline");
    assert!(matches!(error, BenchmarkError::Command { .. }));
}

fn assert_case_shape(cases: &[Case]) {
    assert_eq!(cases.len(), 3);
    assert_eq!(cases[0].expected, Expected::Success);
    assert_eq!(cases[1].expected, Expected::Partial);
    assert_eq!(cases[2].expected, Expected::Failure);
    assert!(cases.iter().all(|case| !case.name.is_empty()));
}

fn initialize_repository(root: &Path) {
    git(root, &["init", "--quiet"]);
    git(root, &["add", "."]);
    commit(root, "nested baseline");
}

fn commit(root: &Path, message: &str) {
    git(
        root,
        &[
            "-c",
            "user.name=Peritus Test",
            "-c",
            "user.email=peritus-test@localhost",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    );
}

fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("git").args(arguments).current_dir(root).output().expect("git");
    assert!(output.status.success(), "git failed: {}", String::from_utf8_lossy(&output.stderr));
}

fn output(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git").args(arguments).current_dir(root).output().expect("git");
    assert!(output.status.success(), "git failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("git output").trim().to_owned()
}
