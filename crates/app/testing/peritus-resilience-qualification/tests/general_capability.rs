//! Generic real-process lifecycle qualification fixtures.

use std::{
    io::{BufRead as _, BufReader},
    process::{Child, Command, Stdio},
};

use peritus_obligations::{
    EvidenceBinding, LifecycleEvidence, LifecycleObservationKind, LifecycleRequirement,
    ObligationLimits,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use serde::Deserialize;

const CASES: &str = include_str!("fixtures/general-capability/lifecycle/cases.json");

#[derive(Deserialize)]
struct FixtureSet {
    cases: Vec<FixtureCase>,
}

#[derive(Deserialize)]
struct FixtureCase {
    name: String,
    observation: Observation,
    final_state: FinalState,
    expected: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Observation {
    PublicIngress,
    InternalSimulation,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FinalState {
    Reaped,
    StillRunning,
}

#[test]
fn only_a_real_public_termination_and_observed_exit_satisfy_lifecycle() {
    observe_real_process_termination();
    let requirement = LifecycleRequirement::new(digest(1), digest(2), digest(3), digest(4));
    let fixtures: FixtureSet = serde_json::from_str(CASES).expect("lifecycle fixtures");
    for fixture in fixtures.cases {
        let observation_kind = match fixture.observation {
            Observation::PublicIngress => LifecycleObservationKind::PublicIngress,
            Observation::InternalSimulation => LifecycleObservationKind::InternalSimulation,
        };
        let final_state = match fixture.final_state {
            FinalState::Reaped => digest(4),
            FinalState::StillRunning => digest(5),
        };
        let evidence = LifecycleEvidence::new(
            binding(),
            digest(1),
            digest(2),
            digest(3),
            final_state,
            observation_kind,
        );
        assert_eq!(evidence.satisfies(requirement), fixture.expected, "{}", fixture.name);
    }
}

fn observe_real_process_termination() {
    let mut child = long_lived_child();
    let stdout = child.stdout.take().expect("child stdout");
    let mut ready = String::new();
    BufReader::new(stdout).read_line(&mut ready).expect("read readiness");
    assert_eq!(ready.trim(), "ready");
    #[cfg(unix)]
    terminate(&child);
    #[cfg(windows)]
    terminate(&mut child);
    child.wait().expect("observe and reap child");
    assert!(child.try_wait().expect("reap observation").is_some());
}

#[cfg(unix)]
fn long_lived_child() -> Child {
    Command::new("/bin/sh")
        .args(["-c", "trap 'exit 0' TERM; echo ready; while :; do sleep 1; done"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn lifecycle fixture")
}

#[cfg(windows)]
fn long_lived_child() -> Child {
    Command::new("cmd.exe")
        .args(["/D", "/Q", "/C", "echo ready & ping -n 30 127.0.0.1 >nul"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn lifecycle fixture")
}

#[cfg(unix)]
fn terminate(child: &Child) {
    use nix::{sys::signal, unistd::Pid};

    let raw_pid = i32::try_from(child.id()).expect("child pid");
    signal::kill(Pid::from_raw(raw_pid), signal::Signal::SIGTERM).expect("send public signal");
}

#[cfg(windows)]
fn terminate(child: &mut Child) {
    child.kill().expect("terminate child through public process handle");
}

const fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::new([byte; 32])
}

fn binding() -> EvidenceBinding {
    let candidate = CandidateIdentity::new(
        RunId::new([7; 16]).expect("run id"),
        WorkspaceId::new([8; 16]).expect("workspace id"),
        digest(9),
        1,
        1,
    )
    .expect("candidate");
    EvidenceBinding::new(
        RequirementId::new(digest(10)),
        digest(11),
        candidate,
        digest(12),
        Vec::new(),
        ObligationLimits::production(),
    )
    .expect("evidence binding")
}
