//! Real daemon/IPC schedules with a recording provider dependency and disposable state.
#[path = "daemon_build.rs"]
mod build;
#[path = "daemon_fixture.rs"]
mod fixture;
#[path = "daemon_provider.rs"]
mod provider;
#[path = "daemon_scenarios.rs"]
mod scenarios;
use peritus_provider_core::ModelProvider;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, sync::Arc};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Scenario {
    SteerBeforeTool,
    SteerBeforeFinal,
    SteerAtFinalization,
    ModelChange,
    CancelRetry,
    CrashProvider,
    CrashAfterEffect,
    IdleNewRun,
    UnsupportedModel,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    scenario: Scenario,
    text: String,
    edits: u8,
    reconnect: bool,
}
pub fn strategy(_steps: usize) -> BoxedStrategy<Case> {
    (
        proptest::sample::select(vec![
            Scenario::SteerBeforeTool,
            Scenario::SteerBeforeFinal,
            Scenario::SteerAtFinalization,
            Scenario::ModelChange,
            Scenario::CancelRetry,
            Scenario::CrashProvider,
            Scenario::CrashAfterEffect,
            Scenario::IdleNewRun,
            Scenario::UnsupportedModel,
        ]),
        proptest::sample::select(vec![
            "steering α🙂",
            "new exact instruction",
            "a\nb\tμ",
            "changed scope 234",
        ]),
        0u8..3,
        any::<bool>(),
    )
        .prop_map(|(scenario, text, edits, reconnect)| Case {
            scenario,
            text: text.into(),
            edits,
            reconnect,
        })
        .boxed()
}
pub fn providers(root: &Path) -> Option<Vec<Arc<dyn ModelProvider>>> {
    provider::providers(root)
}

pub fn admission_barrier(
    invocation: &str,
    sequence: u32,
    revision: u64,
) -> Result<(), peritus_agent::DeveloperLoopError> {
    boundary(
        "admission",
        &serde_json::json!({"invocation":invocation,"sequence":sequence,"input_revision":revision}),
    )
    .map_err(peritus_agent::DeveloperLoopError::Context)
}
pub fn finalization_barrier() {
    boundary("finalization", &serde_json::json!({"point":"after-runner-before-publication"}))
        .expect("finalization fixture barrier");
}
fn boundary(name: &str, payload: &serde_json::Value) -> Result<(), String> {
    let Some(root) = std::env::var_os("PRAXIS_DAEMON_ROOT") else { return Ok(()) };
    let root = std::path::PathBuf::from(root);
    let enabled = root.join(format!("enable-{name}-barrier"));
    if !enabled.exists() {
        return Ok(());
    }
    fixture::save(&root.join(format!("{name}-ready.json")), payload)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !root.join(format!("release-{name}")).exists() {
        if std::time::Instant::now() > deadline {
            return Err(format!("{name} fixture barrier timed out"));
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    std::fs::remove_file(enabled).map_err(|e| e.to_string())
}
pub fn check(case: &Case) -> Result<BTreeMap<String, u64>, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(scenarios::check(case))
}
pub fn examples() -> Vec<Case> {
    [
        Scenario::SteerBeforeTool,
        Scenario::SteerBeforeFinal,
        Scenario::SteerAtFinalization,
        Scenario::ModelChange,
        Scenario::CancelRetry,
        Scenario::CrashProvider,
        Scenario::CrashAfterEffect,
        Scenario::IdleNewRun,
        Scenario::UnsupportedModel,
    ]
    .into_iter()
    .map(|scenario| Case {
        scenario,
        text: "literal steering μ🙂".into(),
        edits: 2,
        reconnect: true,
    })
    .collect()
}
pub fn calibration_case() -> Case {
    Case {
        scenario: Scenario::SteerBeforeTool,
        text: "new governing input".into(),
        edits: 0,
        reconnect: false,
    }
}
#[test]
fn praxis_stage3_daemon() {
    crate::praxis_support::run("daemon", strategy, check, examples, calibration_case);
}
#[test]
fn praxis_stage3_daemon_child() {
    if let Some(root) = std::env::var_os("PRAXIS_DAEMON_ROOT") {
        fixture::child(Path::new(&root)).expect("daemon fixture child");
    }
}
