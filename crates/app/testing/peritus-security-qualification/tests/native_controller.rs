//! Black-box exact-candidate H0 controller protocol checks.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::json;
use sha2::Digest as _;

#[test]
fn controller_reconciles_a_candidate_bound_threat_inventory() {
    let fixture = Fixture::new();
    let output = fixture.run(&fixture.source_sha256);
    assert!(
        output.status.success(),
        "controller stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.response).expect("response bytes"))
            .expect("response JSON");
    assert_eq!(response["outcome"], "passed");
    assert_eq!(response["probe_id"], "h0.inventory.threats");
    assert_eq!(response["native_sandbox_observed"], false);
    assert!(fixture.artifacts.join("probe-evidence.json").is_file());
}

#[test]
fn controller_rejects_a_source_digest_substitution_before_assertions() {
    let fixture = Fixture::new();
    let output = fixture.run(&"ff".repeat(32));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not match"));
    assert!(!fixture.response.exists());
    assert!(fs::read_dir(&fixture.artifacts).expect("artifact root").next().is_none());
}

struct Fixture {
    _root: tempfile::TempDir,
    candidate: PathBuf,
    subject: PathBuf,
    artifacts: PathBuf,
    request: PathBuf,
    response: PathBuf,
    source_sha256: String,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("controller fixture");
        let candidate = root.path().join("candidate");
        let subject = root.path().join("subject");
        let artifacts = root.path().join("artifacts");
        fs::create_dir_all(candidate.join("security")).expect("security directory");
        fs::create_dir(&subject).expect("subject root");
        fs::create_dir(&artifacts).expect("artifact root");
        fs::write(candidate.join("Cargo.toml"), "[workspace]\nresolver = \"3\"\n")
            .expect("candidate manifest");
        fs::write(candidate.join("architecture.toml"), "schema = 3\n")
            .expect("architecture policy");
        fs::write(
            candidate.join("security/threat-model-v1.toml"),
            include_bytes!("../../../../../security/threat-model-v1.toml"),
        )
        .expect("threat inventory");
        git(&candidate, &["init"]);
        git(&candidate, &["config", "user.email", "h0-controller@example.invalid"]);
        git(&candidate, &["config", "user.name", "H0 Controller Test"]);
        git(&candidate, &["add", "."]);
        git(&candidate, &["commit", "-m", "fixture"]);
        let source_sha256 = source_digest(&candidate);
        let request = subject.join("request.json");
        let response = subject.join("response.json");
        Self { _root: root, candidate, subject, artifacts, request, response, source_sha256 }
    }

    fn run(&self, source_sha256: &str) -> std::process::Output {
        let request = json!({
            "schema_version": 1,
            "subject_id": "h0-controller-test",
            "probe_id": "h0.inventory.threats",
            "target": "tier-one-host",
            "candidate": {
                "acceptance_spec_id": "01".repeat(16),
                "harness_id": "02".repeat(16),
                "workspace_id": "03".repeat(16),
                "workspace_generation": 1,
                "workspace_revision": 1,
                "policy_id": "04".repeat(16),
                "provider_profile_id": "05".repeat(16),
                "source_sha256": source_sha256,
                "release_manifest_sha256": "07".repeat(32),
                "qualification_plan_sha256": "08".repeat(32)
            },
            "limits": {
                "duration_millis": 300_000,
                "processes": 64,
                "peak_memory_bytes": 4_u64 * 1024 * 1024 * 1024,
                "output_bytes": 64_u64 * 1024 * 1024,
                "artifacts": 256
            }
        });
        let bytes = serde_json::to_vec_pretty(&request).expect("request JSON");
        fs::write(&self.request, &bytes).expect("request file");
        let request_sha256 = hex(sha2::Sha256::digest(&bytes).into());
        Command::new(env!("CARGO_BIN_EXE_peritus-h0-controller"))
            .args(["--request", text(&self.request)])
            .args(["--response", text(&self.response)])
            .args(["--subject-root", text(&self.subject)])
            .args(["--artifact-root", text(&self.artifacts)])
            .args(["--subject-id", "h0-controller-test"])
            .args(["--request-sha256", &request_sha256])
            .args(["--candidate-root", text(&self.candidate)])
            .output()
            .expect("run H0 controller")
    }
}

fn git(root: &Path, arguments: &[&str]) {
    let output =
        Command::new("git").current_dir(root).args(arguments).output().expect("run fixture git");
    assert!(output.status.success(), "git stderr: {}", String::from_utf8_lossy(&output.stderr));
}

fn source_digest(root: &Path) -> String {
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["archive", "--format=tar", "HEAD"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("start git archive");
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    let stdout = child.stdout.as_mut().expect("git stdout");
    loop {
        let count = stdout.read(&mut buffer).expect("read git archive");
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    assert!(child.wait().expect("wait git archive").success());
    hex(hasher.finalize().into())
}

fn hex(bytes: [u8; 32]) -> String {
    use std::fmt::Write as _;

    let mut value = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(&mut value, "{byte:02x}");
    }
    value
}

fn text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 fixture path")
}
