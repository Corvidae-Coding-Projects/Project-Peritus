//! Black-box final H4 bundle reduction from retained signed observations.

#[path = "finalize_operator/support.rs"]
mod support;

use std::fs;

use serde_json::{Value, json};

use support::{finalize, prepare_fixture};

#[test]
fn finalizer_emits_one_ready_no_overwrite_bundle() {
    let root = tempfile::tempdir().expect("H4 fixture root");
    let plan_path = prepare_fixture(root.path());
    let output = root.path().join("final");
    let result = finalize(&plan_path, root.path(), &output).output().expect("finalize");
    let report: Value = serde_json::from_slice(
        &fs::read(output.join("qualification-report.json")).expect("report"),
    )
    .expect("report JSON");
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stderr),
        serde_json::to_string_pretty(&report).expect("render report")
    );
    assert_eq!(report["verdict"], "ready");
    assert_eq!(report["blockers"], json!([]));
    assert!(output.join("evidence-manifest.json").is_file());
    let overwrite = finalize(&plan_path, root.path(), &output).output().expect("overwrite");
    assert!(!overwrite.status.success());
}

#[test]
fn finalizer_rejects_unsigned_cleanup_substitution() {
    let root = tempfile::tempdir().expect("H4 fixture root");
    let plan_path = prepare_fixture(root.path());
    let mut plan: Value =
        serde_json::from_slice(&fs::read(&plan_path).expect("plan bytes")).expect("plan JSON");
    plan["campaigns"][0]["cleanup"]["remaining_processes"] = json!(1);
    let substituted = root.path().join("substituted-plan.json");
    fs::write(&substituted, serde_json::to_vec(&plan).expect("substituted plan"))
        .expect("write substituted plan");
    let output = root.path().join("rejected");
    let result = finalize(&substituted, root.path(), &output).output().expect("finalize");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("differ from the signed payload"));
    assert!(!output.exists());
}
