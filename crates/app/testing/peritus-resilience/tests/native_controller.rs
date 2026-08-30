//! Persistent native H1 controller protocol, evidence, and process ownership checks.

#![cfg(unix)]

use std::fs;
use std::future::Future;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use peritus_resilience::{
    EvidenceDigest, NativeControllerLimits, NativeResilienceFactory, QualificationConfig,
    QualificationRunner, QualificationText, QualificationVerdict, ScenarioCatalog, ScenarioFailure,
    SubjectDescriptor, SubjectId,
};
use sha2::{Digest as _, Sha256};

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
static NATIVE_TEST_GUARD: Mutex<()> = Mutex::new(());

#[test]
fn persistent_controller_qualifies_all_43_fresh_subjects_and_retains_evidence() {
    let _native_test = native_test_guard();
    let fixture = NativeFixture::new(&valid_controller(ABC_SHA256, true));
    let factory = fixture.factory();
    let catalog = ScenarioCatalog::h1_production().expect("built-in H1 catalog");
    let report = block_on(QualificationRunner::run(factory.config(), &catalog, &factory));

    assert_eq!(
        report.verdict(),
        QualificationVerdict::Ready,
        "native H1 failures: {:?}",
        report
            .cases()
            .iter()
            .filter(|case| !case.failures().is_empty())
            .map(|case| (case.scenario().id().as_str(), case.failures()))
            .collect::<Vec<_>>()
    );
    assert_eq!(report.summary().passed(), 43);
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch contents").count(), 0);
    assert_retained_evidence(&fixture.artifacts, 43);
}

#[test]
fn false_retained_digest_fails_recovery_but_each_controller_still_cleans_up() {
    let _native_test = native_test_guard();
    let fixture = NativeFixture::new(&valid_controller(&"0".repeat(64), true));
    let factory = fixture.factory();
    let catalog = ScenarioCatalog::h1_production().expect("built-in H1 catalog");
    let report = block_on(QualificationRunner::run(factory.config(), &catalog, &factory));

    assert!(!report.is_ready());
    assert_eq!(report.summary().failed(), 43);
    assert!(report.cases().iter().all(|case| {
        case.failures().iter().any(|failure| {
            matches!(
                failure,
                ScenarioFailure::Subject { error, .. }
                    if error.context().as_str().contains("evidence digest")
            )
        }) && case.cleanup().is_some_and(peritus_resilience::CleanupObservation::resources_released)
    }));
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch contents").count(), 0);
    assert_retained_evidence(&fixture.artifacts, 43);
}

#[test]
fn stale_prepare_response_is_rejected_and_the_subject_still_cleans_up() {
    let _native_test = native_test_guard();
    let fixture = NativeFixture::new(&valid_controller(ABC_SHA256, false));
    let factory = fixture.factory();
    let catalog = ScenarioCatalog::h1_production().expect("built-in H1 catalog");
    let report = block_on(QualificationRunner::run(factory.config(), &catalog, &factory));

    assert_eq!(report.summary().failed(), 43);
    assert!(report.cases().iter().all(|case| {
        case.failures().iter().any(|failure| {
            matches!(
                failure,
                ScenarioFailure::Subject { error, .. }
                    if error.context().as_str().contains("stale")
            )
        }) && case.cleanup().is_some_and(peritus_resilience::CleanupObservation::resources_released)
    }));
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch contents").count(), 0);
}

#[test]
fn dropping_a_pending_run_kills_the_controller_and_its_descendant() {
    let _native_test = native_test_guard();
    let fixture = NativeFixture::new(&descendant_controller());
    let factory = fixture.factory();
    let catalog = ScenarioCatalog::h1_production().expect("built-in H1 catalog");
    let mut run = Box::pin(QualificationRunner::run(factory.config(), &catalog, &factory));
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    assert!(matches!(run.as_mut().poll(&mut context), Poll::Pending));
    let pid_path = wait_for_descendant(&fixture.scratch);
    let pid = fs::read_to_string(&pid_path)
        .expect("descendant PID")
        .parse::<i32>()
        .expect("numeric descendant PID");
    drop(run);

    assert_process_exited(pid);
    fs::remove_file(pid_path).expect("remove descendant PID record");
    assert_eq!(fs::read_dir(&fixture.scratch).expect("scratch contents").count(), 0);
}

