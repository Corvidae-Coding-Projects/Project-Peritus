//! Native H0 process-boundary, response-binding, and cleanup checks.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use peritus_security_qualification::{
    CancellationToken, HostFingerprint, IntegratedCandidate, NativeProbeFactory,
    QualificationLimits, QualificationRunner,
};
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};

#[test]
fn native_executor_runs_every_case_in_a_fresh_cleaned_subject() {
    let fixture = NativeFixture::new(valid_executor());
    let mut factory = fixture.factory();
    let candidate = candidate(1);
    let run = QualificationRunner
        .run(&mut factory, candidate, QualificationLimits::production(), &CancellationToken::new())
        .expect("canonical H0 run");
    assert!(
        run.all_passed(),
        "native cases failed: {:?}",
        run.cases()
            .iter()
            .filter(|case| case.outcome() != peritus_security_qualification::CaseOutcome::Passed)
            .map(|case| (case.spec().id(), case.failures()))
            .collect::<Vec<_>>()
    );
    assert_eq!(run.cases().len(), 42);
    assert!(run.cases().iter().all(|case| {
        case.observation().is_some_and(|observation| {
            observation.receipt().executor_digest() == digest_file(&fixture.executor)
        }) && case
            .cleanup()
            .is_some_and(peritus_security_qualification::CleanupObservation::complete)
    }));
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch contents").count(), 0);
    assert_retained_artifacts(&fixture.artifacts, 42, b"abc");
}

#[test]
fn response_for_another_request_is_rejected_and_subject_still_cleans() {
    let fixture = NativeFixture::new(mismatched_executor());
    let mut factory = fixture.factory();
    let candidate = candidate(2);
    let cancellation = CancellationToken::new();
    let run = QualificationRunner
        .run(&mut factory, candidate, QualificationLimits::production(), &cancellation)
        .expect("canonical failing run");
    assert!(!run.all_passed());
    assert!(run.cases().iter().all(|case| {
        case.failures().iter().any(|failure| {
            matches!(
                failure,
                peritus_security_qualification::CaseFailure::NativeExecution(error)
                    if error.detail().contains("exact request document")
            )
        }) && case
            .cleanup()
            .is_some_and(peritus_security_qualification::CleanupObservation::complete)
    }));
}

#[test]
fn response_with_a_false_artifact_digest_is_rejected_and_retained() {
    let fixture = NativeFixture::new(tampered_artifact_executor());
    let mut factory = fixture.factory();
    let run = QualificationRunner
        .run(
            &mut factory,
            candidate(4),
            QualificationLimits::production(),
            &CancellationToken::new(),
        )
        .expect("canonical failing run");
    assert!(!run.all_passed());
    let unexpected = run
        .cases()
        .iter()
        .filter(|case| {
            !case.failures().iter().any(|failure| {
                matches!(
                    failure,
                    peritus_security_qualification::CaseFailure::NativeExecution(error)
                        if error.detail().contains("artifact digest")
                )
            }) || !case
                .cleanup()
                .is_some_and(peritus_security_qualification::CleanupObservation::complete)
        })
        .map(|case| (case.spec().id(), case.failures()))
        .collect::<Vec<_>>();
    assert!(unexpected.is_empty(), "unexpected native results: {unexpected:?}");
    assert_retained_artifacts(&fixture.artifacts, 42, b"abc");
}

