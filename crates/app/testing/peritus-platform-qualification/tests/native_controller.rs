//! Native H2 controller, retained evidence, and process ownership checks.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use peritus_platform_qualification::{
    Architecture, ArtifactRole, FreshSubjectRunner, InstallPath, ManifestArtifact,
    NativeControllerLimits, NativePlatformFactory, PackageManifest, PackageVersion, Platform,
    PlatformVersion, QualificationReport, QualificationTarget, ReadinessVerdict,
    RelativePackagePath, ReleaseLayout, ScenarioId, digest_bytes,
};

#[test]
fn native_controller_qualifies_all_18_fresh_subjects_and_retains_raw_evidence() {
    let fixture = NativeFixture::new(Binding::Exact, NativeControllerLimits::default());
    let (target, manifest) = fixture.manifest();
    let mut factory = fixture.factory();
    let run = FreshSubjectRunner.run(&mut factory, target, &manifest).expect("native H2 run");
    let report = QualificationReport::evaluate(run);

    assert!(matches!(report.verdict(), ReadinessVerdict::Ready(_)));
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch").count(), 0);
    assert_retained_evidence(&fixture.artifacts, ScenarioId::all().len());
}

#[test]
fn one_command_operator_runs_all_scenarios_and_publishes_a_bound_report() {
    let fixture = NativeFixture::new(Binding::Exact, NativeControllerLimits::default());
    let (_, manifest) = fixture.manifest();
    let manifest_path = fixture.root.path().join("manifest.toml");
    let report_path = fixture.root.path().join("h2-report.json");
    fs::write(&manifest_path, manifest.canonical_bytes()).expect("manifest file");

    let status =
        operator_command(&fixture, &manifest_path, &report_path).status().expect("run peritus-h2");

    assert!(status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(&report_path).expect("report bytes"))
            .expect("report JSON");
    assert_eq!(report["verdict"]["status"], "ready");
    assert_eq!(report["manifest_sha256"], manifest.digest().to_hex());
    assert_eq!(report["scenarios"].as_array().expect("scenarios").len(), 18);
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch").count(), 0);
    assert_retained_evidence(&fixture.artifacts, 18);
}

#[test]
fn scenario_shard_runs_exactly_three_fresh_subjects_and_retains_each_report() {
    let fixture = NativeFixture::new(Binding::Exact, NativeControllerLimits::default());
    let (_, manifest) = fixture.manifest();
    let manifest_path = fixture.root.path().join("manifest.toml");
    let report_path = fixture.root.path().join("h2-shard-4");
    fs::write(&manifest_path, manifest.canonical_bytes()).expect("manifest file");

    let status = operator_command(&fixture, &manifest_path, &report_path)
        .args(["--shard", "4"])
        .status()
        .expect("run peritus-h2 shard");

    assert!(status.success());
    let reports = fs::read_dir(&report_path)
        .expect("shard reports")
        .map(|entry| entry.expect("report entry").path())
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 3);
    for report in reports {
        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(report).expect("report")).expect("report JSON");
        assert_eq!(document["kind"], "h2-scenario-shard");
        assert_eq!(document["verdict"], "ready");
        assert_eq!(document["manifest_sha256"], manifest.digest().to_hex());
    }
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch").count(), 0);
    assert_retained_evidence(&fixture.artifacts, 3);
}

fn operator_command(fixture: &NativeFixture, manifest: &Path, report: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_peritus-h2"));
    command.args([
        "--controller",
        fixture.controller.to_str().expect("controller path"),
        "--package",
        fixture.package.to_str().expect("package path"),
        "--manifest",
        manifest.to_str().expect("manifest path"),
        "--scratch",
        fixture.scratch.to_str().expect("scratch path"),
        "--artifacts",
        fixture.artifacts.to_str().expect("artifact path"),
        "--report",
        report.to_str().expect("report path"),
        "--platform",
        "linux",
        "--architecture",
        "x86_64",
        "--version",
        "6.6.0",
    ]);
    command
}

#[test]
fn false_retained_digest_is_rejected_but_the_fresh_subject_is_removed() {
    let fixture = NativeFixture::new(Binding::FalseDigest, NativeControllerLimits::default());
    let (target, manifest) = fixture.manifest();
    let mut factory = fixture.factory();
    let error = FreshSubjectRunner
        .run(&mut factory, target, &manifest)
        .expect_err("false artifact digest must fail");

    assert!(error.detail().contains("digest"));
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch").count(), 0);
    assert_retained_evidence(&fixture.artifacts, 1);
}

