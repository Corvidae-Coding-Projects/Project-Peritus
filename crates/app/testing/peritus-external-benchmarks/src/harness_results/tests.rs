use std::{ffi::OsString, fs};

use serde_json::{Value, json};
use tempfile::TempDir;

use super::run_cli;

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn publishes_complete_selection_manifest_and_aggregate() {
    let fixture = Fixture::new(&["001-alpha", "002-beta"]);
    fixture.write_result("model-a", "001-alpha", 1.0, 0.8, 0.8, 2, 20, 10, false);
    fixture.write_result("model-b", "001-alpha", 1.0, 0.8, 0.8, 2, 20, 10, false);
    fixture.write_result("model-a", "002-beta", 0.5, 0.6, 0.3, 1, 8, 4, false);

    run_cli(fixture.arguments("allow-legacy")).expect("publish report");
    let report: Value = serde_json::from_slice(&fs::read(&fixture.output).unwrap()).unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(report["selected_tasks"], 2);
    assert_eq!(report["aggregate"]["adapter_successes"], 2);
    assert_eq!(report["aggregate"]["outcome_mean"], 0.75);
    assert_eq!(report["aggregate"]["process_mean"], 0.7);
    assert_eq!(report["aggregate"]["security_mean"], 1.0);
    assert_eq!(report["aggregate"]["combined_mean"], 0.55);
    assert_eq!(report["aggregate"]["request_count"], 3);
    assert_eq!(report["aggregate"]["total_tokens"], 42);
    assert_eq!(report["tasks"][0]["candidate_results"], 2);
    assert_eq!(report["agent"]["native_invocations_with_identity"], 0);
    assert_eq!(report["tasks"][0]["result_sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn strict_identity_requires_and_retains_native_build_identity() {
    let missing = Fixture::new(&["001-alpha"]);
    missing.write_result("model", "001-alpha", 1.0, 1.0, 1.0, 1, 5, 2, false);
    let error = run_cli(missing.arguments("require-native")).unwrap_err().to_string();
    assert!(error.contains("has no native build identity"));
    assert!(!missing.output.exists());

    let present = Fixture::new(&["001-alpha"]);
    present.write_result("model", "001-alpha", 1.0, 1.0, 1.0, 1, 5, 2, true);
    run_cli(present.arguments("require-native")).expect("strict report");
    let report: Value = serde_json::from_slice(&fs::read(&present.output).unwrap()).unwrap();
    assert_eq!(report["agent"]["source_revisions"][0], REVISION);
    assert_eq!(report["agent"]["binary_sha256s"][0], DIGEST);
}

#[test]
fn rejects_incomplete_catalog_coverage() {
    let fixture = Fixture::new(&["001-alpha", "002-beta"]);
    fixture.write_result("model", "001-alpha", 1.0, 1.0, 1.0, 1, 5, 2, false);
    let error = run_cli(fixture.arguments("allow-legacy")).unwrap_err().to_string();
    assert!(error.contains("missing=[\"002-beta\"]"));
    assert!(!fixture.output.exists());
}

struct Fixture {
    _temporary: TempDir,
    campaign: std::path::PathBuf,
    catalog: std::path::PathBuf,
    pin: std::path::PathBuf,
    output: std::path::PathBuf,
    expected_tasks: usize,
}

impl Fixture {
    fn new(task_ids: &[&str]) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let campaign = temporary.path().join("campaign");
        let catalog = temporary.path().join("tasks");
        fs::create_dir_all(campaign.join("results")).unwrap();
        fs::create_dir_all(&catalog).unwrap();
        for task_id in task_ids {
            fs::create_dir(catalog.join(task_id)).unwrap();
        }
        let pin = temporary.path().join("pin.toml");
        fs::write(&pin, "revision = 'pinned'\n").unwrap();
        let output = temporary.path().join("report.json");
        Self {
            _temporary: temporary,
            campaign,
            catalog,
            pin,
            output,
            expected_tasks: task_ids.len(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_result(
        &self,
        model: &str,
        task_id: &str,
        outcome: f64,
        process: f64,
        combined: f64,
        requests: u64,
        input: u64,
        output: u64,
        with_identity: bool,
    ) {
        let sandbox = self.campaign.join("workspaces").join(task_id);
        let workspace = sandbox.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let usage_log = sandbox.join("usage-proxy/requests.jsonl");
        fs::create_dir_all(usage_log.parent().unwrap()).unwrap();
        fs::write(&usage_log, "").unwrap();
        let prompt = sandbox.join("prompt.txt");
        fs::write(&prompt, "task").unwrap();
        let result = json!({
            "task_id": task_id,
            "elapsed_sec": 2.0,
            "mode": "live",
            "model_id": "peritus",
            "api_model_slug": model,
            "api_model_label": model,
            "session_id": format!("session-{task_id}"),
            "prompt_file": prompt,
            "workspace": workspace,
            "adapter_result": {"ok": true},
            "usage_summary": {
                "available": true,
                "source": "proxy",
                "log_file": usage_log,
                "request_count": requests,
                "input_tokens": input,
                "output_tokens": output,
                "cache_read_tokens": 3,
                "cache_write_tokens": 4,
                "total_tokens": input + output,
                "providers": ["normalized"],
                "models": [model]
            },
            "scoring": {
                "outcome_score": outcome,
                "process_score": process,
                "security_score": 1.0,
                "combined_score": combined
            }
        });
        let path =
            self.campaign.join("results/peritus").join(model).join(format!("{task_id}.json"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        let invocation = json!({
            "task_id": task_id,
            "session_id": format!("session-{task_id}"),
            "agent_identity": with_identity.then(|| json!({
                "package_version": "0.1.0",
                "source_revision": REVISION,
                "binary_sha256": DIGEST
            }))
        });
        let invocation_path = sandbox.join("peritus-benchmark/invocation.json");
        fs::create_dir_all(invocation_path.parent().unwrap()).unwrap();
        fs::write(invocation_path, serde_json::to_vec_pretty(&invocation).unwrap()).unwrap();
    }

    fn arguments(&self, identity_policy: &str) -> Vec<OsString> {
        [
            OsString::from("report"),
            OsString::from("--campaign-dir"),
            self.campaign.as_os_str().to_owned(),
            OsString::from("--task-catalog"),
            self.catalog.as_os_str().to_owned(),
            OsString::from("--output"),
            self.output.as_os_str().to_owned(),
            OsString::from("--pin-file"),
            self.pin.as_os_str().to_owned(),
            OsString::from("--expected-tasks"),
            OsString::from(self.expected_tasks.to_string()),
            OsString::from("--campaign-label"),
            OsString::from("test-campaign"),
            OsString::from("--identity-policy"),
            OsString::from(identity_policy),
        ]
        .into_iter()
        .collect()
    }
}