struct NativeFixture {
    _root: tempfile::TempDir,
    scratch: PathBuf,
    artifacts: PathBuf,
    controller: PathBuf,
}

impl NativeFixture {
    fn new(source: &str) -> Self {
        let root = tempfile::tempdir().expect("native H1 fixture root");
        let scratch = root.path().join("scratch");
        let artifacts = root.path().join("artifacts");
        fs::create_dir(&scratch).expect("scratch parent");
        fs::create_dir(&artifacts).expect("artifact parent");
        let controller = write_controller(root.path(), source);
        Self { _root: root, scratch, artifacts, controller }
    }

    fn factory(&self) -> NativeResilienceFactory {
        let candidate_digest = file_digest(&self.controller);
        NativeResilienceFactory::new(
            &self.controller,
            &self.controller,
            &self.scratch,
            &self.artifacts,
            SubjectDescriptor::new(
                SubjectId::new("peritus.release.candidate").expect("subject ID"),
                QualificationText::new("integrated Peritus release candidate")
                    .expect("subject text"),
                candidate_digest,
            ),
            QualificationConfig::default(),
            NativeControllerLimits::default(),
        )
        .expect("native H1 factory")
    }
}

fn file_digest(path: &Path) -> EvidenceDigest {
    let bytes = fs::read(path).expect("read candidate executable");
    EvidenceDigest::from_bytes(Sha256::digest(bytes).into())
}

fn write_controller(parent: &Path, source: &str) -> PathBuf {
    let path = parent.join("h1-controller.sh");
    fs::write(&path, source).expect("write native H1 controller");
    let mut permissions = fs::metadata(&path).expect("controller metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("controller permissions");
    path
}

fn valid_controller(evidence_sha256: &str, bind_prepare_request: bool) -> String {
    let abc = ABC_SHA256;
    let request_sha_command = if bind_prepare_request {
        r#"request_sha=$(printf '%s' "$request" | sed -n 's/.*"request_sha256":"\([^"]*\)".*/\1/p')"#
    } else {
        r#"if [ "$stage" = prepare ]; then
    request_sha=0000000000000000000000000000000000000000000000000000000000000000
  else
    request_sha=$(printf '%s' "$request" | sed -n 's/.*"request_sha256":"\([^"]*\)".*/\1/p')
  fi"#
    };
    format!(
        r#"#!/bin/sh
set -eu
artifact_root=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --artifact-root) artifact_root=$2; shift 2 ;;
    --candidate-executable|--subject-root|--instance-id|--subject-id|--build-sha256|--executor-sha256) shift 2 ;;
    --serve) shift ;;
    *) exit 64 ;;
  esac