#[test]
fn stale_scenario_response_is_rejected_while_independent_cleanup_still_passes() {
    let fixture = NativeFixture::new(Binding::StaleResponse, NativeControllerLimits::default());
    let (target, manifest) = fixture.manifest();
    let mut factory = fixture.factory();
    let error = FreshSubjectRunner
        .run(&mut factory, target, &manifest)
        .expect_err("stale response must fail");

    assert!(error.detail().contains("exact request"));
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch").count(), 0);
    assert_retained_evidence(&fixture.artifacts, 1);
}

#[test]
fn deadline_terminates_the_controller_and_its_descendant() {
    let limits = NativeControllerLimits::new(
        Duration::from_millis(200),
        64 * 1024,
        32 * 1024,
        1024 * 1024,
        1024 * 1024,
        8,
    )
    .expect("limits");
    let fixture = NativeFixture::new(Binding::Descendant, limits);
    let (target, manifest) = fixture.manifest();
    let mut factory = fixture.factory();
    let error = FreshSubjectRunner
        .run(&mut factory, target, &manifest)
        .expect_err("controller deadline must fail");

    assert!(matches!(
        error.code(),
        peritus_platform_qualification::QualificationErrorCode::NativeExecution
    ));
    let pid_path = wait_for_descendant(&fixture.scratch);
    let pid = fs::read_to_string(&pid_path)
        .expect("descendant PID")
        .parse::<i32>()
        .expect("numeric descendant PID");
    assert_process_exited(pid);
    fs::remove_file(pid_path).expect("remove descendant PID record");
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch").count(), 0);
}

#[derive(Clone, Copy)]
enum Binding {
    Exact,
    FalseDigest,
    StaleResponse,
    Descendant,
}

struct NativeFixture {
    root: tempfile::TempDir,
    package: PathBuf,
    scratch: PathBuf,
    artifacts: PathBuf,
    controller: PathBuf,
    limits: NativeControllerLimits,
}

impl NativeFixture {
    fn new(binding: Binding, limits: NativeControllerLimits) -> Self {
        let root = tempfile::tempdir().expect("native H2 fixture root");
        let package = root.path().join("package");
        let scratch = root.path().join("scratch");
        let artifacts = root.path().join("artifacts");
        fs::create_dir(&package).expect("package source");
        fs::create_dir(&scratch).expect("scratch parent");
        fs::create_dir(&artifacts).expect("artifact parent");
        let controller = write_controller(root.path(), &controller_source(binding));
        Self { root, package, scratch, artifacts, controller, limits }
    }

    fn factory(&self) -> NativePlatformFactory {
        NativePlatformFactory::new(
            &self.controller,
            &self.package,
            &self.scratch,
            &self.artifacts,
            self.limits,
        )
        .expect("native H2 factory")
    }

    fn manifest(&self) -> (QualificationTarget, PackageManifest) {
        let target = QualificationTarget::new(
            Platform::Linux,
            Architecture::X86_64,
            PlatformVersion::new(6, 6, 0, 0),
        );
        let home = InstallPath::new(Platform::Linux, "/home/h2-subject").expect("home");
        let layout = ReleaseLayout::production(Platform::Linux, &home).expect("layout");
        let roles = [
            (ArtifactRole::Daemon, "bin/peritusd", true),
            (ArtifactRole::Cli, "bin/peritus", true),
            (ArtifactRole::Tui, "bin/peritus-tui", true),
            (ArtifactRole::SandboxHelper, "libexec/peritus-linux-sandbox-helper", true),
            (ArtifactRole::ServiceDefinition, "share/peritus/peritus.service", false),
            (ArtifactRole::Installer, "Install-Peritus.sh", true),
            (ArtifactRole::Uninstaller, "Uninstall-Peritus.sh", true),
            (ArtifactRole::Upgrader, "Upgrade-Peritus.sh", true),
        ];
        let artifacts = roles
            .into_iter()
            .map(|(role, path, executable)| {
                let bytes = format!("fixture:{path}\n").into_bytes();
                let destination = self.package.join(path);
                fs::create_dir_all(destination.parent().expect("artifact parent"))
                    .expect("artifact parent");
                fs::write(&destination, &bytes).expect("artifact bytes");
                ManifestArtifact::new(
                    role,
                    RelativePackagePath::new(path).expect("path"),
                    digest_bytes(&bytes),
                    executable,
                )
                .expect("manifest artifact")
            })
            .collect();
        let manifest = PackageManifest::new(
            PackageVersion::new("0.1.0").expect("version"),
            Platform::Linux,
            Architecture::X86_64,
            layout.digest(),
            artifacts,
        )
        .expect("manifest");
        (target, manifest)
    }
}