#[test]
fn cancellation_kills_the_complete_native_process_group() {
    let fixture = NativeFixture::new(descendant_executor());
    let pid_path = fixture.scratch.join("descendant.pid");
    let mut factory = fixture.factory();
    let cancellation = CancellationToken::new();
    let cancellation_request = cancellation.clone();
    let observed_pid = pid_path.clone();
    let canceller = thread::spawn(move || {
        for _ in 0..200 {
            if observed_pid.exists() {
                cancellation_request.cancel();
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        cancellation_request.cancel();
    });
    let started = Instant::now();
    let run = QualificationRunner
        .run(&mut factory, candidate(3), QualificationLimits::production(), &cancellation)
        .expect("cancelled canonical run");
    canceller.join().expect("canceller");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!run.all_passed());
    assert!(
        run.cases()[0]
            .cleanup()
            .is_some_and(peritus_security_qualification::CleanupObservation::complete)
    );
    let pid = fs::read_to_string(&pid_path)
        .expect("descendant PID")
        .parse::<i32>()
        .expect("numeric descendant PID");
    assert_process_exited(pid);
}

struct NativeFixture {
    _root: tempfile::TempDir,
    scratch: PathBuf,
    artifacts: PathBuf,
    executor: PathBuf,
}

impl NativeFixture {
    fn new(source: &str) -> Self {
        let root = tempfile::tempdir().expect("native fixture root");
        let scratch = root.path().join("scratch");
        let artifacts = root.path().join("artifacts");
        fs::create_dir(&scratch).expect("scratch parent");
        fs::create_dir(&artifacts).expect("artifact parent");
        let executor = write_executor(root.path(), source);
        Self { _root: root, scratch, artifacts, executor }
    }

    fn factory(&self) -> NativeProbeFactory {
        NativeProbeFactory::new(
            &self.executor,
            &self.scratch,
            &self.artifacts,
            HostFingerprint::from_document(b"reviewed-linux-host-v1"),
        )
        .expect("native factory")
    }
}

fn write_executor(parent: &std::path::Path, source: &str) -> PathBuf {
    let path = parent.join("native-probe.sh");
    fs::write(&path, source).expect("write native executor");
    let mut permissions = fs::metadata(&path).expect("executor metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("executor permissions");
    path
}

const fn valid_executor() -> &'static str {
    r#"#!/bin/sh
set -eu
response=$4
artifact_root=$8
subject=${10}
request_digest=${12}
printf 'abc' > "$artifact_root/raw.bin"
printf '{"schema_version":1,"subject_id":"%s","request_sha256":"%s","probe_id":"%s","outcome":"passed","native_sandbox_observed":true,"usage":{"elapsed_millis":1,"process_count":1,"peak_memory_bytes":4096,"output_bytes":0,"artifact_count":1},"evidence":[{"kind":"digest","label":"assertion.raw","path":"raw.bin","sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","bytes":3}]}' "$subject" "$request_digest" "$(sed -n 's/.*"probe_id": "\([^"]*\)".*/\1/p' "$2")" > "$response"
"#
}

const fn mismatched_executor() -> &'static str {
    r#"#!/bin/sh
set -eu
probe=$(sed -n 's/.*"probe_id": "\([^"]*\)".*/\1/p' "$2")
printf '{"schema_version":1,"subject_id":"%s","request_sha256":"%064d","probe_id":"%s","outcome":"passed","native_sandbox_observed":true,"usage":{"elapsed_millis":1,"process_count":1,"peak_memory_bytes":4096,"output_bytes":0,"artifact_count":0},"evidence":[{"kind":"fact","label":"assertion.observed","value":true}]}' "${10}" 0 "$probe" > "$4"
"#
}

const fn tampered_artifact_executor() -> &'static str {
    r#"#!/bin/sh
set -eu
probe=$(sed -n 's/.*"probe_id": "\([^"]*\)".*/\1/p' "$2")
printf 'abc' > "$8/raw.bin"
printf '{"schema_version":1,"subject_id":"%s","request_sha256":"%s","probe_id":"%s","outcome":"passed","native_sandbox_observed":true,"usage":{"elapsed_millis":1,"process_count":1,"peak_memory_bytes":4096,"output_bytes":0,"artifact_count":1},"evidence":[{"kind":"digest","label":"assertion.raw","path":"raw.bin","sha256":"%064d","bytes":3}]}' "${10}" "${12}" "$probe" 0 > "$4"
"#
}

const fn descendant_executor() -> &'static str {
    r#"#!/bin/sh
set -eu
sleep 30 &
child=$!
printf '%s' "$child" > "$(dirname "$(dirname "$0")")/descendant.pid"
wait "$child"
"#
}

fn assert_process_exited(pid: i32) {
    for _ in 0..100 {
        if matches!(kill(Pid::from_raw(pid), None), Err(Errno::ESRCH)) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("native probe descendant {pid} remained after cancellation");
}

fn assert_retained_artifacts(parent: &std::path::Path, expected: usize, contents: &[u8]) {
    let roots = fs::read_dir(parent)
        .expect("retained roots")
        .collect::<Result<Vec<_>, _>>()
        .expect("retained root entry");
    assert_eq!(roots.len(), expected);
    for root in roots {
        assert!(root.file_type().expect("retained root type").is_dir());
        assert_eq!(fs::read(root.path().join("raw.bin")).expect("retained artifact"), contents);
    }
}

fn candidate(seed: u8) -> IntegratedCandidate {
    IntegratedCandidate::new(
        RevisionTuple::new(
            AcceptanceSpecId::new([seed; 16]).expect("acceptance"),
            HarnessId::new([seed.wrapping_add(1); 16]).expect("harness"),
            WorkspaceId::new([seed.wrapping_add(2); 16]).expect("workspace"),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([seed.wrapping_add(3); 16]).expect("policy"),
            ProviderProfileId::new([seed.wrapping_add(4); 16]).expect("provider"),
        ),
        Sha256Digest::new([seed; 32]),
        Sha256Digest::new([seed.wrapping_add(10); 32]),
        Sha256Digest::new([seed.wrapping_add(20); 32]),
    )
}

fn digest_file(path: &std::path::Path) -> Sha256Digest {
    peritus_security_qualification::digest_bytes(&fs::read(path).expect("executor bytes"))
}
