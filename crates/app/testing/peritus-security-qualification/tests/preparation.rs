//! Black-box reproducible H0 candidate and native-host preparation.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use peritus_security_qualification::{QualificationPlatform, parse_candidate_json};

#[test]
fn preparation_binds_committed_source_and_refuses_dirty_or_overwritten_outputs() {
    let temporary = tempfile::tempdir().expect("preparation root");
    let repository = temporary.path().join("candidate");
    fs::create_dir(&repository).expect("candidate root");
    write_candidate_tree(&repository);
    git(&repository, &["init", "--quiet"]);
    git(&repository, &["config", "user.email", "h0-preparation@example.invalid"]);
    git(&repository, &["config", "user.name", "H0 Preparation Test"]);
    git(&repository, &["config", "core.autocrlf", "false"]);
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "--quiet", "-m", "candidate"]);
    let controller = temporary.path().join(controller_name());
    fs::write(&controller, b"reviewed-controller\n").expect("controller");

    let first = temporary.path().join("first");
    let output = prepare(&repository, &controller, &first);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let candidate_bytes = fs::read(first.join("candidate.json")).expect("candidate document");
    let candidate = parse_candidate_json(&candidate_bytes).expect("prepared candidate");
    assert!(candidate.source_digest().into_bytes().iter().any(|byte| *byte != 0));
    let host: serde_json::Value =
        serde_json::from_slice(&fs::read(first.join("host-facts.json")).expect("host facts"))
            .expect("host JSON");
    assert_eq!(host["schema_version"], 1);
    assert_eq!(host["platform"], platform().as_str());
    assert_eq!(host["candidate_source_sha256"], hex(candidate.source_digest().into_bytes()));
    assert!(first.join("scratch").is_dir());
    assert!(first.join("artifacts").is_dir());

    let collision = prepare(&repository, &controller, &first);
    assert!(!collision.status.success());
    assert!(String::from_utf8_lossy(&collision.stderr).contains("already exists"));

    let second = temporary.path().join("second");
    assert!(prepare(&repository, &controller, &second).status.success());
    assert_eq!(candidate_bytes, fs::read(second.join("candidate.json")).expect("second candidate"));

    fs::write(repository.join("architecture.toml"), "changed = true\n").expect("dirty source");
    let dirty = prepare(&repository, &controller, &temporary.path().join("dirty"));
    assert!(!dirty.status.success());
    assert!(String::from_utf8_lossy(&dirty.stderr).contains("differs from its committed"));
}

fn prepare(repository: &Path, controller: &Path, output: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_peritus-h0-prepare"))
        .args(["--candidate-root", repository.to_str().expect("candidate path")])
        .args(["--controller", controller.to_str().expect("controller path")])
        .args(["--output", output.to_str().expect("output path")])
        .args(["--platform", platform().as_str()])
        .output()
        .expect("run H0 preparation")
}

fn write_candidate_tree(root: &Path) {
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = []\n[workspace.package]\nrepository = \"https://github.com/Corvidae-Coding-Projects/Project-Peritus\"\n",
    );
    for path in [
        ".design/peritus-production-architecture.md",
        ".design/b2-acceptance-specification.md",
        ".github/workflows/release.yml",
        "Cargo.lock",
        "architecture.toml",
        "docs/h0-security-qualification.md",
        "install.ps1",
        "install.sh",
        "rust-toolchain.toml",
        "xtask/src/product_package.rs",
        "xtask/src/release.rs",
    ] {
        write(root, path, &format!("fixture for {path}\n"));
    }
    for directory in [
        "crates/app/peritus-cli",
        "crates/app/peritus-daemon",
        "crates/app/peritus-launcher",
        "crates/app/peritus-tui",
        "crates/app/testing/peritus-security-qualification",
        "crates/foundation/peritus-policy",
        "crates/foundation/peritus-quality-policy",
        "crates/foundation/peritus-security-policy",
        "crates/foundation/peritus-spec",
        "crates/model",
        "crates/orchestration/peritus-agent",
        "crates/orchestration/peritus-harness",
        "crates/orchestration/peritus-orchestrator",
        "release",
        "security",
        "xtask/src/product_package",
    ] {
        write(root, &format!("{directory}/fixture.txt"), directory);
    }
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
    fs::write(path, contents).expect("fixture file");
}

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("git").current_dir(root).args(arguments).status().expect("run Git");
    assert!(status.success(), "git {arguments:?}");
}

const fn platform() -> QualificationPlatform {
    QualificationPlatform::current().expect("test runs on a supported H0 platform")
}

fn controller_name() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("peritus-h0-controller.exe")
    } else {
        PathBuf::from("peritus-h0-controller")
    }
}

fn hex(bytes: [u8; 32]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}