done
while IFS= read -r request; do
  stage=$(printf '%s' "$request" | sed -n 's/.*"stage":"\([^"]*\)".*/\1/p')
  sequence=$(printf '%s' "$request" | sed -n 's/.*"sequence":\([0-9]*\).*/\1/p')
  instance=$(printf '%s' "$request" | sed -n 's/.*"instance_id":"\([^"]*\)".*/\1/p')
  scenario=$(printf '%s' "$request" | sed -n 's/.*"scenario":{{"id":"\([^"]*\)".*/\1/p')
  {request_sha_command}
  expected=$(printf '%s' "$request" | sed -n 's/.*"expected_recovery":"\([^"]*\)".*/\1/p')
  case "$stage" in
    prepare)
      payload='{{"terminal":"active","journal_head_sha256":"{abc}"}}'
      ;;
    inject)
      payload='{{"reached":true}}'
      ;;
    recover)
      fault_kind=$(printf '%s' "$request" | sed -n 's/.*"fault":{{"kind":"\([^"]*\)".*/\1/p')
      target=$(printf '%s' "$request" | sed -n 's/.*"target":"\([^"]*\)".*/\1/p')
      dependency=$(printf '%s' "$request" | sed -n 's/.*"dependency":"\([^"]*\)".*/\1/p')
      journal=recovered-and-verified
      artifacts=verified
      projection=verified
      detected=null
      admitted=true
      if [ "$fault_kind" = corruption ]; then
        detected=\"$target\"
        admitted=false
        case "$target" in
          journal) journal=hash-divergence-detected ;;
          projection) projection=rebuilt-and-verified; admitted=true ;;
          *) artifacts=divergence-detected ;;
        esac
      fi
      discovered=0
      failed=0
      candidates=0
      case "$fault_kind" in
        dependency-death|daemon-kill|host-reboot) discovered=1; failed=1; candidates=1 ;;
      esac
      provider=0
      tool=0
      worker=0
      if [ "$fault_kind" = retry-exhaustion ]; then
        case "$dependency" in
          provider) provider=3 ;;
          tool) tool=2 ;;
          worker) worker=2 ;;
        esac
      fi
      for name in fault journal recovery ownership resource final; do
        printf abc > "$artifact_root/$name.json"
      done
      payload='{{"outcome":"'"$expected"'","acceptance":{{"terminal":"failed","revision_bound":false,"evidence_current":false}},"journal":"'"$journal"'","artifacts":"'"$artifacts"'","projection":"'"$projection"'","corruption":{{"detected":'"$detected"',"mutation_admitted":'"$admitted"'}},"ownership":{{"scan_completed":true,"discovered":'"$discovered"',"resumed":0,"failed":'"$failed"',"indeterminate":0,"unaccounted":0,"orphan_candidates_detected":'"$candidates"',"orphans_remaining":0}},"retries":{{"provider":'"$provider"',"tool":'"$tool"',"worker":'"$worker"',"reconciliation":1}},"resources":{{"events":12,"evidence_bytes":18,"peak_owned_processes":2,"cleanup_steps":1,"logical_ticks":50}},"temporary_objects":0,"artifact_count":6,"evidence":[{{"kind":"fault-injection","id":"fault","path":"fault.json","sha256":"{evidence_sha256}","bytes":3}},{{"kind":"journal","id":"journal","path":"journal.json","sha256":"{evidence_sha256}","bytes":3}},{{"kind":"recovery","id":"recovery","path":"recovery.json","sha256":"{evidence_sha256}","bytes":3}},{{"kind":"ownership","id":"ownership","path":"ownership.json","sha256":"{evidence_sha256}","bytes":3}},{{"kind":"resource","id":"resource","path":"resource.json","sha256":"{evidence_sha256}","bytes":3}},{{"kind":"final-state","id":"final","path":"final.json","sha256":"{evidence_sha256}","bytes":3}}],"milestones":[{{"sequence":0,"kind":"prepared","detail":"baseline prepared"}},{{"sequence":1,"kind":"fault-armed","detail":"fault armed"}},{{"sequence":2,"kind":"fault-observed","detail":"fault observed"}},{{"sequence":3,"kind":"recovery-started","detail":"recovery started"}},{{"sequence":4,"kind":"reconciled","detail":"work reconciled"}},{{"sequence":5,"kind":"inspected","detail":"state inspected"}}]}}'
      ;;
    cleanup)
      payload='{{"resources_released":true,"owned_work_remaining":0,"cleanup_steps":1}}'
      ;;
    *) exit 65 ;;
  esac
  printf '{{"schema_version":1,"stage":"%s","sequence":%s,"instance_id":"%s","scenario_id":"%s","request_sha256":"%s","payload":%s}}\n' "$stage" "$sequence" "$instance" "$scenario" "$request_sha" "$payload"
  [ "$stage" = cleanup ] && exit 0
done
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
    --candidate-executable|--artifact-root|--instance-id|--subject-id|--build-sha256|--executor-sha256) shift 2 ;;
    --serve) shift ;;
    *) exit 64 ;;
  esac
done
IFS= read -r request
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
        assert_eq!(fs::read(root.path().join("final.json")).expect("retained evidence"), b"abc");
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
    panic!("native controller descendant {pid} remained after cancellation");
}

struct ThreadWake(thread::Thread);

fn native_test_guard() -> MutexGuard<'static, ()> {
    NATIVE_TEST_GUARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park_timeout(Duration::from_millis(100)),
        }
    }
}
