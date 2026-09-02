//! Black-box native H0 shard-operator lifecycle.

#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use peritus_security_qualification::{
    IntegratedCandidate, QualificationPlatform, QualificationShard, candidate_json,
};
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};

#[test]
fn native_operator_publishes_one_parseable_linux_shard_without_overwrite() {
    let root = tempfile::tempdir().expect("operator root");
    let scratch = root.path().join("scratch");
    let artifacts = root.path().join("artifacts");
    fs::create_dir(&scratch).expect("scratch");
    fs::create_dir(&artifacts).expect("artifacts");
    let controller = write_controller(root.path(), "passing-controller.sh", controller_source());
    let candidate = root.path().join("candidate.json");
    fs::write(&candidate, candidate_json(integrated_candidate()).expect("candidate JSON"))
        .expect("write candidate");
    let host_facts = root.path().join("host-facts.json");
    fs::write(&host_facts, b"{\"host\":\"test-linux\"}\n").expect("host facts");
    let report = root.path().join("linux-shard.json");

    let first = run_operator(
        &controller,
        root.path(),
        &candidate,
        &host_facts,
        &scratch,
        &artifacts,
        &report,
    );
    assert!(first.status.success());
    let shard = QualificationShard::parse_ready_json(&fs::read(&report).expect("report bytes"))
        .expect("ready shard");
    assert_eq!(shard.platform(), QualificationPlatform::Linux);
    assert_eq!(shard.cases().len(), 40);
    assert!(fs::read_dir(&scratch).expect("scratch entries").next().is_none());
    assert_eq!(fs::read_dir(&artifacts).expect("artifact roots").count(), 40);

    let second = run_operator(
        &controller,
        root.path(),
        &candidate,
        &host_facts,
        &scratch,
        &artifacts,
        &report,
    );
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("report path already exists"));
}

#[test]
fn failed_native_operator_still_publishes_a_diagnostic_shard() {
    let root = tempfile::tempdir().expect("operator root");
    let scratch = root.path().join("scratch");
    let artifacts = root.path().join("artifacts");
    fs::create_dir(&scratch).expect("scratch");
    fs::create_dir(&artifacts).expect("artifacts");
    let controller =
        write_controller(root.path(), "failing-controller.sh", failing_controller_source());
    let candidate = root.path().join("candidate.json");
    fs::write(&candidate, candidate_json(integrated_candidate()).expect("candidate JSON"))
        .expect("write candidate");
    let host_facts = root.path().join("host-facts.json");
    fs::write(&host_facts, b"{\"host\":\"test-linux\"}\n").expect("host facts");
    let report = root.path().join("linux-shard.json");

    let output = run_operator(
        &controller,
        root.path(),
        &candidate,
        &host_facts,
        &scratch,
        &artifacts,
        &report,
    );
    assert!(!output.status.success());
    let report_bytes = fs::read(&report).expect("diagnostic report bytes");
    let report_text = String::from_utf8(report_bytes.clone()).expect("diagnostic report UTF-8");
    assert!(report_text.contains("\"status\": \"failed\""));
    assert!(QualificationShard::parse_ready_json(&report_bytes).is_err());
    assert!(fs::read_dir(&scratch).expect("scratch entries").next().is_none());
}

fn run_operator(
    controller: &Path,
    candidate_root: &Path,
    candidate: &Path,
    host_facts: &Path,
    scratch: &Path,
    artifacts: &Path,
    report: &Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_peritus-h0"))
        .args(["--controller", controller.to_str().expect("controller path")])
        .args(["--candidate", candidate.to_str().expect("candidate path")])
        .args(["--candidate-root", candidate_root.to_str().expect("candidate root path")])
        .args(["--host-facts", host_facts.to_str().expect("host facts path")])
        .args(["--scratch", scratch.to_str().expect("scratch path")])
        .args(["--artifacts", artifacts.to_str().expect("artifact path")])
        .args(["--report", report.to_str().expect("report path")])
        .args(["--platform", "linux"])
        .output()
        .expect("run peritus-h0")
}

fn write_controller(parent: &Path, name: &str, source: &str) -> PathBuf {
    let path = parent.join(name);
    fs::write(&path, source).expect("write controller");
    let mut permissions = fs::metadata(&path).expect("controller metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("controller permissions");
    path
}

const fn controller_source() -> &'static str {
    r#"#!/bin/sh
set -eu
test -d "${14}"
response=$4
artifact_root=$8
subject=${10}
request_digest=${12}
printf 'observed' > "$artifact_root/raw.bin"
probe=$(sed -n 's/.*"probe_id": "\([^"]*\)".*/\1/p' "$2")
printf '{"schema_version":1,"subject_id":"%s","request_sha256":"%s","probe_id":"%s","outcome":"passed","native_sandbox_observed":true,"usage":{"elapsed_millis":1,"process_count":1,"peak_memory_bytes":4096,"output_bytes":0,"artifact_count":1},"evidence":[{"kind":"digest","label":"assertion.raw","path":"raw.bin","sha256":"604cee807f644af47487bf2bbab442b94212ac5119f36f995f78e9e4694dae8c","bytes":8}]}' "$subject" "$request_digest" "$probe" > "$response"
"#
}

const fn failing_controller_source() -> &'static str {
    r#"#!/bin/sh
set -eu
response=$4
subject=${10}
request_digest=${12}
probe=$(sed -n 's/.*"probe_id": "\([^"]*\)".*/\1/p' "$2")
printf '{"schema_version":1,"subject_id":"%s","request_sha256":"%s","probe_id":"%s","outcome":"failed","native_sandbox_observed":true,"usage":{"elapsed_millis":1,"process_count":1,"peak_memory_bytes":4096,"output_bytes":0,"artifact_count":0},"evidence":[{"kind":"fact","label":"assertion.failed","value":true}]}' "$subject" "$request_digest" "$probe" > "$response"
"#
}

fn integrated_candidate() -> IntegratedCandidate {
    IntegratedCandidate::new(
        RevisionTuple::new(
            AcceptanceSpecId::new([1; 16]).expect("acceptance"),
            HarnessId::new([2; 16]).expect("harness"),
            WorkspaceId::new([3; 16]).expect("workspace"),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([4; 16]).expect("policy"),
            ProviderProfileId::new([5; 16]).expect("provider"),
        ),
        Sha256Digest::new([6; 32]),
        Sha256Digest::new([7; 32]),
        Sha256Digest::new([8; 32]),
    )
}