fn write_controller(parent: &Path, source: &str) -> PathBuf {
    let path = parent.join("h2-controller.sh");
    fs::write(&path, source).expect("write native H2 controller");
    let mut permissions = fs::metadata(&path).expect("controller metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("controller permissions");
    path
}

fn controller_source(binding: Binding) -> String {
    if matches!(binding, Binding::Descendant) {
        return descendant_controller();
    }
    let scenario_sha = digest_bytes(b"abc").sha256().to_hex();
    let reported_sha =
        if matches!(binding, Binding::FalseDigest) { "0".repeat(64) } else { scenario_sha };
    let cleanup_sha = digest_bytes(b"clean").sha256().to_hex();
    let response_binding = if matches!(binding, Binding::StaleResponse) {
        "0000000000000000000000000000000000000000000000000000000000000000".to_owned()
    } else {
        "$request_sha".to_owned()
    };
    format!(
        r#"#!/bin/sh
set -eu
response=
cleanup=
artifact_root=
request_file=
request_sha=
subject=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --request) request_file=$2; shift 2 ;;
    --response) response=$2; shift 2 ;;
    --cleanup-response) cleanup=$2; shift 2 ;;
    --artifact-root) artifact_root=$2; shift 2 ;;
    --subject-id) subject=$2; shift 2 ;;
    --request-sha256) request_sha=$2; shift 2 ;;
    --subject-root|--package-root) shift 2 ;;
    *) exit 64 ;;
  esac
done
scenario=$(sed -n 's/^[[:space:]]*"id": "\([^"]*\)",*/\1/p' "$request_file" | head -n 1)
printf abc > "$artifact_root/scenario.json"
printf clean > "$artifact_root/cleanup.json"
printf '{{"schema_version":1,"subject_id":"%s","scenario_id":"%s","request_sha256":"%s","outcome":"passed","artifact_count":1,"evidence":[{{"kind":"fact","label":"scenario.observed","value":true}},{{"kind":"digest","label":"scenario.raw","path":"scenario.json","sha256":"{reported_sha}","bytes":3}}]}}' "$subject" "$scenario" "{response_binding}" > "$response"
printf '{{"schema_version":1,"subject_id":"%s","scenario_id":"%s","request_sha256":"%s","complete":true,"remaining_resources":0,"evidence":{{"path":"cleanup.json","sha256":"{cleanup_sha}","bytes":5}}}}' "$subject" "$scenario" "$request_sha" > "$cleanup"
"#,
    )
}

fn descendant_controller() -> String {
    r#"#!/bin/sh
set -eu
subject_root=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --subject-root) subject_root=$2; shift 2 ;;
    --request|--response|--cleanup-response|--package-root|--artifact-root|--subject-id|--request-sha256) shift 2 ;;
    *) exit 64 ;;
  esac
done
sleep 30 &
child=$!
printf '%s' "$child" > "$(dirname "$subject_root")/descendant.pid"
wait "$child"
"#
    .to_owned()
}

fn assert_retained_evidence(parent: &Path, expected_roots: usize) {
    let roots = fs::read_dir(parent)
        .expect("retained roots")
        .collect::<Result<Vec<_>, _>>()
        .expect("retained root entry");
    assert_eq!(roots.len(), expected_roots);
    for root in roots {
        assert!(root.file_type().expect("retained root type").is_dir());
        assert_eq!(fs::read(root.path().join("scenario.json")).expect("scenario evidence"), b"abc");
        assert_eq!(fs::read(root.path().join("cleanup.json")).expect("cleanup evidence"), b"clean");
    }
}

fn wait_for_descendant(scratch: &Path) -> PathBuf {
    let path = scratch.join("descendant.pid");
    for _ in 0..200 {
        if path.exists() {
            return path;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("controller descendant PID was not recorded");
}

fn assert_process_exited(pid: i32) {
    for _ in 0..100 {
        if matches!(kill(Pid::from_raw(pid), None), Err(Errno::ESRCH)) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("native controller descendant {pid} remained after deadline");
}
